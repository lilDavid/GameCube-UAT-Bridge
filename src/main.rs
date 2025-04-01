use std::{env, error::Error, fmt::{Debug, Display, Write}, io::{self, Read, Write as _}, net::{Ipv4Addr, TcpStream}, num::TryFromIntError, thread};

use json::{array, object, JsonValue};
use websocket::{Message, OwnedMessage};

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

const NINTENDONT_PORT: u16 = 43673;

const UAT_PORT_MAIN: u16 = 65399;
const UAT_PROTOCOL_VERSION: i32 = 0;

const GCN_GAME_ID_ADDRESS: u32 = 0x80000000;

const PRIME_GAME_STATE_ADDRESS: u32 = 0x805A8C40;
const PRIME_WORLD_OFFSET: i32 = 0x84;

fn main() -> Result<(), Box<dyn Error>> {
    let mut argv = env::args();
    argv.next();  // Consume argv[0]

    let address = argv.next().expect("Need IP address");
    println!("Connecting to Nintendont at {}...", address);
    let mut nintendont_socket = TcpStream::connect((address, NINTENDONT_PORT))?;
    println!("Connected");

    request_meta_info(&mut nintendont_socket)?;

    let result = read_memory(&mut nintendont_socket, &[GCN_GAME_ID_ADDRESS, GCN_GAME_ID_ADDRESS + 4])?;
    let game_id = String::from_utf8(result[0..6].into())?;
    let game_revision = result[7];
    println!(">> Game ID: {}", game_id);
    println!(">> Revision: {}", game_revision);

    let result = read_memory(&mut nintendont_socket, &[PRIME_GAME_STATE_ADDRESS])?;
    let address = u32::from_be_bytes([result[0], result[1], result[2], result[3]]);
    let result = read_memory(&mut nintendont_socket, &[address + PRIME_WORLD_OFFSET as u32])?;
    let world = u32::from_be_bytes([result[0], result[1], result[2], result[3]]);
    println!(">> Game world: {}", world);

    let uat_server = websocket::server::sync::Server::bind((Ipv4Addr::LOCALHOST, UAT_PORT_MAIN))?;
    for connection in uat_server.filter_map(Result::ok) {
        thread::spawn(move || {
            let mut client = connection.accept().unwrap();

            client.send_message(&Message::text(json::stringify(array![object!{
                cmd: "Info",
                protocol: UAT_PROTOCOL_VERSION,
                name: "Metroid Prime",
                version: "0-00",
                features: array![],
                slots: array![],
            }]))).unwrap();

            loop {
                let message = client.recv_message().unwrap();

                let data = match message {
                    OwnedMessage::Text(text) => text,
                    OwnedMessage::Binary(_) => todo!(),
                    OwnedMessage::Ping(data) => { client.send_message(&Message::pong(data)).unwrap(); continue },
                    OwnedMessage::Pong(_) => continue,
                    OwnedMessage::Close(_) => break,
                };
                let frame = json::parse(&data).expect("Received invalid JSON");

                for command in frame.members() {
                    let v = &command["cmd"];
                    if v != "Sync" {
                        todo!();
                    }

                    let response = json::stringify(array![object!{
                        cmd: "Var",
                        name: "world",
                        value: world,
                    }]);
                    client.send_message(&Message::text(response)).expect("Could not send message");
                }
            }
        });
    }

    Ok(())
}
