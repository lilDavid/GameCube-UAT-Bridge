mod connector;
mod gamecube;
mod uat;

use std::{env, error::Error, io::ErrorKind, net::{IpAddr, Ipv4Addr}, str::FromStr, sync::{mpsc::{channel, Sender}, Arc, Mutex}, thread, time::Duration};

use connector::GameCubeConnector;
use gamecube::{GCN_GAME_ID_ADDRESS, PRIME_GAME_STATE_ADDRESS, PRIME_WORLD_OFFSET};
use uat::{command::{ClientCommand, ServerCommand}, variable::{Variable, VariableStore}, MessageResponse, Server};
use websocket::WebSocketError;

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

    let uat_server = Server::new(Ipv4Addr::LOCALHOST, UAT_PORT_MAIN)?;
    for client in uat_server.accept_clients().filter_map(Result::ok) {
        let variable_store = Arc::clone(&variable_store);
        let (mut socket_reader, mut socket_writer) = client.split()?;
        let (item_sender, item_receiver) = channel();
        client_message_channels.lock().unwrap().push(item_sender);

        thread::spawn(move || {
            let info = vec![ServerCommand::info("Metroid Prime", Some("0-00"))];
            socket_writer.send(&info).unwrap();

            let socket_writer = Arc::new(Mutex::new(socket_writer));
            let writer = Arc::clone(&socket_writer);
            thread::spawn(move || {
                let writer = writer;
                for received in item_receiver {
                    let message = received.into_iter().map(|watch| ServerCommand::var(&watch.name, watch.value)).collect::<Vec::<_>>();
                    if let Err(WebSocketError::IoError(err)) = writer.lock().unwrap().send(&message) {
                        if err.kind() != ErrorKind::BrokenPipe {
                            eprintln!("{}", err);
                        }
                        break;
                    }
                }
            });

            loop {
                let commands = match socket_reader.receive() {
                    Ok(commands) => commands,
                    Err(err) => match socket_writer.lock().unwrap().handle_error(err) {
                        MessageResponse::Continue => continue,
                        MessageResponse::Stop(err) => {
                            if let Some(e) = err { eprintln!("{}", e); }
                            break;
                        }
                    }
                };
                for command in commands {
                    let response = match ClientCommand::from(command) {
                        ClientCommand::Sync(_) => variable_store.lock().unwrap().variable_values().map(|(name, value)| ServerCommand::var(name, value.clone())).collect::<Vec<_>>(),
                        _ => todo!(),
                    };
                    socket_writer.lock().unwrap().send(&response).expect("Could not send message");
                }
            }

            socket_writer.lock().map(|w| w.shutdown_all()).ok();
        });
    }

    Ok(())
}
