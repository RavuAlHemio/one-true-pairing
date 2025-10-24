use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::debug;
use whale_land::{Connection, Error, ObjectId, Packet};
use whale_land::protocol::EventHandler;
use whale_land::protocol::wayland::{wl_data_source_v3_request_proxy, wl_registry_v1_event_handler};


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
    data_device_manager_id: RwLock<Option<ObjectId>>,
    clipboard_id: Arc<RwLock<Option<ObjectId>>>,
    interface_to_def: Arc<RwLock<BTreeMap<String, InterfaceDef>>>,
}
impl RegistryResponder {
    pub fn new() -> Self {
        Self {
            data_device_manager_id: RwLock::new(None),
            clipboard_id: Arc::new(RwLock::new(None)),
            interface_to_def: Arc::new(RwLock::new(BTreeMap::new())),
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
        connection.object_id_seen(name_oid);

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
