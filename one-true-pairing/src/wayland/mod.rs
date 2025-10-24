use std::pin::Pin;

use whale_land::{Connection, Error, Packet};
use whale_land::protocol::EventHandler;
use whale_land::protocol::wayland::wl_registry_v1_event_handler;


pub struct RegistryResponder;
impl EventHandler for RegistryResponder {
    fn handle_event<'life0, 'life1, 'async_trait>(&'life0 self, connection: &'life1 Connection, packet: Packet) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'async_trait>>
            where 'life0: 'async_trait, 'life1: 'async_trait, Self: 'async_trait {
        Box::pin(wl_registry_v1_event_handler::handle_event(self, connection, packet))
    }
}
impl wl_registry_v1_event_handler for RegistryResponder {
    async fn handle_global(
        &self,
        _connection: &Connection,
        name: u32,
        interface: String,
        version: u32,
    ) {
        println!("{} is {} v{}", name, interface, version);
    }

    async fn handle_global_remove(
        &self,
        _connection: &Connection,
        name: u32,
    ) {
        println!("{} is gone", name);
    }
}
