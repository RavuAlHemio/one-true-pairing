pub mod wayland;


use crate::wayland::packet::Packet;


pub trait EventHandler {
    async fn handle_event(&self, packet: Packet) -> Result<(), crate::wayland::Error>;
}
