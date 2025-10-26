pub mod ext_data_control_v1;
pub mod wayland;


use async_trait::async_trait;

use crate::{Connection, Error, Packet};


#[async_trait]
pub trait EventHandler {
    async fn handle_event(&self, connection: &Connection, packet: Packet) -> Result<(), Error>;
}
