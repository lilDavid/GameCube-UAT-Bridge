use std::{io::{self, ErrorKind, Read, Write}, net::{IpAddr, TcpStream}};

use super::GameCubeConnector;

struct Op {
    address_index: u8,
    write_value: Option<u32>,
}

impl Op {
    fn new(address_index: u8, write_value: Option<u32>) -> Self {
        Self {address_index, write_value}
    }

    fn has_read(&self) -> bool {
        return true;
    }

    fn has_write(&self) -> bool {
        return self.write_value.is_some();
    }

    fn is_word(&self) -> bool {
        return true;
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut op_byte = self.address_index;
        let mut buffer = Vec::new();

        if self.has_read() {
            op_byte |= Self::READ;
        }
        if self.has_write() {
            op_byte |= Self::WRITE;
        }
        if self.is_word() {
            op_byte |= Self::WORD;
        }

        buffer.push(op_byte);
        if let Some(write_value) = self.write_value {
            buffer.extend_from_slice(&write_value.to_be_bytes());
        }
        buffer
    }

    const READ: u8 = 0x80;
    const WRITE: u8 = 0x40;
    const WORD: u8 = 0x20;
}

fn write_to_socket(socket: &mut TcpStream, data: &[u8]) -> Result<Vec<u8>, io::Error> {
    socket.write(data)?;
    let mut buffer = [0; 1024];
    let response = socket.read(&mut buffer)?;
    let result = Vec::from(&buffer[..response]);
    Ok(result)
}

#[allow(dead_code)]
pub struct NitendontConnectionInfo {
    protocol_version: u32,
    max_input_bytes: u32,
    max_output_bytes: u32,
    max_addresses: u32,
}

impl NitendontConnectionInfo {
    fn get(socket: &mut TcpStream) -> Result<Self, io::Error> {
        let result = write_to_socket(socket, &[1, 0, 0, 1])?;

        let mut i = 0;
        let mut values = Vec::new();
        while let Some(bytes) = result.get(i .. i + 4) {
            let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
            values.push(u32::from_be_bytes(bytes));
            i += 4;
        }

        Ok(Self {
            protocol_version: *values.get(0).ok_or(io::Error::new(ErrorKind::InvalidData, "Could not unpack protocol info"))?,
            max_input_bytes: *values.get(1).ok_or(io::Error::new(ErrorKind::InvalidData, "Could not unpack protocol info"))?,
            max_output_bytes: *values.get(2).ok_or(io::Error::new(ErrorKind::InvalidData, "Could not unpack protocol info"))?,
            max_addresses: *values.get(3).ok_or(io::Error::new(ErrorKind::InvalidData, "Could not unpack protocol info"))?,
        })
    }
}

#[allow(dead_code)]
pub struct NintendontConnector {
    socket: TcpStream,
    connection_info: NitendontConnectionInfo,
}

impl NintendontConnector {
    const PORT: u16 = 43673;

    pub fn new(ip_addr: IpAddr) -> Result<Self, io::Error> {
        let mut socket = TcpStream::connect((ip_addr, Self::PORT))?;
        let connection_info = NitendontConnectionInfo::get(&mut socket)?;
        Ok(Self {socket, connection_info})
    }

    fn send_socket(&mut self, addresses: &[u32], ops: &[Op]) -> Result<Vec<u8>, io::Error> {
        let mut data: Vec<u8> = vec![
            0,
            ops.len().try_into().map_err(|_| io::Error::new(ErrorKind::InvalidInput, "Too many operations provided"))?,
            addresses.len().try_into().map_err(|_| io::Error::new(ErrorKind::InvalidInput, "Too many addresses provided"))?,
            1
        ];
        addresses.iter().copied().map(u32::to_be_bytes).flatten().for_each(|byte| data.push(byte));
        for op in ops {
            data.extend_from_slice(&op.to_bytes());
        }

        let response = write_to_socket(&mut self.socket, &data)?;

        let mut last_validation_index = 0;
        for i in 0..ops.len() {
            last_validation_index = i / 8;
            if response[last_validation_index] & (1 << (i % 8)) == 0 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid address"));
            }
        }

        Ok(Vec::from(&response[last_validation_index + 1 ..]))
    }

    fn read_memory(&mut self, ops: &[u32]) -> Result<Vec<u8>, io::Error> {
        let mut addresses = Vec::new();
        for i in 0..ops.len() {
            addresses.push(Op::new(i as u8, None))
        }
        self.send_socket(ops, &addresses)
    }
}

impl GameCubeConnector for NintendontConnector {
    fn read_address(&mut self, size: u32, address: u32) -> Result<Vec<u8>, io::Error> {
        let residual = address % 4;
        let size = size + residual;
        let address = address - residual;
        let mut addresses = Vec::new();
        for i in (0..size).step_by(4) {
            addresses.push(address + i);
        }
        let result = self.read_memory(&addresses)?;
        Ok(result.into_iter().skip(residual as usize).take(size as usize).collect::<Vec<_>>())
    }
}
