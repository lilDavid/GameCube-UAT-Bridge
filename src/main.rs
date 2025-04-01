mod connector;
mod gamecube;
mod uat;

use std::{error::Error, net::Ipv4Addr, sync::{mpsc::{channel, Sender}, Arc, Mutex}, thread, time::Duration};

use uat::command::{ClientCommand, InfoCommand, VarCommand};
use websocket::{Message, OwnedMessage};

#[cfg(target_os = "windows")]
use crate::connector::dolphin::read_game_world;
#[cfg(not(target_os = "windows"))]
use crate::connector::nintendont::read_game_world;

use crate::uat::UAT_PORT_MAIN;

struct VariableWatch {
    name: String,
    value: u32,  // TODO: More types
}

fn main() -> Result<(), Box<dyn Error>> {
    let world = read_game_world()?;

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
