mod connector;
mod config;
mod game_interface;
mod uat;

use std::{env, error::Error, fs::File, io::{ErrorKind, Read}, net::{IpAddr, Ipv4Addr}, str::FromStr, sync::{mpsc::{channel, Sender}, Arc, Mutex}, thread, time::Duration};

use config::GameRegister;
use connector::GameCubeConnector;
use game_interface::GameCubeInterface;
use uat::{command::{ClientCommand, ServerCommand}, variable::{Variable, VariableStore}, MessageResponse, Server};
use websocket::WebSocketError;

#[cfg(target_os = "windows")]
use crate::connector::dolphin::DolphinConnector;
use crate::connector::nintendont::NintendontConnector;
use crate::uat::UAT_PORT_MAIN;

const GCN_GAME_ID_ADDRESS: u32 = 0x80000000;

#[derive(Debug, Clone)]
struct VariableWatch {
    name: String,
    value: Option<Variable>,
}

#[cfg(target_os = "windows")]
fn get_dolphin_connector() -> Result<Box<dyn GameCubeConnector + Send>, &'static str> {
    let result = loop {
        println!("Connecting to Dolphin...");
        match DolphinConnector::new() {
            Ok(dolphin) => break Box::new(dolphin),
            Err(err) => eprintln!("{}", err),
        }
    };
    println!("Connected");
    Ok(result)
}

#[cfg(not(target_os = "windows"))]
fn get_dolphin_connector() -> Result<Box<dyn GameCubeConnector + Send>, &'static str> {
    Err("Dolphin is not supported on this platform")
}

fn get_nintendont_connector(address: &str) -> Box<dyn GameCubeConnector + Send> {
    println!("Connecting to Nintendont at {}...", address);
    let result = loop {
        match NintendontConnector::new(IpAddr::from_str(address).unwrap()) {
            Ok(nintendont) => break Box::new(nintendont),
            Err(err) => eprintln!("{}", err),
        }
    };
    println!("Connected");
    result
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut argv = env::args();
    argv.next();  // Consume argv[0]

    let target = argv.next().ok_or("Need IP Address or to specify Dolphin")?;

    let mut connector: Box<dyn GameCubeConnector + Send> = if target.to_lowercase() == "dolphin" {
        get_dolphin_connector().unwrap()
    } else {
        get_nintendont_connector(&target)
    };

    let mut game_register = GameRegister::new();
    while let Some(filename) = argv.next() {
        let mut file = File::open(filename)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let json = json::parse(&contents)?;
        game_register.register_from_json(&json).map_err(|_| "Could not register game")?;
    }

    let game_id = String::from_utf8(connector.read_address(6, GCN_GAME_ID_ADDRESS)?).unwrap();
    println!(">> Game ID: {}", game_id);
    let game_revision = connector.read_address(1, GCN_GAME_ID_ADDRESS + 6)?[0];
    println!(">> Revision: {}", game_revision);

    let game_info = game_register.identify(&game_id, game_revision).ok_or("Could not identify game")?;
    let game_info_command = vec![ServerCommand::info(game_info.name(), Some(game_info.version()))];

    let mut interface = GameCubeInterface::new(
        connector,
        game_info.variables(),
    );
    let mut variable_store = VariableStore::new();
    for variable in interface.variable_definitions() {
        variable_store.register_variable(variable.name()).ok();
    }

    let client_message_channels: Arc<Mutex<Vec<Sender<Vec<VariableWatch>>>>> = Arc::new(Mutex::new(Vec::new()));
    let channels = Arc::clone(&client_message_channels);
    let variable_store: Arc<Mutex<VariableStore>> = Arc::new(Mutex::new(variable_store));
    let variables = Arc::clone(&variable_store);
    thread::spawn(move || {
        let client_message_channels = channels;
        let variable_store = variables;

        loop {
            let changes = {
                let mut variables = variable_store.lock().unwrap();
                interface.read_variables()
                    .filter_map(|(name, value)| match variables.update_variable(name, value.clone()).unwrap() {
                        true => Some(VariableWatch { name: name.to_owned(), value: value }),
                        false => None
                    })
                    .collect::<Vec<_>>()
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

        let info = game_info_command.clone();
        thread::spawn(move || {
            socket_writer.send(&info).unwrap();

            let socket_writer = Arc::new(Mutex::new(socket_writer));
            let writer = Arc::clone(&socket_writer);
            thread::spawn(move || {
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
                        ClientCommand::Sync(_) => variable_store.lock().unwrap().variable_values().map(|(name, value)| ServerCommand::var(name, value.cloned())).collect::<Vec<_>>(),
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
