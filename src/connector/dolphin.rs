use std::sync::Mutex;

use dolphin_memory::Dolphin;

use crate::connector::{GameCubeConnector, GameCubeConnectorError};

pub struct DolphinConnector {
    dolphin: Mutex<Dolphin>
}

impl DolphinConnector {
    pub fn new() -> Result<Self, dolphin_memory::ProcessError> {
        Ok(Self { dolphin: Mutex::new(Dolphin::new()?) })
    }
}

impl GameCubeConnector for DolphinConnector {
    fn read_address(&mut self, size: u32, address: u32) -> Result<Vec<u8>, GameCubeConnectorError> {
        self.read_pointers(size, address, &[])
    }

    fn read_pointers(&mut self, size: u32, address: u32, offsets: &[i32]) -> Result<Vec<u8>, GameCubeConnectorError> {
        // For some reason, passing an empty Vec instead of None causes read() to always return a null address error
        let offsets = if offsets.len() == 0 {
            None
        } else {
            Some(offsets.iter().copied().map(|i| i as isize as usize).collect::<Vec<usize>>())
        };
        Ok(self.dolphin.lock().unwrap().read(size as usize, address as usize, offsets.as_ref().map(Vec::as_slice))?)
    }
}
