use futures_util::StreamExt;
use tracing::{info, instrument, warn};
use zbus::DBusError;
use zbus::names::BusName;


/// Waits until the given object appears on the bus.
#[instrument(skip(dbus_conn))]
pub async fn wait_for_object(
    dbus_conn: &zbus::connection::Connection,
    object_name: BusName<'_>,
) {
    // first register the change listener, then ask about an existing owner;
    // this prevents a race condition

    // talk to the bus manager
    let dbus_proxy = zbus::fdo::DBusProxy::new(&dbus_conn)
        .await.expect("failed to create D-Bus API proxy");

    // register the change listener
    let mut new_kid_on_the_block_stream = dbus_proxy.receive_name_owner_changed_with_args(&[
        (0, object_name.as_str()),
    ])
        .await.expect("failed to obtain stream waiting for name owner change");

    // ask if there (already) is someone there
    let name_owner_res = dbus_proxy
        .get_name_owner(object_name.clone()).await;
    match name_owner_res {
        Ok(_) => {
            // nothing to wait for :-)
            return;
        },
        Err(e) if e.name() == "org.freedesktop.DBus.Error.NameHasNoOwner" => {
            // we shall wait
        },
        Err(e) => panic!("failed to query {} name owner: {}", object_name.as_str(), e),
    }

    warn!("no one is currently offering that name; I have to be patient");

    loop {
        let new_kid = new_kid_on_the_block_stream
            .next().await.expect("new-owner stream ended");
        let new_kid_args = new_kid
            .args().expect("failed to obtain new-owner event args");
        if new_kid_args.name() != "org.freedesktop.secrets" {
            continue;
        }
        if new_kid_args.new_owner().is_none() {
            // still nobody there
            continue;
        }

        // this is it
        break;
    }

    info!("a provider has appeared :-)");
}
