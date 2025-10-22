use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::AtomicU32;

use tokio::net::UnixStream;
use tokio::sync::Mutex;

use crate::socket_fd_ext::SocketFdExt;
use crate::wayland::error::Error;
use crate::wayland::packet::Packet;


const RUNTIME_DIR_VAR: &str = "XDG_RUNTIME_DIR";
const WAYLAND_DISPLAY_VAR: &str = "WAYLAND_DISPLAY";
const DEFAULT_WAYLAND_DISPLAY: &str = "wayland-0";


#[derive(Debug)]
pub struct Connection {
    socket: UnixStream,
    send_lock: Mutex<()>,
    recv_lock: Mutex<()>,
    next_object_id: AtomicU32,
}
impl Connection {
    pub async fn new_from_env() -> Result<Self, Error> {
        let runtime_dir = env::var_os(RUNTIME_DIR_VAR)
            .ok_or_else(|| Error::MissingEnvVar { name: RUNTIME_DIR_VAR.to_owned() })?;
        let wayland_display = env::var_os(WAYLAND_DISPLAY_VAR)
            .unwrap_or_else(|| OsString::from(DEFAULT_WAYLAND_DISPLAY));
        let mut wayland_display_path = PathBuf::from(&runtime_dir);
        wayland_display_path.push(&wayland_display);

        let socket = UnixStream::connect(&wayland_display_path).await?;
        Ok(Self {
            socket,
            send_lock: Mutex::new(()),
            recv_lock: Mutex::new(()),
            next_object_id: AtomicU32::new(1),
        })
    }

    pub async fn send_packet(&self, packet: &Packet) -> Result<(), Error> {
        let serialized = packet.serialize()?;

        {
            let send_guard = self.send_lock.lock().await;

            // SocketFdExt functions handle WouldBlock for us
            let mut total_sent = self.socket
                .send_with_fds(&serialized, packet.fds()).await?;

            while total_sent < serialized.len() {
                // send more
                let now_sent = self.socket.send(&serialized[total_sent..]).await?;
                total_sent += now_sent;
            }

            drop(send_guard);
        }

        Ok(())
    }

    pub async fn recv_packet(&self) -> Result<Packet, Error> {
        let packet = {
            let recv_guard = self.recv_lock.lock().await;

            // sender ID, size, opcode
            let mut fixed_buf = [0u8; 8];

            // SocketFdExt functions handle WouldBlock for us
            let (mut total_received, fds) = self.socket
                .recv_with_fds(&mut fixed_buf).await?;
            while total_received < fixed_buf.len() {
                // receive more
                let now_received = self.socket
                    .recv(&mut fixed_buf[total_received..]).await?;
                total_received += now_received;
            }

            let object_id = u32::from_ne_bytes(fixed_buf[0..4].try_into().unwrap());
            let size_and_opcode = u32::from_ne_bytes(fixed_buf[4..8].try_into().unwrap());
            let packet_size: usize = (size_and_opcode >> 16).try_into().unwrap();
            let opcode: u16 = (size_and_opcode & 0xFF).try_into().unwrap();

            if packet_size < 8 {
                // 8 bytes are the fixed header and thereby the minimum
                return Err(Error::PacketTooShort { actual: packet_size, minimum: 8 });
            }

            // read the payload
            let mut payload = vec![0u8; packet_size - 8];
            total_received = self.socket
                .recv(&mut payload).await?;
            while total_received < payload.len() {
                let now_received = self.socket
                    .recv(&mut payload[total_received..]).await?;
                total_received += now_received;
            }

            drop(recv_guard);

            Packet::new_from_existing(
                object_id,
                opcode,
                payload,
                fds,
            )
        };

        Ok(packet)
    }
}
