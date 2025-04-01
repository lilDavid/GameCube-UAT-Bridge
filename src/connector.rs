#[cfg(target_os = "windows")]
pub mod dolphin;
pub mod nintendont;

use std::{error::Error, fmt::Display, io};

#[derive(Debug)]
pub enum GameCubeConnectorError {
    IoError(io::Error),
    InvalidAddress(u32),
}

impl Display for GameCubeConnectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(err) => err.fmt(f),
            Self::InvalidAddress(addr) => write!(f, "Invalid address: {}", addr)
        }
    }
}

impl Error for GameCubeConnectorError {}

impl From<io::Error> for GameCubeConnectorError {
    fn from(err: io::Error) -> Self {
        Self::IoError(err)
    }
}

pub trait GameCubeConnector {
    fn read_address(&mut self, size: u32, address: u32) -> Result<Vec<u8>, GameCubeConnectorError>;

    fn read_pointers(&mut self, size: u32, address: u32, offsets: &[i32]) -> Result<Vec<u8>, GameCubeConnectorError> {
        // Empty => read <size> bytes at <address>
        // 1 item => result <- read 4 bytes at <address>; read <size> bytes at <result> + <offset>

        let mut address = address;
        let mut offsets = offsets.into_iter().copied();
        while let Some(offset) = offsets.next() {
            let result = self.read_address(4, address)?;
            address = u32::from_be_bytes([result[0], result[1], result[2], result[3]]) + offset as u32;
        }

        self.read_address(size, address)
    }
}
