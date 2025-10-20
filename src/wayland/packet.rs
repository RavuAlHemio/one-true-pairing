use std::fmt;
use std::io;
use std::mem::{size_of, size_of_val};
use std::num::NonZero;
use std::os::fd::{AsRawFd, RawFd};
use std::ptr::null_mut;

use libc::{cmsghdr, iovec, msghdr, SCM_RIGHTS, sendmsg, SOL_SOCKET};
use tokio::io::{AsyncWriteExt, Interest};
use tokio::net::UnixStream;

use crate::wayland::fixed::Fixed;


#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    TooLong { actual: usize, maximum: usize },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e)
                => write!(f, "I/O error: {}", e),
            Self::TooLong { actual, maximum }
                => write!(f, "packet too long ({} bytes > maximum {} bytes)", actual, maximum),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::TooLong { .. } => None,
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self { Self::Io(value) }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WaylandPacket {
    object_id: u32,
    // top_size_bytes_bottom_opcode: u32
    opcode: u16, // merged with size in protocol
    payload: Vec<u8>,
    fds: Vec<RawFd>,
}
impl WaylandPacket {
    pub fn new(
        object_id: u32,
        opcode: u16,
    ) -> Self {
        Self {
            object_id,
            opcode,
            payload: Vec::new(),
            fds: Vec::new(),
        }
    }

    pub fn object_id(&self) -> u32 { self.object_id }
    pub fn opcode(&self) -> u16 { self.opcode }
    
    pub fn set_object_id(&mut self, new_value: u32) { self.object_id = new_value; }
    pub fn set_opcode(&mut self, new_value: u16) { self.opcode = new_value; }

    pub fn push_uint(&mut self, value: u32) {
        let bs = value.to_ne_bytes();
        self.payload.extend(&bs);
    }

    pub fn push_int(&mut self, value: i32) {
        let bs = value.to_ne_bytes();
        self.payload.extend(&bs);
    }

    pub fn push_fixed(&mut self, value: Fixed) {
        let bs = value.inner_value().to_ne_bytes();
        self.payload.extend(&bs);
    }

    pub fn push_str(&mut self, value: &str) {
        assert!(!value.contains("\0"));
        let len_with_nul = value.len() + 1;
        let lwn_u32: u32 = len_with_nul.try_into().unwrap();
        self.payload.extend(&lwn_u32.to_ne_bytes());
        self.payload.extend(value.as_bytes());
        self.payload.push(0x00);

        // align to 4 bytes
        let realign_count = (4 - (len_with_nul % 4)) % 4;
        self.payload.extend(std::iter::repeat_n(0x00, realign_count));
    }

    pub fn push_object(&mut self, obj_id: Option<NonZero<u32>>) {
        match obj_id {
            Some(oi) => self.push_uint(oi.into()),
            None => self.push_uint(0),
        }
    }

    pub fn push_fd(&mut self, fd: RawFd) {
        self.fds.push(fd);
    }

    pub fn clear_payload(&mut self) {
        self.payload.clear();
        self.fds.clear();
    }

    pub fn serialize(&self) -> Result<Vec<u8>, Error> {
        let total_bytes = 8 + self.payload.len();
        let max_size: usize = u16::MAX.into();
        if total_bytes > max_size {
            return Err(Error::TooLong {
                actual: total_bytes,
                maximum: max_size,
            });
        }
        let size_u16: u16 = total_bytes.try_into().unwrap();
        let top_size_bytes_bottom_opcode =
            (u32::from(size_u16) << 16)
            | (u32::from(self.opcode) <<  0)
        ;

        let mut buf = vec![0u8; total_bytes];
        buf[0..4].copy_from_slice(&self.object_id.to_ne_bytes());
        buf[4..8].copy_from_slice(&top_size_bytes_bottom_opcode.to_ne_bytes());
        buf[8..].copy_from_slice(&self.payload);
        Ok(buf)
    }

    pub async fn send(&self, socket: &mut UnixStream) -> Result<(), Error> {
        let mut buf = self.serialize()?;

        if self.fds.len() == 0 {
            socket.write(&buf).await?;
        } else {
            // well, this won't be easy

            // assemble the "additional stuff" structure
            let add_stuff_header_len = size_of::<cmsghdr>();
            let add_stuff_payload_len = self.fds.len() * size_of::<RawFd>();
            let add_stuff_len = add_stuff_header_len + add_stuff_payload_len;
            let mut add_stuff_buf = vec![0u8; add_stuff_len];
            let header = cmsghdr {
                cmsg_len: add_stuff_len,
                cmsg_level: SOL_SOCKET,
                cmsg_type: SCM_RIGHTS,
            };
            unsafe {
                write_val_as_bytes(
                    &header,
                    &mut add_stuff_buf[..add_stuff_header_len],
                );
                write_slice_as_bytes(
                    self.fds.as_slice(),
                    &mut add_stuff_buf[add_stuff_header_len..],
                );
            }

            // collect it in the struct
            let mut iov = iovec {
                iov_base: buf.as_mut_ptr() as *mut _,
                iov_len: buf.len(),
            };
            let add_struct = msghdr {
                msg_name: null_mut(),
                msg_namelen: 0,
                msg_iov: &mut iov,
                msg_iovlen: 1,
                msg_control: add_stuff_buf.as_mut_ptr() as *mut _,
                msg_controllen: add_stuff_len,
                msg_flags: 0,
            };

            // wait until we are ready to send
            socket.writable().await?;

            // grab the file descriptor
            let fd: RawFd = socket.as_raw_fd();

            // blammo
            socket.try_io(
                Interest::WRITABLE,
                || {
                    let sent = unsafe {
                        sendmsg(fd, &add_struct, 0)
                    };
                    if sent == -1 {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                },
            )?;
        }

        Ok(())
    }
}

unsafe fn write_val_as_bytes<T>(value: &T, buf: &mut [u8]) {
    let size = size_of_val(value);
    assert_eq!(size, buf.len());
    let ptr_t = value as *const T;
    let ptr_b = ptr_t as *const u8;
    let slice_b = unsafe {
        std::slice::from_raw_parts(
            ptr_b,
            size,
        )
    };
    buf.copy_from_slice(slice_b);
}

unsafe fn write_slice_as_bytes<T>(value: &[T], buf: &mut [u8]) {
    if value.len() == 0 {
        return;
    }

    let size1 = size_of_val(&value[0]);
    let size = value.len() * size1;
    assert_eq!(size, buf.len());
    let ptr_b = value.as_ptr() as *const u8;
    let slice_b = unsafe {
        std::slice::from_raw_parts(
            ptr_b,
            size,
        )
    };
    buf.copy_from_slice(slice_b);
}
