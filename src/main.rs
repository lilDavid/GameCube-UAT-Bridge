mod connector;
mod gamecube;

use std::{error::Error, net::Ipv4Addr, thread};

use json::{array, object};
use websocket::{Message, OwnedMessage};

#[cfg(target_os = "windows")]
use crate::connector::dolphin::read_game_world;
#[cfg(not(target_os = "windows"))]
use crate::connector::nintendont::read_game_world;

const UAT_PORT_MAIN: u16 = 65399;
const UAT_PROTOCOL_VERSION: i32 = 0;

fn main() -> Result<(), Box<dyn Error>> {
    let world = read_game_world()?;

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
