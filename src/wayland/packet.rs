use std::num::NonZero;
use std::os::fd::RawFd;

use crate::wayland::error::Error;
use crate::wayland::fixed::Fixed;


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

    pub fn new_from_existing(
        object_id: u32,
        opcode: u16,
        payload: Vec<u8>,
        fds: Vec<RawFd>,
    ) -> Self {
        Self {
            object_id,
            opcode,
            payload,
            fds,
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
            return Err(Error::PacketTooLong {
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

    pub fn fds(&self) -> &[RawFd] { &self.fds }
}
