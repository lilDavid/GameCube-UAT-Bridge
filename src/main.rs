use std::{env, error::Error, fmt::{Debug, Display, Write}, io::{self, Read, Write as _}, net::TcpStream, num::TryFromIntError};

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

    const READ: u8 = 0x80;
    const WRITE: u8 = 0x40;
    const WORD: u8 = 0x20;
}

fn hex_string<'iterable>(data: impl IntoIterator<Item = &'iterable u8>) -> String {
    let iter = data.into_iter();
    let mut result = String::with_capacity(2 * match iter.size_hint() {
        (_, Some(max)) => max,
        (min, None) => min,
    });

    for &byte in iter {
        write!(&mut result, "{:02X}", byte).expect("Could not write");
    }

    return result;
}

fn write_to_socket(socket: &mut TcpStream, data: &[u8]) -> Result<Vec<u8>, io::Error> {
    println!("\nSent: {} {}", socket.write(data)?, hex_string(data));

    let mut buffer = [0; 1024];
    let response = socket.read(&mut buffer)?;
    let result = Vec::from(&buffer[..response]);
    println!("Response: {}", hex_string(&result));

    Ok(result)
}

fn request_meta_info(socket: &mut TcpStream) -> Result<(), io::Error>{
    let result = write_to_socket(socket, &[1, 0, 0, 1])?;

    let mut i = 0;
    let mut values = Vec::new();
    while let Some(bytes) = result.get(i .. i + 4) {
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
        values.push(u32::from_be_bytes(bytes));
        i += 4;
    }

    println!("> API version: {}", values.get(0).expect("Could not find API version"));
    println!("> Max input bytes: {}", values.get(1).expect("Could not find max input bytes"));
    println!("> Max output bytes: {}", values.get(2).expect("Could not find max output bytes"));
    println!("> Max addresses: {}", values.get(3).expect("Could not find max addresses"));

    Ok(())
}

#[derive(Debug)]
#[allow(dead_code)]
enum SendSocketError {
    TooManyAddresses(TryFromIntError),
    TooManyOps(TryFromIntError),
    IOError(io::Error),
}

impl Display for SendSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

impl Error for SendSocketError {}

impl From<io::Error> for SendSocketError {
    fn from(err: io::Error) -> Self {
        Self::IOError(err)
    }
}

#[allow(dead_code)]
fn send_socket(socket: &mut TcpStream, addresses: &[u32], ops: &[Op]) -> Result<Vec<u8>, SendSocketError> {
    let mut data: Vec<u8> = vec![
        0,
        ops.len().try_into().map_err(SendSocketError::TooManyOps)?,
        addresses.len().try_into().map_err(SendSocketError::TooManyAddresses)?,
        1
    ];
    addresses.iter().copied().map(u32::to_be_bytes).flatten().for_each(|byte| data.push(byte));
    for op in ops {
        let mut op_byte = op.address_index;
        if op.has_read() {
            op_byte |= Op::READ;
        }
        if op.has_write() {
            op_byte |= Op::WRITE;
        }
        if op.is_word() {
            op_byte |= Op::WORD;
        }
        data.push(op_byte);
        if let Some(write_value) = op.write_value {
            data.extend_from_slice(&write_value.to_be_bytes());
        }
    }

    let response = write_to_socket(socket, &data)?;

    let mut last_validation_index = 0;
    for i in 0..ops.len() {
        last_validation_index = i / 8;
        if response[last_validation_index] & (1 << (i % 8)) == 0 {
            panic!("Op {} used an invalid address", i);
        }
    }

    Ok(Vec::from(&response[last_validation_index + 1 ..]))
}

fn read_memory(socket: &mut TcpStream, ops: &[u32]) -> Result<Vec<u8>, SendSocketError> {
    let mut addresses = Vec::new();
    for i in 0..ops.len() {
        addresses.push(Op::new(i as u8, None))
    }
    send_socket(socket, ops, &addresses)
}

const PORT: u16 = 43673;

const GCN_GAME_ID_ADDRESS: u32 = 0x80000000;

fn main() -> Result<(), Box<dyn Error>> {
    let mut argv = env::args();
    argv.next();  // Consume argv[0]

    let address = argv.next().expect("Need IP address");
    println!("Connecting to Nintendont at {}...", address);
    let mut stream = TcpStream::connect((address, PORT))?;
    println!("Connected");

    request_meta_info(&mut stream)?;

    let result = read_memory(&mut stream, &[GCN_GAME_ID_ADDRESS, GCN_GAME_ID_ADDRESS + 4])?;
    let game_id = String::from_utf8(result[0..6].into())?;
    let game_revision = result[7];
    println!(">> Game ID: {}", game_id);
    println!(">> Revision: {}", game_revision);

    Ok(())
}
