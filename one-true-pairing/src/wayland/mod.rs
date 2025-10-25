use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;

use libc;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tokio::sync::mpsc::UnboundedSender;
use tokio_fd::AsyncFd;
use tracing::{debug, error, warn};
use whale_land::{Connection, Error, NewObject, ObjectId, Packet};
use whale_land::protocol::EventHandler;
use whale_land::protocol::wayland::{wl_data_source_v3_event_handler, wl_data_source_v3_request_proxy, wl_registry_v1_event_handler};

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


pub struct RegistryResponder {
    data_device_manager_id: Arc<RwLock<Option<ObjectId>>>,
    interface_to_def: Arc<RwLock<BTreeMap<String, InterfaceDef>>>,
}
impl RegistryResponder {
    pub fn new(
        data_device_manager_id: Arc<RwLock<Option<ObjectId>>>,
        interface_to_def: Arc<RwLock<BTreeMap<String, InterfaceDef>>>,
    ) -> Self {
        Self {
            data_device_manager_id,
            interface_to_def,
        }
    }

    pub fn interface_to_def(&self) -> Arc<RwLock<BTreeMap<String, InterfaceDef>>> {
        Arc::clone(&self.interface_to_def)
    }

    pub async fn data_device_manager_id(&self) -> Option<ObjectId> {
        let value = {
            let guard = self.data_device_manager_id
                .read().await;
            *guard
        };
        value
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

        if interface == "wl_data_device_manager" {
            let mut ddmi_guard = self.data_device_manager_id
                .write().await;
            *ddmi_guard = Some(name_oid);
        }
    }

    async fn handle_global_remove(
        &self,
        _connection: &Connection,
        name: u32,
    ) {
        println!("{} is gone", name);
    }
}


pub struct ClipboardResponder {
    data: String,
    clipboard_sender: UnboundedSender<ClipboardMessage>,
}
impl ClipboardResponder {
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
        let proxy = wl_data_source_v3_request_proxy::new(conn);
        if let Err(e) = proxy.send_destroy(data_source_obj_id).await {
            error!("error destroying ClipboardResponder {}'s wl_data_source: {}", data_source_obj_id.0, e);
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
impl EventHandler for ClipboardResponder {
    fn handle_event<'life0, 'life1, 'async_trait>(&'life0 self, connection: &'life1 Connection, packet: Packet) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'async_trait>>
            where 'life0: 'async_trait, 'life1: 'async_trait, Self: 'async_trait {
        Box::pin(wl_data_source_v3_event_handler::handle_event(self, connection, packet))
    }
}
impl wl_data_source_v3_event_handler for ClipboardResponder {
    fn handle_target(
        &self,
        connection: &whale_land::Connection,
        mime_type: ::std::string::String,
    ) -> impl ::std::future::Future<Output = ()> + ::std::marker::Send + ::std::marker::Sync {
        // this is drag'n'drop only, we don't care
        let _ = connection;
        let _ = mime_type;
        std::future::ready(())
    }

    fn handle_send(
        &self,
        _connection: &whale_land::Connection,
        mime_type: ::std::string::String,
        fd: ::std::os::fd::RawFd,
    ) -> impl ::std::future::Future<Output = ()> + ::std::marker::Send + ::std::marker::Sync {
        async move {
            debug!("ClipboardResponder should send clipboard as {:?} to {}", mime_type, fd);

            // alright then
            let wrapped_fd = AsyncFd::try_from(fd).unwrap();
            let res = self.do_handle_send(mime_type, wrapped_fd).await;

            // close the file descriptor
            let result = unsafe {
                libc::close(fd)
            };
            if result != 0 {
                error!("error trying to close file descriptor: {}", std::io::Error::last_os_error());
            }

            res
        }
    }

    fn handle_cancelled(
        &self,
        connection: &whale_land::Connection,
    ) -> impl ::std::future::Future<Output = ()> + ::std::marker::Send + ::std::marker::Sync {
        // tell our owner that they should give us up
        debug!("ClipboardResponder is being cancelled");
        let _ = connection;
        self.clipboard_sender.send(ClipboardMessage::Clear);
        std::future::ready(())
    }

    fn handle_dnd_drop_performed(
        &self,
        connection: &whale_land::Connection,
    ) -> impl ::std::future::Future<Output = ()> + ::std::marker::Send + ::std::marker::Sync {
        // this is drag'n'drop only, we don't care
        let _ = connection;
        std::future::ready(())
    }

    fn handle_dnd_finished(
        &self,
        connection: &whale_land::Connection,
    ) -> impl ::std::future::Future<Output = ()> + ::std::marker::Send + ::std::marker::Sync {
        // this is drag'n'drop only, we don't care
        let _ = connection;
        std::future::ready(())
    }

    fn handle_action(
        &self,
        connection: &whale_land::Connection,
        dnd_action: u32,
    ) -> impl ::std::future::Future<Output = ()> + ::std::marker::Send + ::std::marker::Sync {
        // this is drag'n'drop only, we don't care
        let _ = connection;
        let _ = dnd_action;
        std::future::ready(())
    }
}
