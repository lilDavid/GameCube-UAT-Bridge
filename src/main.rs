mod connection;
mod lua;
mod uat;

use std::{env, error::Error, io::ErrorKind, net::{IpAddr, Ipv4Addr}, str::FromStr, sync::{mpsc::{channel, Receiver, Sender, TryRecvError}, Arc, Mutex, RwLock}, thread::{self, JoinHandle}, time::Duration};

use connection::GameCubeConnection;
use lua::LuaInterface;
use uat::{command::{ClientCommand, InfoCommand, ServerCommand}, variable::VariableStore, Client, MessageReadError, MessageResponse, Server};
use websocket::{WebSocketError, WebSocketResult};

#[cfg(target_os = "windows")]
use crate::connection::dolphin::DolphinConnection;
use crate::connection::nintendont::NintendontConnection;

#[cfg(target_os = "windows")]
fn connect_to_dolphin() -> Box<dyn GameCubeConnection> {
    let result = loop {
        println!("Connecting to Dolphin...");
        match DolphinConnection::new() {
            Ok(dolphin) => break Box::new(dolphin),
            Err(err) => {eprintln!("{}", err); thread::sleep(Duration::from_secs(1))},
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
            Err(err) => {eprintln!("{}", err); thread::sleep(Duration::from_secs(1))},
        }
    };
    println!("Connected");
    result
}

fn run_uat_server(
    uat_server: Server,
    client_handles: Arc<Mutex<Vec<(Sender<Vec<ServerCommand>>, JoinHandle<WebSocketResult<()>>)>>>,
    variable_store: &Arc<Mutex<VariableStore>>,
    info_cache: &RwLock<Option<InfoCommand>>,
) -> WebSocketResult<()> {
    for mut client in uat_server.accept_clients().filter_map(Result::ok) {
        let variable_store = Arc::clone(variable_store);
        let (command_sender, command_receiver) = channel();
        if let Some(info) = info_cache.read().unwrap().as_ref() {
            client.send(&[ServerCommand::Info(info.clone())])?;
        }

        let handle = thread::spawn(move || {
            serve_uat_client(client, command_receiver, &variable_store)
        });
        client_handles.lock().unwrap().push((command_sender, handle));
    }

    Ok(())
}

fn serve_uat_client(mut client: Client, server_messages: Receiver<Vec<ServerCommand>>, variable_store: &Mutex<VariableStore>) -> WebSocketResult<()> {
    loop {
        match server_messages.try_recv() {
            Ok(server_commands) => client.send(&server_commands)?,
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => break,
        }

        let commands = match client.receive() {
            Ok(commands) => commands,
            Err(MessageReadError::SocketError(WebSocketError::IoError(err))) if err.kind() == ErrorKind::WouldBlock => continue,
            Err(err) => match client.handle_error(err) {
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
            client.send(&response)?
        }
    }
    Ok(())
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

    let info_cache: Arc<RwLock<Option<InfoCommand>>> = Arc::new(RwLock::new(None));
    let variable_store = VariableStore::new();
    let variable_store: Arc<Mutex<VariableStore>> = Arc::new(Mutex::new(variable_store));

    let uat_server = Server::new(Ipv4Addr::LOCALHOST)?;
    let client_handles = Arc::new(Mutex::new(Vec::new()));
    let info = Arc::clone(&info_cache);
    let variables = Arc::clone(&variable_store);
    let handles = Arc::clone(&client_handles);
    thread::spawn(move || {
        run_uat_server(uat_server, handles, &variables, &info)
    });

    loop {
        let connection = connection_factory();
        match lua_interface.connect(connection) {
            Ok((name, interface)) => {
                println!("Found interface {} for {}", name, interface.name()?.unwrap_or_else(|| "<nil>".into()));
                let mut info_cache = info_cache.write().unwrap();
                let info_command = InfoCommand::new(interface.name()?.as_deref(), interface.version()?.as_deref());
                info_cache.replace(info_command.clone());
                let mut handles = client_handles.lock().unwrap();
                handles.retain(|(_, thread)| !thread.is_finished());
                for (channel, _) in handles.iter() {
                    channel.send(vec![ServerCommand::Info(info_command.clone())]).ok();
                }
            },
            Err(_) => {
                println!("No interface found for this game");
                thread::sleep(Duration::from_secs(1));
            }
        }

        while let Some(result) = lua_interface.run_game_watcher() {
            match result {
                Ok(updates) => Some(updates),
                Err(e) => { eprintln!("{}", e); break }
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
                .map(|(name, value)| ServerCommand::var(&name, value))
                .collect::<Vec<_>>()
            ).map(|changes| {
                let mut handles = client_handles.lock().unwrap();
                handles.retain(|(_, thread)| !thread.is_finished());
                for (channel, _) in handles.iter() {
                    channel.send(changes.clone()).ok();
                }
            });

            thread::sleep(Duration::from_secs(1));
        }

        println!("Disconnected");
    }
}
