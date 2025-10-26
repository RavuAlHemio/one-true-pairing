use std::collections::BTreeMap;
use std::os::fd::RawFd;
use std::pin::Pin;
use std::sync::Arc;

use libc;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tokio::sync::mpsc::UnboundedSender;
use tokio_fd::AsyncFd;
use tracing::{debug, error, warn};
use whale_land::{Connection, Error, NewObject, NewObjectId, ObjectId, Packet};
use whale_land::protocol::EventHandler;
use whale_land::protocol::ext_data_control_v1::{
    ext_data_control_device_v1_v1_event_handler, ext_data_control_manager_v1_v1_request_proxy, ext_data_control_source_v1_v1_event_handler, ext_data_control_source_v1_v1_request_proxy
};
use whale_land::protocol::wayland::{wl_registry_v1_event_handler, wl_registry_v1_request_proxy, wl_seat_v10_event_handler};

use crate::ClipboardMessage;


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceDef {
    pub obj_id: ObjectId,
    pub name: String,
    pub version: u32,
}
impl InterfaceDef {
    pub fn new(
        obj_id: ObjectId,
        name: String,
        version: u32,
    ) -> Self {
        Self {
            obj_id,
            name,
            version,
        }
    }
}


#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClipboardObjectIds {
    pub seat_id: Option<ObjectId>,
    pub manager_id: Option<ObjectId>,
    pub device_id: Option<ObjectId>,
    pub control_source_id: Option<ObjectId>,
}
impl ClipboardObjectIds {
    pub const fn new() -> Self {
        Self {
            seat_id: None,
            manager_id: None,
            device_id: None,
            control_source_id: None,
        }
    }
}


pub struct RegistryResponder {
    object_ids: Arc<RwLock<ClipboardObjectIds>>,
    interface_to_def: Arc<RwLock<BTreeMap<String, InterfaceDef>>>,
}
impl RegistryResponder {
    pub fn new(
        object_ids: Arc<RwLock<ClipboardObjectIds>>,
        interface_to_def: Arc<RwLock<BTreeMap<String, InterfaceDef>>>,
    ) -> Self {
        Self {
            object_ids,
            interface_to_def,
        }
    }

    pub fn interface_to_def(&self) -> Arc<RwLock<BTreeMap<String, InterfaceDef>>> {
        Arc::clone(&self.interface_to_def)
    }

    pub async fn object_ids(&self) -> ClipboardObjectIds {
        let value = {
            let guard = self.object_ids
                .read().await;
            *guard
        };
        value
    }

    async fn request_interface(
        connection: &whale_land::Connection,
        registry_id: ObjectId,
        number_on_server: u32,
        interface_name: String,
        interface_version: u32,
    ) -> Option<ObjectId> {
        let object_id = connection.get_and_increment_next_object_id();
        let proxy = wl_registry_v1_request_proxy::new(connection);
        let send_res = proxy.send_bind(
            registry_id,
            number_on_server,
            NewObject {
                object_id,
                interface: interface_name.clone(),
                interface_version,
            },
        ).await;
        if let Err(e) = send_res {
            error!("failed to bind to {} v{}: {}", interface_name, interface_version, e);
            None
        } else {
            Some(object_id)
        }
    }
}
impl EventHandler for RegistryResponder {
    fn handle_event<'life0, 'life1, 'async_trait>(&'life0 self, connection: &'life1 Connection, packet: Packet) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'async_trait>>
            where 'life0: 'async_trait, 'life1: 'async_trait, Self: 'async_trait {
        Box::pin(wl_registry_v1_event_handler::handle_event(self, connection, packet))
    }
}
impl wl_registry_v1_event_handler for RegistryResponder {
    async fn handle_global(
        &self,
        connection: &Connection,
        packet: Packet,
        name: u32,
        interface: String,
        version: u32,
    ) {
        let Some(name_oid) = ObjectId::new(name) else { return };

        debug!("{} is {} v{}", name, interface, version);

        {
            let mut interface_to_def = self.interface_to_def
                .write().await;
            interface_to_def.insert(
                interface.clone(),
                InterfaceDef::new(
                    name_oid,
                    interface.clone(),
                    version,
                ),
            );
        }

        if interface == "wl_seat" {
            let seat_id_opt = Self::request_interface(
                connection,
                packet.object_id(),
                name,
                interface,
                version,
            ).await;
            let Some(seat_id) = seat_id_opt else { return };

            // do we have a data control manager?
            let manager_id_opt = {
                let mut ids_guard = self.object_ids
                    .write().await;
                ids_guard.seat_id = Some(seat_id);
                ids_guard.manager_id
            };
            let Some(manager_id) = manager_id_opt else { return };

            // yes; request the data device
            let data_device_id = connection.get_and_increment_next_object_id();
            let proxy = ext_data_control_manager_v1_v1_request_proxy::new(connection);
            proxy.send_get_data_device(
                manager_id,
                NewObjectId(data_device_id),
                Some(seat_id),
            ).await;

            {
                let mut ids_guard = self.object_ids
                    .write().await;
                ids_guard.device_id = Some(data_device_id);
            }
        } else if interface == "ext_data_control_manager_v1" {
            let manager_id_opt = Self::request_interface(
                connection,
                packet.object_id(),
                name,
                interface,
                version,
            ).await;
            let Some(manager_id) = manager_id_opt else { return };

            // do we have a seat?
            let seat_id_opt = {
                let mut ids_guard = self.object_ids
                    .write().await;
                ids_guard.manager_id = Some(manager_id);
                ids_guard.seat_id
            };
            let Some(seat_id) = seat_id_opt else { return };

            // yes; request the data device
            let data_device_id = connection.get_and_increment_next_object_id();
            let proxy = ext_data_control_manager_v1_v1_request_proxy::new(connection);
            proxy.send_get_data_device(
                manager_id,
                NewObjectId(data_device_id),
                Some(seat_id),
            ).await;

            {
                let mut ids_guard = self.object_ids
                    .write().await;
                ids_guard.device_id = Some(data_device_id);
            }
        }
    }

    async fn handle_global_remove(
        &self,
        _connection: &Connection,
        _packet: Packet,
        name: u32,
    ) {
        println!("{} is gone", name);
    }
}


pub struct SeatResponder;
impl EventHandler for SeatResponder {
    fn handle_event<'life0, 'life1, 'async_trait>(&'life0 self, connection: &'life1 Connection, packet: Packet) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'async_trait>>
            where 'life0: 'async_trait, 'life1: 'async_trait, Self: 'async_trait {
        Box::pin(wl_seat_v10_event_handler::handle_event(self, connection, packet))
    }
}
impl wl_seat_v10_event_handler for SeatResponder {
    async fn handle_capabilities(
        &self,
        connection: &whale_land::Connection,
        packet: whale_land::Packet,
        capabilities: u32,
    ) {
        // capabilities changed
        let _ = connection;
        let _ = packet;
        let _ = capabilities;
    }

    async fn handle_name(
        &self,
        connection: &whale_land::Connection,
        packet: whale_land::Packet,
        name: String,
    ) {
        // this is my name
        let _ = connection;
        let _ = packet;
        let _ = name;
    }
}


pub struct ClipboardDataControlDeviceResponder;
impl EventHandler for ClipboardDataControlDeviceResponder {
    fn handle_event<'life0, 'life1, 'async_trait>(&'life0 self, connection: &'life1 Connection, packet: Packet) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'async_trait>>
            where 'life0: 'async_trait, 'life1: 'async_trait, Self: 'async_trait {
        Box::pin(ext_data_control_device_v1_v1_event_handler::handle_event(self, connection, packet))
    }
}
impl ext_data_control_device_v1_v1_event_handler for ClipboardDataControlDeviceResponder {
    async fn handle_data_offer(
        &self,
        connection: &whale_land::Connection,
        packet: whale_land::Packet,
        id: whale_land::NewObjectId,
    ) {
        // new offer object
        let _ = connection;
        let _ = packet;
        let _ = id;
    }

    async fn handle_selection(
        &self,
        connection: &whale_land::Connection,
        packet: whale_land::Packet,
        id: Option<whale_land::ObjectId>,
    ) {
        // offer object has new content
        let _ = connection;
        let _ = packet;
        let _ = id;
    }

    async fn handle_finished(
        &self,
        connection: &whale_land::Connection,
        packet: whale_land::Packet,
    ) {
        // this data control object has been deleted
        let _ = connection;
        let _ = packet;
    }

    async fn handle_primary_selection(
        &self,
        connection: &whale_land::Connection,
        packet: whale_land::Packet,
        id: Option<whale_land::ObjectId>,
    ) {
        // there's a new "highlighted a bit of text" selection
        let _ = connection;
        let _ = packet;
        let _ = id;
    }
}


pub struct ClipboardDataControlSourceResponder {
    data: String,
    clipboard_sender: UnboundedSender<ClipboardMessage>,
}
impl ClipboardDataControlSourceResponder {
    pub fn new(
        data: String,
        clipboard_sender: UnboundedSender<ClipboardMessage>,
    ) -> Self {
        Self {
            data,
            clipboard_sender,
        }
    }

    pub async fn destroy(conn: &Connection, data_source_obj_id: ObjectId) {
        let proxy = ext_data_control_source_v1_v1_request_proxy::new(conn);
        if let Err(e) = proxy.send_destroy(data_source_obj_id).await {
            error!("error destroying ClipboardDataControlSourceResponder {}'s ext_data_control_source_v1: {}", data_source_obj_id.0, e);
        }
    }

    async fn do_handle_send(
        &self,
        mime_type: String,
        mut fd: AsyncFd,
    ) {
        if mime_type == "text/plain" {
            if let Err(e) = fd.write_all(self.data.as_bytes()).await {
                error!("failed to write clipboard data to file descriptor: {}", e);
                return;
            }
            if let Err(e) = fd.flush().await {
                error!("failed to flush clipboard data through file descriptor: {}", e);
                return;
            }
        } else {
            warn!("unexpected MIME type {:?} requested for clipboard data", mime_type);
        }
    }
}
impl EventHandler for ClipboardDataControlSourceResponder {
    fn handle_event<'life0, 'life1, 'async_trait>(&'life0 self, connection: &'life1 Connection, packet: Packet) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'async_trait>>
            where 'life0: 'async_trait, 'life1: 'async_trait, Self: 'async_trait {
        Box::pin(ext_data_control_source_v1_v1_event_handler::handle_event(self, connection, packet))
    }
}
impl ext_data_control_source_v1_v1_event_handler for ClipboardDataControlSourceResponder {
    async fn handle_send(
        &self,
        _connection: &whale_land::Connection,
        _packet: whale_land::Packet,
        mime_type: String,
        fd: RawFd,
    ) {
        debug!("ClipboardResponder should send clipboard as {:?} to {}", mime_type, fd);

        // alright then
        let wrapped_fd = AsyncFd::try_from(fd).unwrap();
        self.do_handle_send(mime_type, wrapped_fd).await;

        // close the file descriptor
        let result = unsafe {
            libc::close(fd)
        };
        if result != 0 {
            error!("error trying to close file descriptor: {}", std::io::Error::last_os_error());
        }
    }

    fn handle_cancelled(
        &self,
        connection: &whale_land::Connection,
        _packet: whale_land::Packet,
    ) -> impl ::std::future::Future<Output = ()> + ::std::marker::Send + ::std::marker::Sync {
        // tell our owner that they should give us up
        debug!("ClipboardResponder is being cancelled");
        let _ = connection;
        self.clipboard_sender.send(ClipboardMessage::Clear);
        std::future::ready(())
    }
}
