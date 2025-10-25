mod notifier;
mod secrets;
mod wayland;
mod totp;


use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;
use whale_land::{NewObjectId, ObjectId};
use whale_land::protocol::wayland::{
    wl_display_v1_request_proxy,
    wl_data_device_manager_v3_request_proxy,
};
use zbus;

use crate::notifier::{ContextMenu, TrayIcon};
use crate::notifier::proxies::StatusNotifierWatcherProxy;
use crate::secrets::SecretSession;
use crate::wayland::ClipboardResponder;


const TRAY_ICON_BUS_PATH: &str = "/StatusNotifierItem";
const MENU_BUS_PATH: &str = "/SniMenu";
static SECRET_SESSION: OnceLock<RwLock<SecretSession>> = OnceLock::new();


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ClipboardMessage {
    Copy(String),
    Clear,
    Exit,
}


#[tokio::main]
async fn main() {
    // set up tracing
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    info!("I have been assigned PID {}", std::process::id());

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

    // prepare the communications channel
    let (clipboard_sender, mut clipboard_receiver) = mpsc::unbounded_channel();

    // introduce the notifier icon and menu
    let icon = TrayIcon;
    let menu = ContextMenu::new(secret_name_to_path, clipboard_sender.clone());

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
    let registry_id = way_conn.get_and_increment_next_object_id();
    let data_device_manager_id = Arc::new(RwLock::new(None));
    let interface_to_def = Arc::new(RwLock::new(BTreeMap::new()));
    let registry_responder = crate::wayland::RegistryResponder::new(
        Arc::clone(&data_device_manager_id),
        interface_to_def,
    );
    way_conn.register_handler(registry_id, Box::new(registry_responder));

    // get access to Wayland registry
    debug!("querying registry");
    let display = wl_display_v1_request_proxy::new(&way_conn);
    display.send_get_registry(
        ObjectId::DISPLAY,
        NewObjectId(registry_id),
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

    let mut current_data_source = None;

    // alrighty
    loop {
        tokio::select! {
            // zbus has its own task
            message_opt = clipboard_receiver.recv() => {
                match message_opt {
                    None => {
                        error!("clipboard sender went away!");
                        break;
                    },
                    Some(ClipboardMessage::Exit) => {
                        // it's time to end
                        break;
                    },
                    Some(ClipboardMessage::Copy(value)) => {
                        let ddmi_opt = {
                            let ddmi_guard = data_device_manager_id.read().await;
                            *ddmi_guard
                        };
                        let Some(ddmi) = ddmi_opt else {
                            error!("no data device manager => no clipboard");
                            continue;
                        };

                        // clear out the previous data source
                        let old_source_opt = std::mem::replace(&mut current_data_source, None);
                        if let Some(old_source) = old_source_opt {
                            way_conn.drop_handler(old_source);
                            ClipboardResponder::destroy(&way_conn, old_source).await;
                        }

                        // create a new data source
                        let data_source_id = way_conn.get_and_increment_next_object_id();
                        let new_source = ClipboardResponder::new(
                            value,
                            clipboard_sender.clone(),
                        );
                        way_conn.register_handler(data_source_id, Box::new(new_source));
                        let ddm_proxy = wl_data_device_manager_v3_request_proxy::new(&way_conn);
                        if let Err(e) = ddm_proxy.send_create_data_source(ddmi, NewObjectId(data_source_id)).await {
                            error!("error asking to create data source: {}", e);
                            continue;
                        }
                    },
                    Some(ClipboardMessage::Clear) => {
                        let old_source_opt = std::mem::replace(&mut current_data_source, None);
                        if let Some(old_source) = old_source_opt {
                            way_conn.drop_handler(old_source);
                            ClipboardResponder::destroy(&way_conn, old_source).await
                        }
                    },
                }
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
