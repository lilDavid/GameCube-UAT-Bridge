use std::io;

#[cfg(target_os = "windows")]
use dolphin_memory::Dolphin;

use crate::connection::GameCubeConnection;

#[cfg(target_os = "windows")]
pub struct DolphinConnection {
    dolphin: Dolphin
}

#[cfg(not(target_os = "windows"))]
pub enum DolphinConnection {}

impl DolphinConnection {
    #[cfg(target_os = "windows")]
    pub fn new() -> Result<Self, io::Error> {
        let dolphin = Dolphin::new().map_err(|process_error| io::Error::new(io::ErrorKind::NotFound, process_error))?;
        Ok(Self { dolphin })
    }

    #[cfg(not(target_os = "windows"))]
    #[allow(dead_code)]
    pub fn new() -> Result<Self, io::Error> {
        Err(io::Error::new(io::ErrorKind::ConnectionRefused, "Dolphin is not supported on this platform"))
    }
}

#[cfg(target_os = "windows")]
impl GameCubeConnection for DolphinConnection {
    fn read_address(&self, size: u32, address: u32) -> Result<Vec<u8>, io::Error> {
        self.read_pointers(size, address, &[])
    }

    fn read_pointers(&self, size: u32, address: u32, offsets: &[i32]) -> Result<Vec<u8>, io::Error> {
        // For some reason, passing an empty Vec instead of None causes read() to always return a null address error
        let offsets = if offsets.len() == 0 {
            None
        } else {
            Some(offsets.iter().copied().map(|i| i as isize as usize).collect::<Vec<usize>>())
        };
        Ok(self.dolphin.read(size as usize, address as usize, offsets.as_ref().map(Vec::as_slice))?)
    }
}

#[cfg(not(target_os = "windows"))]
impl GameCubeConnection for DolphinConnection {
    fn read_address(&self, _: u32, _: u32) -> Result<Vec<u8>, io::Error> {
        unreachable!()
    }

    fn read_pointers(&self, _: u32, _: u32, _: &[i32]) -> Result<Vec<u8>, io::Error> {
        unreachable!()
    }
}
