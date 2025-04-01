mod connector;
mod gamecube;
mod uat;

use std::{env, error::Error, io::ErrorKind, net::{IpAddr, Ipv4Addr}, str::FromStr, sync::{mpsc::{channel, Sender}, Arc, Mutex}, thread, time::Duration};

use connector::GameCubeConnector;
use gamecube::{GCN_GAME_ID_ADDRESS, PRIME_GAME_STATE_ADDRESS, PRIME_WORLD_OFFSET};
use uat::{command::{ClientCommand, InfoCommand, VarCommand}, variable::{Variable, VariableStore}};
use websocket::{Message, OwnedMessage, WebSocketError};

#[cfg(target_os = "windows")]
use crate::connector::dolphin::DolphinConnector;
use crate::connector::nintendont::NintendontConnector;
use crate::uat::UAT_PORT_MAIN;

#[derive(Debug, Clone)]
struct VariableWatch {
    name: String,
    value: Variable,
}

#[cfg(target_os = "windows")]
fn get_dolphin_connector() -> Result<Box<dyn GameCubeConnector>, &'static str> {
    loop {
        println!("Connecting to Dolphin...");
        match DolphinConnector::new() {
            Ok(dolphin) => break Ok(Box::new(dolphin)),
            Err(err) => eprintln!("{}", err),
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn get_dolphin_connector() -> Result<Box<dyn GameCubeConnector>, &'static str> {
    Err("Dolphin is not supported on this platform")
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut argv = env::args();
    argv.next();  // Consume argv[0]

    let target = argv.next().expect("Need IP Address or to specify Dolphin");

    let mut variable_store = VariableStore::new();
    variable_store.register_variable("world")?;

    let client_message_channels: Arc<Mutex<Vec<Sender<Vec<VariableWatch>>>>> = Arc::new(Mutex::new(Vec::new()));
    let channels = Arc::clone(&client_message_channels);
    let variable_store: Arc<Mutex<VariableStore>> = Arc::new(Mutex::new(variable_store));
    let variables = Arc::clone(&variable_store);
    thread::spawn(move || {
        let client_message_channels = channels;
        let variable_store = variables;
        let mut connector: Box<dyn GameCubeConnector> = {
            if target.to_lowercase() == "dolphin" {
                get_dolphin_connector().unwrap()
            } else {
                println!("Connecting to Nintendont at {}...", target);
                loop {
                    match NintendontConnector::new(IpAddr::from_str(&target).unwrap()) {
                        Ok(nintendont) => break Box::new(nintendont),
                        Err(err) => eprintln!("{}", err),
                    }
                }
            }
        };
        println!("Connected");

        let game_id = String::from_utf8(connector.read_address(6, GCN_GAME_ID_ADDRESS).unwrap()).unwrap();
        println!(">> Game ID: {}", game_id);
        let game_revision = String::from_utf8(connector.read_address(1, GCN_GAME_ID_ADDRESS + 6).unwrap()).unwrap();
        println!(">> Revision: {}", game_revision);

        loop {
            let changes = {
                let mut changes = vec![];
                let mut variables = variable_store.lock().unwrap();
                let world = connector.read_pointers(4, PRIME_GAME_STATE_ADDRESS, &[PRIME_WORLD_OFFSET])
                    .ok()
                    .map(|result| u32::from_be_bytes([result[0], result[1], result[2], result[3]]));
                if variables.update_variable("world", world).unwrap() {
                    if let Some(world) = world {
                        println!(">> Game world: {}", world);
                    } else {
                        println!(">> Game world: None");
                    }
                    changes.push(VariableWatch { name: "world".to_owned(), value: world.clone() });
                }
                changes
            };

            for channel in client_message_channels.lock().unwrap().iter() {
                channel.send(changes.clone()).ok();
            }

            thread::sleep(Duration::from_secs(1));
        }
    });

    let uat_server = websocket::server::sync::Server::bind((Ipv4Addr::LOCALHOST, UAT_PORT_MAIN))?;
    for connection in uat_server.filter_map(Result::ok) {
        let variable_store = Arc::clone(&variable_store);
        let (mut socket_reader, mut socket_writer) = match connection.accept() {
            Ok(client) => {
                match client.split() {
                    Ok(client) => client,
                    Err(e) => { eprintln!("Could not get reader and writer: {}", e); continue; }
                }
            },
            Err((_, e)) => {
                eprintln!("Connection error: {}", e); continue;
            },
        };
        let (item_sender, item_receiver) = channel();
        client_message_channels.lock().unwrap().push(item_sender);

        thread::spawn(move || {
            let info = vec![InfoCommand::new("Metroid Prime", Some("0-00"))];
            socket_writer.send_message(&Message::text(json::stringify(info))).unwrap();

            let socket_writer = Arc::new(Mutex::new(socket_writer));
            let writer = Arc::clone(&socket_writer);
            thread::spawn(move || {
                let writer = writer;
                for received in item_receiver {
                    let message = received.into_iter().map(|watch| VarCommand::new(&watch.name, watch.value)).collect::<Vec::<_>>();
                    if let Err(WebSocketError::IoError(err)) = writer.lock().unwrap().send_message(&Message::text(json::stringify(message))) {
                        if err.kind() != ErrorKind::BrokenPipe {
                            eprintln!("{}", err);
                        }
                        break;
                    }
                }
            });

            loop {
                let message = socket_reader.recv_message().unwrap();

                let data = match message {
                    OwnedMessage::Text(text) => text,
                    OwnedMessage::Binary(_) => todo!(),
                    OwnedMessage::Ping(data) => { socket_writer.lock().unwrap().send_message(&Message::pong(data)).unwrap(); continue },
                    OwnedMessage::Pong(_) => continue,
                    OwnedMessage::Close(_) => break,
                };
                let frame = json::parse(&data).expect("Received invalid JSON");

                for command in frame.members() {
                    let response = match ClientCommand::try_from(command) {
                        Ok(ClientCommand::Sync(_)) => variable_store.lock().unwrap().variable_values().map(|(name, value)| VarCommand::new(name, value.clone())).collect::<Vec<_>>(),
                        _ => todo!(),
                    };
                    socket_writer.lock().unwrap().send_message(&Message::text(json::stringify(response))).expect("Could not send message");
                }
            }

            socket_writer.lock().map(|w| w.shutdown_all()).ok();
        });
    }

    Ok(())
}
