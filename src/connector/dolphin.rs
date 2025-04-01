use dolphin_memory::Dolphin;

use crate::connector::{GameCubeConnector, GameCubeConnectorError};

pub struct DolphinConnector {
    dolphin: Dolphin
}

impl DolphinConnector {
    pub fn new() -> Result<Self, dolphin_memory::ProcessError> {
        Ok(Self { dolphin: Dolphin::new()? })
    }
}

impl GameCubeConnector for DolphinConnector {
    fn read_address(&mut self, size: u32, address: u32) -> Result<Vec<u8>, GameCubeConnectorError> {
        self.read_pointers(size, address, &[])
    }

    fn read_pointers(&mut self, size: u32, address: u32, offsets: &[i32]) -> Result<Vec<u8>, GameCubeConnectorError> {
        Ok(self.dolphin.read(size as usize, address as usize, Some(&offsets.iter().copied().map(|i| i as isize as usize).collect::<Vec<usize>>()))?)
    }
}
