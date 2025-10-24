mod notifier;
mod secrets;
mod wayland;
mod totp;


use std::sync::OnceLock;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;
use whale_land::{NewObjectId, ObjectId};
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
    // set up tracing
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    info!("I have been assigned PID {}", std::process::id());

    // set up stopper
    let stopper = CancellationToken::new();
    STOPPER
        .set(stopper.clone()).expect("STOPPER already set?!");

    // connect to the session bus
    debug!("connecting to D-Bus");
    let dbus_conn = zbus::connection::Builder::session()
        .expect("failed to create connection to D-Bus session bus")
        .build()
        .await.expect("failed to build a D-Bus connection");

    // connect to a secret manager and list the secrets
    debug!("querying secret manager");
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
    debug!("connecting to Wayland");
    let mut way_conn = whale_land::Connection::new_from_env()
        .await.expect("failed to create connection to Wayland server");

    // prepare registry responder
    debug!("creating registry responder");
    way_conn.register_handler(ObjectId::REGISTRY, Box::new(crate::wayland::RegistryResponder::new()));

    // get access to Wayland registry
    debug!("querying registry");
    let display = whale_land::protocol::wayland::wl_display_v1_request_proxy::new(&way_conn);
    display.send_get_registry(
        ObjectId::DISPLAY,
        NewObjectId(ObjectId::REGISTRY),
    )
        .await
        .expect("failed to send wl_display::get_registry packet");

    // scope this so that the icon_host proxy is dropped
    {
        // find a tray icon host
        debug!("poking at the icon host");
        let icon_host = StatusNotifierWatcherProxy::new(&dbus_conn)
            .await.expect("failed to connect to icon host");

        let proto_version = icon_host.protocol_version()
            .await.expect("failed to obtain protocol version");
        assert_eq!(proto_version, 0, "we only support protocol version 0, icon host is using a different one");

        debug!("registering icon");
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
                match way_packet_res {
                    Ok(way_packet) => {
                        if let Err(e) = way_conn.dispatch(way_packet).await {
                            error!("error dispatching Wayland packet: {}", e);
                        }
                    },
                    Err(e) => {
                        error!("error receiving Wayland packet: {}", e);
                    },
                }
            },
        }
    }

    debug!("stopper passed");

    // drop our copy of the D-Bus connection
    drop(dbus_conn);

    // drop the connection inside the secrets session
    {
        let mut session_guard = SECRET_SESSION
            .get().expect("SECRET_SESSION unset?!")
            .write().await;
        debug!("dropping session connection");
        session_guard.drop_connection().await;
        debug!("session connection dropped");
    }

    debug!("D-Bus connection shut down");
}
