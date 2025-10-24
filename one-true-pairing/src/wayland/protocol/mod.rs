pub mod wayland;


use async_trait::async_trait;

use crate::wayland::packet::Packet;


#[async_trait]
pub trait EventHandler {
    async fn handle_event(&self, packet: Packet) -> Result<(), crate::wayland::Error>;
}
