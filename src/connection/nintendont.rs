use std::{cell::RefCell, io::{self, Cursor, ErrorKind, Read as _, Write}, mem, net::{IpAddr, TcpStream}};

use super::{GameCubeConnection, Read};


#[repr(u8)]
enum MemoryOperationType {
    ReadCommands = 0,
    RequestVersion = 1,
}

#[repr(C, packed(1))]
struct MemoryOperationHeader {
    operation_type: MemoryOperationType,
    count: u8,
    absolute_address_count: u8,
    keep_alive: u8,
}

impl MemoryOperationHeader {
    pub fn new(operation_type: MemoryOperationType, count: u8, absolute_address_count: u8, keep_alive: bool) -> Self {
        Self { operation_type, count, absolute_address_count, keep_alive: keep_alive as u8 }
    }

    pub fn read_commands(count: u8, absolute_address_count: u8) -> Self {
        Self::new(MemoryOperationType::ReadCommands, count, absolute_address_count, true)
    }

    pub fn request_version() -> Self {
        Self::new(MemoryOperationType::RequestVersion, 0, 0, true)
    }

    pub fn into_bytes(self) -> Vec<u8> {
        vec![self.operation_type as u8, self.count, self.absolute_address_count, self.keep_alive]
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
struct OperationHeader(u8);

impl OperationHeader {
    const HAS_READ: u8 = 0x80;
    const HAS_WRITE: u8 = 0x40;
    const IS_WORD: u8 = 0x20;
    const HAS_OFFSET: u8 = 0x10;
    const ADDRESS_INDEX_MASK: u8 = 0xF;

    pub fn new(/* has_read: bool, */ /* has_write: bool, */ is_word: bool, has_offset: bool, address_index: u8) -> Self {
        let mut this = Self(0);
        this.set_has_read(true);
        this.set_has_write(false);
        this.set_is_word(is_word);
        this.set_has_offset(has_offset);
        this.set_address_index(address_index);
        this
    }

    pub fn as_byte(&self) -> u8 {
        self.0
    }

    #[allow(unused)]
    fn get_bit(&self, bit: u8) -> bool{
        return self.0 & bit != 0
    }

    #[allow(unused)]
    pub fn has_read(&self) -> bool {
        self.get_bit(Self::HAS_READ)
    }

    #[allow(unused)]
    pub fn has_write(&self) -> bool {
        self.get_bit(Self::HAS_WRITE)
    }

    #[allow(unused)]
    pub fn is_word(&self) -> bool {
        self.get_bit(Self::IS_WORD)
    }

    #[allow(unused)]
    pub fn has_offset(&self) -> bool {
        self.get_bit(Self::HAS_OFFSET)
    }

    #[allow(unused)]
    pub fn address_index(&self) -> u8 {
        return self.0 & Self::ADDRESS_INDEX_MASK
    }

    fn set_bit(&mut self, bit: u8, value: bool) {
        if value {
            self.0 |= bit
        } else {
            self.0 &= !bit
        }
    }

    pub fn set_has_read(&mut self, has_read: bool) {
        self.set_bit(Self::HAS_READ, has_read);
    }

    pub fn set_has_write(&mut self, has_write: bool) {
        self.set_bit(Self::HAS_WRITE, has_write);
    }

    pub fn set_is_word(&mut self, is_word: bool) {
        self.set_bit(Self::IS_WORD, is_word); }

    pub fn set_has_offset(&mut self, has_offset: bool) {
        self.set_bit(Self::HAS_OFFSET, has_offset);
    }

    pub fn set_address_index(&mut self, address_index: u8) {
        if address_index & !Self::ADDRESS_INDEX_MASK != 0 { panic!("invalid address index: {}", address_index) }
        self.0 &= !Self::ADDRESS_INDEX_MASK;
        self.0 |= address_index;
    }
}

fn write_to_socket(socket: &mut TcpStream, data: &[u8]) -> Result<Vec<u8>, io::Error> {
    socket.write(data)?;
    let mut buffer = [0; 1024];
    let response = socket.read(&mut buffer)?;
    let result = Vec::from(&buffer[..response]);
    Ok(result)
}

pub struct NitendontConnectionInfo {
    #[allow(unused)] protocol_version: u32,
    max_input_bytes: u32,
    #[allow(unused)] max_output_bytes: u32,
    max_addresses: u32,
}

impl NitendontConnectionInfo {
    fn get(socket: &mut TcpStream) -> Result<Self, io::Error> {
        let mut cursor = Cursor::new(write_to_socket(socket, &MemoryOperationHeader::request_version().into_bytes())?);
        let mut bytes = [0u8; 4];
        Ok(Self {
            protocol_version: { cursor.read_exact(bytes.as_mut_slice())?; u32::from_be_bytes(bytes) },
            max_input_bytes: { cursor.read_exact(bytes.as_mut_slice())?; u32::from_be_bytes(bytes) },
            max_output_bytes: { cursor.read_exact(bytes.as_mut_slice())?; u32::from_be_bytes(bytes) },
            max_addresses: { cursor.read_exact(bytes.as_mut_slice())?; u32::from_be_bytes(bytes) },
        })
    }
}

pub struct NintendontConnection {
    socket: RefCell<TcpStream>,
    connection_info: NitendontConnectionInfo,
}

impl NintendontConnection {
    const PORT: u16 = 43673;

    pub fn new(ip_addr: IpAddr) -> io::Result<Self> {
        let socket = RefCell::new(TcpStream::connect((ip_addr, Self::PORT))?);
        let connection_info = NitendontConnectionInfo::get(&mut socket.borrow_mut())?;
        Ok(Self {socket, connection_info})
    }
}

impl GameCubeConnection for NintendontConnection {
    fn read(&self, read_list: &[Read]) -> io::Result<Vec<Option<Vec<u8>>>> {
        let mut results = Vec::new();
        let mut result_info = Vec::new();
        let mut cursor = Cursor::new(Vec::new());
        let mut iterator = read_list.iter().peekable();
        loop {
            assert!(result_info.len() <= self.connection_info.max_addresses as usize);
            let send = if result_info.len() == self.connection_info.max_addresses as usize {
                true
            } else {
                let read = iterator.peek();
                let current_position = cursor.position();
                let index = result_info.len() as u8;
                match read {
                    Some(Read::Direct { address, size }) => {
                        result_info.push((address, size));
                        cursor.write(&[OperationHeader::new(false, false, index).as_byte(), *size])?;
                    }
                    Some(Read::Indirect { address, offset, size }) => {
                        result_info.push((address, size));
                        cursor.write(&[OperationHeader::new(false, true, index).as_byte(), *size])?;
                        cursor.write(&offset.to_be_bytes())?;
                    }
                    None => {}
                }
                if read.is_none() {
                    true
                } else if mem::size_of::<u32>() * result_info.len() + cursor.position() as usize > self.connection_info.max_input_bytes as usize {
                    // Rollback and send
                    result_info.pop();
                    cursor.set_position(current_position);
                    true
                } else {
                    false
                }
            };

            if send {
                let address_count = result_info.len() as u8;
                if address_count != 0 {
                    let mut data = MemoryOperationHeader::read_commands(address_count, address_count).into_bytes();
                    for address in &result_info {
                        data.extend_from_slice(&address.0.to_be_bytes());
                    }
                    data.extend_from_slice(cursor.get_ref());

                    assert!(data.len() <= self.connection_info.max_input_bytes as usize);
                    let mut result = write_to_socket(&mut self.socket.borrow_mut(), &data)?;
                    if result.len() == 0 {
                        return Err(io::Error::new(ErrorKind::InvalidData, "received no bytes"));
                    }

                    let mut data = Cursor::new(result.split_off(((address_count - 1) / 8 + 1) as usize));
                    let success_bytes = result;
                    for i in 0..result_info.len() {
                        let index = i / 8;
                        if success_bytes[index] & (1 << (i % 8)) == 0 {
                            results.push(None);
                        } else {
                            let mut result = vec![0u8; *result_info[i].1 as usize];
                            data.read_exact(result.as_mut_slice())?;
                            results.push(Some(result));
                        }
                    }
                }
                result_info.clear();
                cursor.set_position(0);
                if iterator.peek().is_none() {
                    break;
                }
            } else {
                iterator.next();
            }
        }

        Ok(results)
    }
}
