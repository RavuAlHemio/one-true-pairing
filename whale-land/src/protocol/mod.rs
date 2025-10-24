pub mod wayland;


use async_trait::async_trait;

use crate::{Error, Packet};


#[async_trait]
pub trait EventHandler {
    async fn handle_event(&self, packet: Packet) -> Result<(), Error>;
}
