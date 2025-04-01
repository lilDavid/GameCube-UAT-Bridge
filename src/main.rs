mod connector;
mod gamecube;
mod uat;

use std::{env, error::Error, net::{IpAddr, Ipv4Addr}, str::FromStr, sync::{mpsc::{channel, Sender}, Arc, Mutex}, thread, time::Duration};

use connector::GameCubeConnector;
use gamecube::{GCN_GAME_ID_ADDRESS, PRIME_GAME_STATE_ADDRESS, PRIME_WORLD_OFFSET};
use uat::command::{ClientCommand, InfoCommand, VarCommand};
use websocket::{Message, OwnedMessage};

#[cfg(target_os = "windows")]
use crate::connector::dolphin::DolphinConnector;
use crate::connector::nintendont::NintendontConnector;

use crate::uat::UAT_PORT_MAIN;

struct VariableWatch {
    name: String,
    value: u32,  // TODO: More types
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
    let world = {
        let mut argv = env::args();
        argv.next();  // Consume argv[0]

        let target = argv.next().expect("Need IP Address or to specify Dolphin");

        let mut connector: Box<dyn GameCubeConnector> = {
            if target.to_lowercase() == "dolphin" {
                get_dolphin_connector()?
            } else {
                println!("Connecting to Nintendont at {}...", target);
                loop {
                    match NintendontConnector::new(IpAddr::from_str(&target)?) {
                        Ok(nintendont) => break Box::new(nintendont),
                        Err(err) => eprintln!("{}", err),
                    }
                }
            }
        };
        println!("Connected");

        let game_id = String::from_utf8(connector.read_address(6, GCN_GAME_ID_ADDRESS)?)?;
        println!(">> Game ID: {}", game_id);
        let game_revision = String::from_utf8(connector.read_address(1, GCN_GAME_ID_ADDRESS + 6)?)?;
        println!(">> Revision: {}", game_revision);

        let result = connector.read_pointers(4, PRIME_GAME_STATE_ADDRESS, &[PRIME_WORLD_OFFSET])?;
        let world = u32::from_be_bytes([result[0], result[1], result[2], result[3]]);
        println!(">> Game world: {}", world);

        world
    };

    let client_message_channels: Arc<Mutex<Vec<Sender<Vec<VariableWatch>>>>> = Arc::new(Mutex::new(Vec::new()));
    let channels = Arc::clone(&client_message_channels);
    thread::spawn(move || {
        let client_message_channels = channels;

        loop {
            for channel in client_message_channels.lock().unwrap().iter() {
                channel.send(vec![VariableWatch { name: "world".to_owned(), value: world }]).unwrap();
            }

            thread::sleep(Duration::from_secs(1));
        }
    });

    let uat_server = websocket::server::sync::Server::bind((Ipv4Addr::LOCALHOST, UAT_PORT_MAIN))?;
    for connection in uat_server.filter_map(Result::ok) {
        let client = Arc::new(Mutex::new(connection.accept().unwrap()));
        let client2 = Arc::clone(&client);
        let (sender, receiver) = channel();
        client_message_channels.lock().unwrap().push(sender);

        thread::spawn(move || {
            let s = json::stringify(vec![InfoCommand::new("Metroid Prime", Some("0-00"))]);
            println!("{}", s);
            client.lock().unwrap().send_message(&Message::text(s)).unwrap();

            thread::spawn(move || {
                let client = client2;
                loop {
                    let message = receiver.recv().unwrap().into_iter().map(|watch| VarCommand::new(&watch.name, watch.value)).collect::<Vec::<_>>();
                    client.lock().unwrap().send_message(&Message::text(json::stringify(message))).expect("Could not send messsage");
                }
            });

            loop {
                let message = client.lock().unwrap().recv_message().unwrap();

                let data = match message {
                    OwnedMessage::Text(text) => text,
                    OwnedMessage::Binary(_) => todo!(),
                    OwnedMessage::Ping(data) => { client.lock().unwrap().send_message(&Message::pong(data)).unwrap(); continue },
                    OwnedMessage::Pong(_) => continue,
                    OwnedMessage::Close(_) => break,
                };
                let frame = json::parse(&data).expect("Received invalid JSON");

                for command in frame.members() {
                    let response = match ClientCommand::try_from(command) {
                        Ok(ClientCommand::Sync(_)) => vec![ VarCommand::new("world", world)],
                        _ => todo!(),
                    };
                    client.lock().unwrap().send_message(&Message::text(json::stringify(response))).expect("Could not send message");
                }
            }
        });
    }

    Ok(())
}
