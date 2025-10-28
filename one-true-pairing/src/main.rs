mod notifier;
mod secrets;
mod totp;


use std::io;
use std::sync::OnceLock;

use libc::close;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio_fd::AsyncFd;
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;
use whale_land::{NewObject, NewObjectId, ObjectId};
use whale_land::protocol::ext_data_control_v1::{
    ext_data_control_device_v1_v1_event_data_offer_args,
    ext_data_control_device_v1_v1_event_finished_args,
    ext_data_control_device_v1_v1_event_primary_selection_args,
    ext_data_control_device_v1_v1_event_selection_args,
    ext_data_control_device_v1_v1_request_set_selection_args,
    ext_data_control_manager_v1_v1_request_create_data_source_args,
    ext_data_control_manager_v1_v1_request_get_data_device_args,
    ext_data_control_offer_v1_v1_event_offer_args,
    ext_data_control_source_v1_v1_event_cancelled_args,
    ext_data_control_source_v1_v1_event_send_args,
    ext_data_control_source_v1_v1_request_destroy_args,
    ext_data_control_source_v1_v1_request_offer_args,
};
use whale_land::protocol::wayland::{
    wl_display_v1_event_error_args, wl_display_v1_request_proxy, wl_registry_v1_event_global_args,
    wl_registry_v1_request_bind_args, wl_seat_v10_event_capabilities_args,
    wl_seat_v10_event_name_args,
};
use zbus;

use crate::notifier::{ContextMenu, TrayIcon};
use crate::notifier::proxies::StatusNotifierWatcherProxy;
use crate::secrets::SecretSession;


const TRAY_ICON_BUS_PATH: &str = "/StatusNotifierItem";
const MENU_BUS_PATH: &str = "/SniMenu";
const PLAIN_TEXT_MIME_TYPES_SORTED: [&'static str; 5] = [
    "STRING",
    "TEXT",
    "UTF8_STRING",
    "text/plain",
    "text/plain;charset=utf-8",
];
static SECRET_SESSION: OnceLock<RwLock<SecretSession>> = OnceLock::new();


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ClipboardMessage {
    Copy(String),
    Clear,
    Exit,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WaylandData {
    pub registry_id: Option<ObjectId>,
    pub seat_id: Option<ObjectId>,
    pub clipboard_manager_id: Option<ObjectId>,
    pub clipboard_device_id: Option<ObjectId>,
    pub clipboard_data: Option<String>,
    pub clipboard_source_id: Option<ObjectId>,
    pub incoming_offer_id: Option<ObjectId>,
}
impl WaylandData {
    pub const fn new() -> Self {
        Self {
            registry_id: None,
            seat_id: None,
            clipboard_manager_id: None,
            clipboard_device_id: None,
            clipboard_data: None,
            clipboard_source_id: None,
            incoming_offer_id: None,
        }
    }
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
    let way_conn = whale_land::Connection::new_from_env()
        .await.expect("failed to create connection to Wayland server");

    // get access to Wayland registry
    debug!("querying registry");
    let registry_id = way_conn.get_and_increment_next_object_id();
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

    let mut wayland_data = WaylandData::new();
    wayland_data.registry_id = Some(registry_id);

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
                        // update the value
                        copy_dispatch(
                            &way_conn,
                            &mut wayland_data,
                            value,
                        ).await;
                    },
                    Some(ClipboardMessage::Clear) => {
                        // remove the value and destroy the source
                        clear_dispatch(
                            &way_conn,
                            &mut wayland_data,
                        ).await;
                    },
                }
            },
            way_packet_res = way_conn.recv_packet() => {
                match way_packet_res {
                    Ok(way_packet) => {
                        wayland_dispatch(
                            &way_conn,
                            way_packet,
                            &mut wayland_data,
                        ).await
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

async fn wayland_dispatch(
    conn: &whale_land::Connection,
    packet: whale_land::Packet,
    data: &mut WaylandData,
) {
    if packet.object_id() == ObjectId::DISPLAY {
        if packet.opcode() == wl_display_v1_event_error_args::OPCODE {
            let error_args = wl_display_v1_event_error_args::try_from_packet(&packet)
                .expect("invalid wl_display::error packet");
            error!(
                "Wayland server sends an error: object {:?} says [{}] {}",
                error_args.object_id,
                error_args.code,
                error_args.message,
            );
        } else {
            warn!("unhandled event from wl_display: {:?}", packet);
        }
    } else if Some(packet.object_id()) == data.registry_id {
        if packet.opcode() == wl_registry_v1_event_global_args::OPCODE {
            let global_args = wl_registry_v1_event_global_args::try_from_packet(&packet)
                .expect("invalid wl_registry::global packet");
            debug!("{}: {} v{}", global_args.name, global_args.interface, global_args.version);
            match &*global_args.interface {
                "wl_seat" => {
                    // we need this to mess with the clipboard
                    if data.seat_id.is_some() {
                        // dupe, skip
                        // FIXME: what if the seat is replaced later?
                        return;
                    }
                    let new_seat_id = conn.get_and_increment_next_object_id();
                    let gimme_packet = global_event_args_to_bind_request_packet(
                        &global_args,
                        new_seat_id,
                        data.registry_id.unwrap(),
                    );
                    conn.send_packet(&gimme_packet).await
                        .expect("failed to send gimme-seat packet");
                    data.seat_id = Some(new_seat_id);
                    debug!("requested that wl_seat become {:?}", new_seat_id);

                    obtain_data_device_if_ready(conn, data).await;
                },
                "ext_data_control_manager_v1" => {
                    // this allows us to mess with the clipboard
                    if data.clipboard_manager_id.is_some() {
                        // dupe, skip
                        return;
                    }
                    let new_clipboard_manager_id = conn.get_and_increment_next_object_id();
                    let gimme_packet = global_event_args_to_bind_request_packet(
                        &global_args,
                        new_clipboard_manager_id,
                        data.registry_id.unwrap(),
                    );
                    conn.send_packet(&gimme_packet).await
                        .expect("failed to send gimme-clipboard-manager packet");
                    data.clipboard_manager_id = Some(new_clipboard_manager_id);
                    debug!("requested that ext_data_control_manager_v1 become {:?}", new_clipboard_manager_id);

                    obtain_data_device_if_ready(conn, data).await;
                },
                _ => {},
            }
        } else {
            warn!("unhandled event from wl_registry: {:?}", packet);
        }
    } else if Some(packet.object_id()) == data.clipboard_source_id {
        if packet.opcode() == ext_data_control_source_v1_v1_event_send_args::OPCODE {
            let send_args = ext_data_control_source_v1_v1_event_send_args::try_from_packet(&packet)
                .expect("failed to deserialize ext_data_control_source_v1::send args");
            debug!("someone's asking for our contents in format {:?} on FD {}", send_args.mime_type, send_args.fd);
            if PLAIN_TEXT_MIME_TYPES_SORTED.binary_search(&&*send_args.mime_type).is_ok() {
                if let Some(clip_data) = data.clipboard_data.as_ref() {
                    let mut fd = AsyncFd::try_from(send_args.fd)
                        .expect("failed to wrap file descriptor");
                    fd.write_all(clip_data.as_bytes())
                        .await.expect("failed to write clipboard data");
                    fd.flush()
                        .await.expect("failed to flush clipboard data");
                }
            }

            // in any case, close the file descriptor
            let res = unsafe {
                close(send_args.fd)
            };
            if res == -1 {
                panic!("failed to close clipboard fd: {}", io::Error::last_os_error());
            }
        } else if packet.opcode() == ext_data_control_source_v1_v1_event_cancelled_args::OPCODE {
            // something replaced us
            // oh well, drop the data and forget the no-longer-valid source ID
            data.clipboard_data = None;
            data.clipboard_source_id = None;
        }
    } else if Some(packet.object_id()) == data.seat_id {
        if packet.opcode() == wl_seat_v10_event_capabilities_args::OPCODE {
            let capabilities_args = wl_seat_v10_event_capabilities_args::try_from_packet(&packet)
                .expect("failed to decode wl_seat::capabilities args");
            debug!("our seat has capabilities {:032b}", capabilities_args.capabilities);
        } else if packet.opcode() == wl_seat_v10_event_name_args::OPCODE {
            let name_args = wl_seat_v10_event_name_args::try_from_packet(&packet)
                .expect("failed to decode wl_seat::name args");
            debug!("we are on seat {:?}", name_args.name);
        } else {
            warn!("unhandled event from wl_seat: {:?}", packet);
        }
    } else if Some(packet.object_id()) == data.clipboard_device_id {
        if packet.opcode() == ext_data_control_device_v1_v1_event_data_offer_args::OPCODE {
            let offer_args = ext_data_control_device_v1_v1_event_data_offer_args::try_from_packet(&packet)
                .expect("failed to decode ext_data_control_device_v1::data_offer args");
            debug!("we are being offered data in {:?}", offer_args.id);
            data.incoming_offer_id = Some(offer_args.id.0);
        } else if packet.opcode() == ext_data_control_device_v1_v1_event_selection_args::OPCODE {
            let selection_args = ext_data_control_device_v1_v1_event_selection_args::try_from_packet(&packet)
                .expect("failed to decode ext_data_control_device_v1::selection args");
            debug!("selection {:?} is now on offer", selection_args.id);
        } else if packet.opcode() == ext_data_control_device_v1_v1_event_finished_args::OPCODE {
            ext_data_control_device_v1_v1_event_finished_args::try_from_packet(&packet)
                .expect("failed to decode ext_data_control_device_v1::finished args");
            error!("control device is now gone! not sure how to handle this!");
        } else if packet.opcode() == ext_data_control_device_v1_v1_event_primary_selection_args::OPCODE {
            let selection_args = ext_data_control_device_v1_v1_event_primary_selection_args::try_from_packet(&packet)
                .expect("failed to decode ext_data_control_device_v1::primary_selection args");
            debug!("selection {:?} is now on offer as the primary selection", selection_args.id);
        } else {
            warn!("unhandled event from ext_data_control_device_v1: {:?}", packet);
        }
    } else if Some(packet.object_id()) == data.clipboard_manager_id {
        // this object doesn't even have events
        warn!("unhandled event from ext_data_control_manager_v1: {:?}", packet);
    } else if Some(packet.object_id()) == data.incoming_offer_id {
        if packet.opcode() == ext_data_control_offer_v1_v1_event_offer_args::OPCODE {
            let event_offer_args = ext_data_control_offer_v1_v1_event_offer_args::try_from_packet(&packet)
                .expect("failed to decode ext_data_control_offer_v1::offer args");
            debug!("offer supports MIME type {}", event_offer_args.mime_type);
        } else {
            warn!("unhandled event from ext_data_control_offer_v1: {:?}", packet);
        }
    } else {
        warn!("unhandled event: {:?}", packet);
    }
}

fn global_event_args_to_bind_request_packet(
    global_args: &wl_registry_v1_event_global_args,
    new_object_id: ObjectId,
    registry_id: ObjectId,
) -> whale_land::Packet {
    let args = wl_registry_v1_request_bind_args {
        name: global_args.name,
        id: NewObject {
            object_id: new_object_id,
            interface: global_args.interface.clone(),
            interface_version: global_args.version,
        },
    };
    let packet = args.try_into_packet(registry_id)
        .expect("failed to serialize args");
    packet
}

async fn obtain_data_device_if_ready(
    conn: &whale_land::Connection,
    data: &mut WaylandData,
) {
    let Some(clipboard_manager_id) = data.clipboard_manager_id else {
        debug!("we're still missing the clipboard manager");
        return;
    };
    let Some(seat_id) = data.seat_id else {
        debug!("we're still missing the seat");
        return;
    };
    let clipboard_device_id = conn.get_and_increment_next_object_id();
    let gimme = ext_data_control_manager_v1_v1_request_get_data_device_args {
        id: NewObjectId(clipboard_device_id),
        seat: Some(seat_id),
    };
    let packet = gimme.try_into_packet(clipboard_manager_id)
        .expect("failed to serialize packet");
    conn.send_packet(&packet)
        .await.expect("failed to send obtain-data-device packet");
    data.clipboard_device_id = Some(clipboard_device_id);
    debug!("requested that ext_data_control_device_v1 become {:?}", clipboard_device_id);
}

async fn copy_dispatch(
    conn: &whale_land::Connection,
    data: &mut WaylandData,
    new_content: String,
) {
    debug!("publishing {:?} on the clipboard", new_content);

    // store the new content
    data.clipboard_data = Some(new_content);

    // do we have a data source?
    if data.clipboard_source_id.is_some() {
        // yup; no need to change anything here
        return;
    }

    let Some(manager_id) = data.clipboard_manager_id else {
        error!("cannot copy data onto clipboard without a clipboard manager");
        return;
    };
    let Some(device_id) = data.clipboard_device_id else {
        error!("cannot copy data onto clipboard without a clipboard device");
        return;
    };

    // request a source from the manager
    let source_id = conn.get_and_increment_next_object_id();
    let gimme = ext_data_control_manager_v1_v1_request_create_data_source_args {
        id: NewObjectId(source_id),
    };
    let gimme_packet = gimme.try_into_packet(manager_id)
        .expect("failed to serialize create-data-source packet");
    conn.send_packet(&gimme_packet)
        .await.expect("failed to send create-data-source packet");
    debug!("requested that ext_data_control_source_v1 become {:?}", source_id);

    // inform everyone that we can do plain text (in all its variants)
    for plain_text_type in PLAIN_TEXT_MIME_TYPES_SORTED {
        let i_can_plain_text = ext_data_control_source_v1_v1_request_offer_args {
            mime_type: plain_text_type.to_owned(),
        };
        let i_can_packet = i_can_plain_text.try_into_packet(source_id)
            .expect("failed to serialize I-can-do-plain-text packet");
        conn.send_packet(&i_can_packet)
            .await.expect("failed to send I-can-do-plain-text packet");
    }
    debug!("informed about our support for text/plain");

    // set us as the data source
    let set_data_source = ext_data_control_device_v1_v1_request_set_selection_args {
        source: Some(source_id),
    };
    let set_data_packet = set_data_source.try_into_packet(device_id)
        .expect("failed to serialize set-data-source packet");
    conn.send_packet(&set_data_packet)
        .await.expect("failed to send set-data-source packet");
    data.clipboard_source_id = Some(source_id);
    debug!("asked ext_data_control_device_v1 {:?} that {:?} becomes the selection", device_id, source_id);
}


async fn clear_dispatch(
    conn: &whale_land::Connection,
    data: &mut WaylandData,
) {
    // drop the content
    data.clipboard_data = None;

    // do we have a data source?
    let Some(source_id) = data.clipboard_source_id else {
        // nope; no need to worry
        return;
    };

    let destroy_source = ext_data_control_source_v1_v1_request_destroy_args {
    };
    let destroy_packet = destroy_source.try_into_packet(source_id)
        .expect("failed to serialize destroy-data-source packet");
    conn.send_packet(&destroy_packet)
        .await.expect("failed to send destroy-data-source packet");
    debug!("ask that we {:?} are no longer the data source", source_id);

    // forget our data source
    data.clipboard_source_id = None;
}
