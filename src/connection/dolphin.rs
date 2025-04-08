use std::io;

#[cfg(target_os = "windows")]
use dolphin_memory::Dolphin;

use crate::connection::GameCubeConnection;

use super::Read;

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
    fn read(&self, read_list: &[Read]) -> io::Result<Vec<Option<Vec<u8>>>> {
        read_list.iter().map(|read| {
            let (&address, &size, offsets) = match read {
                Read::Direct { address, size } => (address, size, None),
                Read::Indirect { address, offset, size } => (address, size, Some([*offset as usize])),
            };

            match self.dolphin.read(
                size as usize,
                address as usize,
                offsets.as_ref().map(AsRef::as_ref)
            ) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(err)
                    if err.kind() == io::ErrorKind::InvalidData
                        && err.get_ref()
                            .map(|err| err.to_string() == "null pointer address")
                            .unwrap_or(false)
                    => Ok(None),
                Err(err) => Err(err),
            }
        }).collect::<io::Result<Vec<_>>>()
    }
}

#[cfg(not(target_os = "windows"))]
impl GameCubeConnection for DolphinConnection {
    fn read(&self, _: &[Read]) -> io::Result<Vec<Option<Vec<u8>>>> {
        unreachable!()
    }
}
