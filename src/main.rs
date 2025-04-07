mod connection;
mod lua;
mod uat;

use std::{env, error::Error, io::ErrorKind, net::{IpAddr, Ipv4Addr}, str::FromStr, sync::mpsc::{channel, TryRecvError}, thread::{self}, time::Duration};

use connection::GameCubeConnection;
use lua::{VerificationError, LuaInterface};
use uat::{command::{ClientCommand, ServerCommand}, variable::VariableStore, Client, Server};

#[cfg(target_os = "windows")]
use crate::connection::dolphin::DolphinConnection;
use crate::connection::nintendont::NintendontConnection;

const CONNECTION_ATTEMPT_INTERVAL: Duration = Duration::from_secs(5);
const GAME_WATCH_INTERVAL: Duration = Duration::from_millis(500);

#[cfg(target_os = "windows")]
fn connect_to_dolphin() -> Box<dyn GameCubeConnection> {
    let result = loop {
        println!("Connecting to Dolphin...");
        match DolphinConnection::new() {
            Ok(dolphin) => break Box::new(dolphin),
            Err(err) => {eprintln!("{}", err); thread::sleep(CONNECTION_ATTEMPT_INTERVAL)},
        }
    };
    println!("Connected");
    result
}

#[cfg(not(target_os = "windows"))]
fn connect_to_dolphin() -> Box<dyn GameCubeConnection> {
    panic!()
}

fn connect_to_nintendont(address: IpAddr) -> Box<dyn GameCubeConnection> {
    println!("Connecting to Nintendont at {}...", address);
    let result = loop {
        match NintendontConnection::new(address) {
            Ok(nintendont) => break Box::new(nintendont),
            Err(err) => {eprintln!("{}", err); thread::sleep(CONNECTION_ATTEMPT_INTERVAL)},
        }
    };
    println!("Connected");
    result
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut argv = env::args();
    argv.next();  // Consume argv[0]

    let target = argv.next().ok_or("Need IP Address or to specify Dolphin")?;

    let connection_factory: Box<dyn Fn() -> Box<dyn GameCubeConnection>> = if target.to_lowercase() == "dolphin" {
        if cfg!(target_os = "windows") {
            Box::new(connect_to_dolphin)
        } else {
            Err("Dolphin is not supported on this platform")?
        }
    } else {
        let address = IpAddr::from_str(&target)?;
        Box::new(move || connect_to_nintendont(address))
    };

    let lua_interface = LuaInterface::new()?;
    for arg in argv {
        lua_interface.run_script(arg)?;
    }

    let uat_server = Server::new(Ipv4Addr::LOCALHOST)?;
    let (client_sender, client_receiver) = channel();
    println!("Listening for UAT clients on port {}", uat_server.local_addr()?.port());
    thread::spawn(move || {
        for client in uat_server.accept_clients().filter_map(Result::ok) {
            if let Err(_) = client_sender.send(client) {
                break
            };
        }
        println!("Server closed");
    });

    let mut variable_store = VariableStore::new();
    let mut clients: Vec<Client> = Vec::new();
    loop {
        match lua_interface.verify_current_game() {
            Ok(_) => {}
            Err(VerificationError::NotConnected) => {}
            Err(VerificationError::VerificationFailed) => {
                println!("Current interface failed to re-verify, disconnecting.");
                lua_interface.disconnect();
            }
            Err(VerificationError::VerificationError(err)) => {
                println!("{}", err);
                println!("Current interface encountered an error while re-verifying, disconnecting.");
                lua_interface.disconnect();
            }
        }

        if !lua_interface.is_connected() {
            let connection = connection_factory();
            match lua_interface.connect(connection) {
                Ok((name, interface)) => {
                    println!(
                        "Found interface {} for {}",
                        name,
                        interface.name()
                            .unwrap_or_else(|_| Some("<invalid>".into()))
                            .unwrap_or_else(|| "<nil>".into())
                    );
                },
                Err(_) => {
                    println!("No interface found for this game");
                    thread::sleep(CONNECTION_ATTEMPT_INTERVAL);
                    continue;
                }
            };
        }

        let changes = match lua_interface.run_game_watcher() {
            Some(Ok(updates)) => updates,
            Some(Err(e)) => {
                eprintln!("{}", e);
                println!("Disconnected");
                continue
            }
            None => {
                println!("Disconnected");
                continue
            }
        }.into_iter()
            .filter_map(|(k, res)| match res {
                Ok(v) => Some((k, v)),
                Err(e) => { eprintln!("{}", e); None },
            })
            .filter(|(name, value)| variable_store.update_variable(&name, value.clone()))
            .inspect(|(name, value)| println!(":{} = {}", name, value))
            .map(|(name, value)| ServerCommand::var(&name, value))
            .collect::<Vec<_>>();

        // FIXME: Operations are entirely skipped if they block, which could be a problem for Sync responses.
        // Unsure how to fix without more threads.
        let mut cache_variables: Option<Vec<ServerCommand>> = None;
        for client in &mut clients {
            let mut replies = Vec::new();
            let mut sent_variables = false;
            match client.receive() {
                Ok(messages) => {
                    for message in messages {
                        match message {
                            Ok(ClientCommand::Sync(_)) => if !sent_variables {
                                replies.extend_from_slice(cache_variables.get_or_insert_with(||
                                    variable_store.variable_values()
                                    .map(|(name, value)| ServerCommand::var(name, value.clone()))
                                    .collect()
                                ));
                                sent_variables = true;
                            },
                            Err(error_reply) => replies.push(ServerCommand::ErrorReply(error_reply)),
                        }
                    }
                }
                Err(err) => {
                    if err.kind() != ErrorKind::WouldBlock {
                        eprintln!("{}", err);
                        client.shutdown().ok();
                    }
                }
            };
            if !sent_variables {
                replies.extend_from_slice(&changes);
            }
            if client.connected() && !replies.is_empty() {
                client.send(&replies).unwrap_or_else(|err| eprintln!("{}", err));
            }
        }

        let mut cache_info = None;
        while let Some(mut new_client) = match client_receiver.try_recv() {
            Ok(client) => Some(client),
            Err(TryRecvError::Empty) => None,
            Err(dc) => Err(dc)?,
        } {
            if cache_info.is_none() {
                cache_info = lua_interface.get_info().map(ServerCommand::Info);
            }
            if let Some(info) = &cache_info {
                new_client.send(&[info.clone()]).or_else(|_| new_client.shutdown()).ok();
            } else {
                break;
            }

            clients.push(new_client);
        }

        clients.retain(Client::connected);

        thread::sleep(GAME_WATCH_INTERVAL);
    }
}
