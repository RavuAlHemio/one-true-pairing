use std::ffi::c_void;
use std::fmt;
use std::io;
use std::mem::{size_of, size_of_val};
use std::num::NonZero;
use std::os::fd::{AsRawFd, RawFd};
use std::ptr::null_mut;

use libc::CMSG_DATA;
use libc::CMSG_FIRSTHDR;
use libc::CMSG_LEN;
use libc::CMSG_SPACE;
use libc::{cmsghdr, iovec, msghdr, recvmsg, SCM_RIGHTS, sendmsg, SOL_SOCKET};
use tokio::io::Interest;
use tokio::net::UnixStream;

use crate::wayland::fixed::Fixed;


#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    TooLong { actual: usize, maximum: usize },
    TooShort { actual: usize, minimum: usize },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e)
                => write!(f, "I/O error: {}", e),
            Self::TooLong { actual, maximum }
                => write!(f, "packet too long ({} bytes > maximum {} bytes)", actual, maximum),
            Self::TooShort { actual, minimum }
                => write!(f, "packet too short ({} bytes < minimum {} bytes)", actual, minimum),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::TooLong { .. } => None,
            Self::TooShort { .. } => None,
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self { Self::Io(value) }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Packet {
    object_id: u32,
    // top_size_bytes_bottom_opcode: u32
    opcode: u16, // merged with size in protocol
    payload: Vec<u8>,
    fds: Vec<RawFd>,
}
impl Packet {
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

    pub async fn send(&self, socket: &UnixStream) -> Result<(), Error> {
        let mut buf = self.serialize()?;

        // we could use the Rust-only happy path if we have no file descriptors to send
        // it's probably better for debugging if there is only one code path though

        // assemble the general message structure including the buffer for "additional stuff"
        let add_stuff_payload_len = self.fds.len() * size_of::<RawFd>();
        let add_stuff_len: usize = unsafe {
            CMSG_SPACE(
                add_stuff_payload_len.try_into().unwrap()
            ).try_into().unwrap()
        };
        let mut add_stuff_buf = vec![0u8; add_stuff_len];
        let mut iov = iovec {
            iov_base: buf.as_mut_ptr() as *mut c_void,
            iov_len: buf.len(),
        };
        let add_struct = msghdr {
            msg_name: null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: add_stuff_buf.as_mut_ptr() as *mut c_void,
            msg_controllen: add_stuff_len,
            msg_flags: 0,
        };

        unsafe {
            // get the header of the first additional-stuff value
            let add_first_header = CMSG_FIRSTHDR(&add_struct);

            // populate it
            (*add_first_header).cmsg_level = SOL_SOCKET;
            (*add_first_header).cmsg_type = SCM_RIGHTS;
            (*add_first_header).cmsg_len = CMSG_LEN(
                add_stuff_payload_len.try_into().unwrap()
            ).try_into().unwrap();

            // get the location of its data and write the FDs
            let data_ptr = CMSG_DATA(add_first_header);
            let data_ptr_slice = std::slice::from_raw_parts_mut(data_ptr, add_stuff_payload_len);
            write_slice_as_bytes(
                self.fds.as_slice(),
                data_ptr_slice,
            );
        }

        // grab the file descriptor
        let fd: RawFd = socket.as_raw_fd();

        // send first chunk (including file descriptors)
        let mut total_sent = loop {
            // wait until we are ready to send
            socket.writable().await?;

            let send_res: Result<usize, io::Error> = socket.try_io(
                Interest::WRITABLE,
                || {
                    let sent = unsafe {
                        sendmsg(fd, &add_struct, 0)
                    };
                    if sent == -1 {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(sent.try_into().unwrap())
                    }
                },
            );
            match send_res {
                Ok(n) => break n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // try again
                    continue;
                },
                Err(e) => return Err(e.into()),
            }
        };

        // keep trying until we get all of it out
        while total_sent < buf.len() {
            // wait for readiness again
            socket.writable().await?;

            let now_sent = match socket.try_write(&buf[total_sent..]) {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    continue;
                },
                Err(e) => return Err(e.into()),
            };
            total_sent += now_sent;
        }

        Ok(())
    }

    pub async fn recv(socket: &UnixStream) -> Result<Packet, Error> {
        // start by receiving the fixed part: object ID, length and opcode
        // as well as any FDs
        let mut fixed_buf = [0u8; 8];

        let mut iov = iovec {
            iov_base: fixed_buf.as_mut_ptr() as *mut c_void,
            iov_len: fixed_buf.len(),
        };
        // let's hope 4M is big enough for the additional stuff
        let mut add_stuff_buf = vec![0u8; 4*1024*1024];
        let mut msg = msghdr {
            msg_name: null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: add_stuff_buf.as_mut_ptr() as *mut c_void,
            msg_controllen: add_stuff_buf.len(),
            msg_flags: 0,
        };

        let fd = socket.as_raw_fd();

        // and here we go again
        let mut total_received = loop {
            socket.readable().await?;

            let receive_res: Result<usize, io::Error> = socket.try_io(
                Interest::READABLE,
                || {
                    let received = unsafe {
                        recvmsg(fd, &mut msg, 0)
                    };
                    if received == -1 {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(received.try_into().unwrap())
                    }
                },
            );
            match receive_res {
                Ok(n) => break n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // try again
                    continue;
                },
                Err(e) => return Err(e.into()),
            }
        };

        // okay, we received all the file descriptors we are going to receive
        // find them (if there are any)
        unsafe {
            let first_add_header = CMSG_FIRSTHDR(&msg);
            todo!();
        }
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
