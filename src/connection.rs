#[cfg(target_os = "windows")]
pub mod dolphin;
pub mod nintendont;

use std::io;

#[derive(Clone, Debug)]
pub enum Read {
    Direct { address: u32, size: u8 },
    Indirect { address: u32, offset: i16, size: u8 },
}

impl Read {
    pub fn address(address: u32, size: u8) -> Self {
        Self::Direct { address, size }
    }

    pub fn pointer(address: u32, offset: i16, size: u8) -> Self {
        Self::Indirect {
            address,
            offset,
            size,
        }
    }

    pub fn from_parts(address: u32, size: u8, offset: Option<i16>) -> Self {
        match offset {
            None => Read::address(address, size),
            Some(offset) => Read::pointer(address, offset, size),
        }
    }
}

pub trait GameCubeConnection {
    fn read_single(&self, read: Read) -> io::Result<Option<Vec<u8>>> {
        self.read(&[read])
            .map(|slices| slices.into_iter().next().unwrap())
    }

    fn read(&self, read_list: &[Read]) -> io::Result<Vec<Option<Vec<u8>>>>;
}
