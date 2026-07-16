use std::{
    io,
    rc::{Rc, Weak},
};

use crate::connection::GameCubeConnection;

use super::Read;

mod dme {
    use std::{
        ffi::{c_char, c_int},
        io,
    };

    enum Status {
        Hooked,
        NotRunning,
        NoEmu,
        NotHooked,
    }

    extern "C" {
        fn Dolphin_getStatus() -> c_int;
        fn Dolphin_hook();
        fn Dolphin_unHook();
        fn Dolphin_isValidConsoleAddress(address: u32) -> bool;
        fn Dolphin_readBytes(address: u32, buffer: *mut c_char, size: usize) -> bool;
    }

    fn get_status() -> Status {
        match unsafe { Dolphin_getStatus() } {
            0 => Status::Hooked,
            1 => Status::NotRunning,
            2 => Status::NoEmu,
            3 => Status::NotHooked,
            other => panic!("Unexpected status value: {}", other),
        }
    }

    fn status_to_result(status: Status) -> io::Result<()> {
        match status {
            Status::Hooked => Ok(()),
            Status::NotHooked => Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Not hooked to Dolphin",
            )),
            Status::NotRunning => Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "No Dolphin process found",
            )),
            Status::NoEmu => Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Dolphin is not running",
            )),
        }
    }

    pub fn hook() -> io::Result<()> {
        // Hook if not hooked
        match get_status() {
            Status::NotHooked => unsafe {
                Dolphin_hook();
            },
            _ => {}
        }

        // Check result
        match get_status() {
            Status::NotHooked => unreachable!(),
            Status::Hooked => Ok(()),
            status => {
                unhook();
                status_to_result(status)
            }
        }
    }

    pub fn unhook() {
        unsafe {
            Dolphin_unHook();
        }
    }

    fn check_hooked() -> io::Result<()> {
        status_to_result(get_status())
    }

    fn read_memory(address: u32, size: usize) -> io::Result<Option<Vec<u8>>> {
        check_hooked()?;
        unsafe {
            if !Dolphin_isValidConsoleAddress(address) {
                return Ok(None);
            }

            let mut result: Vec<u8> = Vec::with_capacity(size);
            if !Dolphin_readBytes(address, result.as_mut_ptr() as *mut c_char, size) {
                return Ok(None);
            }
            result.set_len(size);
            Ok(Some(result))
        }
    }

    pub fn read(address: u32, size: usize, offset: Option<i16>) -> io::Result<Option<Vec<u8>>> {
        let address = match offset {
            Some(offset) => read_memory(address, 4)?
                .and_then(|v| v.try_into().ok())
                .and_then(|bytes| u32::from_be_bytes(bytes).checked_add_signed(offset as i32)),
            None => Some(address),
        };
        match address {
            Some(address) => read_memory(address, size),
            None => Ok(None),
        }
    }
}

struct Dolphin;

impl Dolphin {
    fn hook() -> Result<Self, io::Error> {
        dme::hook()?;
        Ok(Self)
    }

    fn read(&self, address: u32, size: usize, offset: Option<i16>) -> io::Result<Option<Vec<u8>>> {
        dme::read(address, size, offset)
    }
}

impl Drop for Dolphin {
    fn drop(&mut self) {
        dme::unhook();
    }
}

const DOLPHIN: Weak<Dolphin> = Weak::new();

pub struct DolphinConnection {
    dolphin: Rc<Dolphin>,
}

impl DolphinConnection {
    pub fn new() -> Result<Self, io::Error> {
        let dolphin = match DOLPHIN.upgrade() {
            Some(d) => d,
            None => Rc::new(Dolphin::hook()?),
        };
        Ok(Self { dolphin })
    }
}

impl GameCubeConnection for DolphinConnection {
    fn read(&self, read_list: &[Read]) -> io::Result<Vec<Option<Vec<u8>>>> {
        read_list
            .iter()
            .map(|read| {
                let (address, size, offset) = match read {
                    Read::Direct { address, size } => (*address, *size, None),
                    Read::Indirect {
                        address,
                        offset,
                        size,
                    } => (*address, *size, Some(*offset)),
                };
                self.dolphin.read(address, size as usize, offset)
            })
            .collect::<io::Result<Vec<_>>>()
    }
}
