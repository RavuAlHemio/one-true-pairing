mod notifier;
mod secrets;
mod socket_fd_ext;
mod totp;
mod wayland;


use std::sync::OnceLock;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use zbus;

use crate::notifier::{ContextMenu, TrayIcon};
use crate::notifier::proxies::StatusNotifierWatcherProxy;
use crate::secrets::SecretSession;


const TRAY_ICON_BUS_PATH: &str = "/StatusNotifierItem";
const MENU_BUS_PATH: &str = "/SniMenu";
static STOPPER: OnceLock<CancellationToken> = OnceLock::new();
static SECRET_SESSION: OnceLock<RwLock<SecretSession>> = OnceLock::new();


#[tokio::main]
async fn main() {
    eprintln!("I have been assigned PID {}", std::process::id());

    // set up stopper
    let stopper = CancellationToken::new();
    STOPPER
        .set(stopper.clone()).expect("STOPPER already set?!");

    // connect to the session bus
    eprintln!("connecting to D-Bus");
    let dbus_conn = zbus::connection::Builder::session()
        .expect("failed to create connection to D-Bus session bus")
        .build()
        .await.expect("failed to build a D-Bus connection");

    // connect to a secret manager and list the secrets
    eprintln!("querying secret manager");
    let secret_session = SecretSession::new(dbus_conn.clone()).await;
    let secret_name_to_path = secret_session.get_secrets().await;
    SECRET_SESSION
        .set(RwLock::new(secret_session))
        .expect("SECRET_SESSION already set?!");

    // introduce the notifier icon and menu
    let icon = TrayIcon;
    let menu = ContextMenu::new(secret_name_to_path);

    // register them with the session bus
    let object_server = dbus_conn
        .object_server();
    object_server
        .at(MENU_BUS_PATH, menu)
        .await.expect("failed to serve menu via D-Bus");
    object_server
        .at(TRAY_ICON_BUS_PATH, icon)
        .await.expect("failed to serve icon via D-Bus");
    let dbus_name: &str = dbus_conn
        .unique_name()
        .expect("failed to obtain unique name from D-Bus connection");

    // connect to Wayland
    eprintln!("connecting to Wayland");
    let way_conn = crate::wayland::Connection::new_from_env()
        .await.expect("failed to create connection to Wayland server");

    // get access to Wayland registry
    const WL_DISPLAY_WELL_KNOWN_OID: u32 = 1;
    const WL_DISPLAY_REQUEST_GET_REGISTRY: u16 = 1;
    const WL_REGISTRY_OID: u32 = 2;
    let mut get_registry = wayland::Packet::new(
        WL_DISPLAY_WELL_KNOWN_OID,
        WL_DISPLAY_REQUEST_GET_REGISTRY,
    );
    get_registry.push_uint(WL_REGISTRY_OID);
    way_conn.send_packet(&get_registry).await
        .expect("failed to send wl_display::get_registry packet");

    // scope this so that the icon_host proxy is dropped
    {
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
    }

    // alrighty
    loop {
        tokio::select! {
            // zbus has its own task
            _ = stopper.cancelled() => {
                // it's time to end
                break;
            },
            way_packet_res = way_conn.recv_packet() => {
                println!("way_packet_res: {:?}", way_packet_res);
            },
        }
    }

    eprintln!("stopper passed");

    // drop our copy of the D-Bus connection
    drop(dbus_conn);

    // drop the connection inside the secrets session
    {
        let mut session_guard = SECRET_SESSION
            .get().expect("SECRET_SESSION unset?!")
            .write().await;
        eprintln!("dropping session connection");
        session_guard.drop_connection().await;
        eprintln!("session connection dropped");
    }

    eprintln!("D-Bus connection shut down");
}
