mod connector;
mod lua;
mod uat;

use std::{env, error::Error, io::ErrorKind, net::{IpAddr, Ipv4Addr}, str::FromStr, sync::{mpsc::{channel, Sender}, Arc, Mutex}, thread, time::Duration};

use connector::GameCubeConnector;
use json::JsonValue;
use lua::LuaInterface;
use uat::{command::{ClientCommand, ServerCommand}, variable::VariableStore, MessageResponse, Server};
use websocket::WebSocketError;

#[cfg(target_os = "windows")]
use crate::connector::dolphin::DolphinConnector;
use crate::connector::nintendont::NintendontConnector;

#[derive(Debug, Clone)]
struct VariableWatch {
    name: String,
    value: JsonValue,
}

#[cfg(target_os = "windows")]
fn get_dolphin_connector() -> Result<Box<dyn GameCubeConnector + Send + Sync>, &'static str> {
    let result = loop {
        println!("Connecting to Dolphin...");
        match DolphinConnector::new() {
            Ok(dolphin) => break Box::new(dolphin),
            Err(err) => {eprintln!("{}", err); thread::sleep(Duration::from_secs(1))},
        }
    };
    println!("Connected");
    Ok(result)
}

#[cfg(not(target_os = "windows"))]
fn get_dolphin_connector() -> Result<Box<dyn GameCubeConnector + Send + Sync>, &'static str> {
    Err("Dolphin is not supported on this platform")
}

fn get_nintendont_connector(address: &str) -> Box<dyn GameCubeConnector + Send + Sync> {
    println!("Connecting to Nintendont at {}...", address);
    let result = loop {
        match NintendontConnector::new(IpAddr::from_str(address).unwrap()) {
            Ok(nintendont) => break Box::new(nintendont),
            Err(err) => {eprintln!("{}", err); thread::sleep(Duration::from_secs(1))},
        }
    };
    println!("Connected");
    result
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut argv = env::args();
    argv.next();  // Consume argv[0]

    let target = argv.next().ok_or("Need IP Address or to specify Dolphin")?;

    let connector: Box<dyn GameCubeConnector + Send + Sync> = if target.to_lowercase() == "dolphin" {
        get_dolphin_connector().unwrap()
    } else {
        get_nintendont_connector(&target)
    };

    let mut lua_interface = LuaInterface::new(connector)?;
    for arg in argv {
        lua_interface.run_script(arg)?;
    }

    let interface = match lua_interface.select_game_interface() {
        Some((name, interface)) => { println!("Found interface {} for {}", name, interface.name()?.unwrap_or_else(|| "<nil>".into())); interface },
        None => Err("No interface found")?
    };

    let game_info_command = vec![ServerCommand::info(interface.name()?.as_deref(), interface.version()?.as_deref())];

    let client_message_channels: Arc<Mutex<Vec<Sender<Vec<VariableWatch>>>>> = Arc::new(Mutex::new(Vec::new()));
    let channels = Arc::clone(&client_message_channels);
    let variable_store = VariableStore::new();
    let variable_store: Arc<Mutex<VariableStore>> = Arc::new(Mutex::new(variable_store));
    let variables = Arc::clone(&variable_store);
    thread::spawn(move || {
        let client_message_channels = channels;
        let variable_store = variables;

        loop {
            match lua_interface.run_game_watcher() {
                Some(Ok(updates)) => {
                    Some(updates)
                }
                Some(Err(e)) => { eprintln!("{}", e); None }
                None => None
            }.map(|changes|
                changes.into_iter()
                .filter_map(|(k, res)| match res {
                    Ok(v) => Some((k, v)),
                    Err(e) => { eprintln!("{}", e); None },
                })
                .filter(|(name, value)| {
                    let mut variables = variable_store.lock().unwrap();
                    variables.update_variable(&name, value.clone())
                })
                .inspect(|(name, value)| {
                    println!(":{} = {}", name, value)
                })
                .map(|(name, value)| VariableWatch { name: name.to_owned(), value: value })
                .collect::<Vec<_>>()
            ).map(|changes|
                for channel in client_message_channels.lock().unwrap().iter() {
                    channel.send(changes.clone()).ok();
                }
            );

            thread::sleep(Duration::from_secs(1));
        }
    });

    let uat_server = Server::new(Ipv4Addr::LOCALHOST)?;
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
                        ClientCommand::Sync(_) => variable_store.lock().unwrap()
                            .variable_values()
                            .map(|(name, value)| ServerCommand::var(name, value.clone()))
                            .collect::<Vec<_>>(),
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
