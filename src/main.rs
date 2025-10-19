mod notifier;
mod secrets;


use std::sync::{Barrier, LazyLock};

use zbus;

use crate::notifier::{ContextMenu, TrayIcon};
use crate::notifier::proxies::StatusNotifierWatcherProxy;
use crate::secrets::SecretSession;


const TRAY_ICON_BUS_PATH: &str = "/StatusNotifierItem";
const MENU_BUS_PATH: &str = "/SniMenu";
static STOPPER: LazyLock<Barrier> = LazyLock::new(|| Barrier::new(2));


#[tokio::main]
async fn main() {
    eprintln!("I have been assigned PID {}", std::process::id());

    // connect to the session bus
    eprintln!("connecting to D-Bus");
    let dbus_conn = zbus::connection::Builder::session()
        .expect("failed to create connection to D-Bus session bus")
        .build()
        .await.expect("failed to build a D-Bus connection");

    // connect to a secret manager and list the secrets
    eprintln!("connecting to a secret manager");
    let secret_session = SecretSession::new(&dbus_conn).await;
    let secret_name_to_path = secret_session.get_secrets().await;

    // introduce the notifier icon and menu
    let icon = TrayIcon;
    let menu = ContextMenu::new(secret_name_to_path);

    // register them with the session bus
    let object_server = dbus_conn.object_server();
    object_server
        .at(MENU_BUS_PATH, menu)
        .await.expect("failed to serve menu via D-Bus");
    object_server
        .at(TRAY_ICON_BUS_PATH, icon)
        .await.expect("failed to serve icon via D-Bus");
    let dbus_name: &str = &*dbus_conn.unique_name()
        .expect("failed to obtain unique name from D-Bus connection");

    // connect to Wayland
    eprintln!("connecting to Wayland");
    let way_conn = wayland_client::Connection::connect_to_env()
        .expect("failed to create connection to Wayland server");

    // find a tray icon host
    eprintln!("poking at the icon host");
    let icon_host = StatusNotifierWatcherProxy::new(&dbus_conn)
        .await.expect("failed to connect to icon host");

    let proto_version = icon_host.protocol_version()
        .await.expect("failed to obtain protocol version");
    assert_eq!(proto_version, 0, "we only support protocol version 0, icon host is using a different one");

    eprintln!("registering icon");
    icon_host.register_status_notifier_item(dbus_name.to_owned())
        .await.expect("failed to register icon");

    STOPPER.wait();
    eprintln!("stopper passed");
}
