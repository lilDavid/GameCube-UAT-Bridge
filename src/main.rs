mod connector;
mod gamecube;

use std::{error::Error, net::Ipv4Addr, sync::{mpsc::{channel, Sender}, Arc, Mutex}, thread, time::Duration};

use json::{array, object, JsonValue};
use websocket::{Message, OwnedMessage};

#[cfg(target_os = "windows")]
use crate::connector::dolphin::read_game_world;
#[cfg(not(target_os = "windows"))]
use crate::connector::nintendont::read_game_world;

struct SyncCommand {
    #[allow(dead_code)]
    slot: Option<String>,
}

enum ClientCommand {
    Sync(SyncCommand),
}

impl TryFrom<&JsonValue> for ClientCommand {
    type Error = ();

    fn try_from(value: &JsonValue) -> Result<Self, Self::Error> {
        if let JsonValue::Object(obj) = value {
            match obj["cmd"].as_str() {
                Some("Sync") => Ok(Self::Sync(SyncCommand { slot: obj["slot"].as_str().map(String::from) })),
                _ => Err(()),
            }
        } else {
            Err(())
        }
    }
}

struct InfoCommand {
    name: String,
    version: Option<String>,
    features: Option<Vec<String>>,
    slots: Option<Vec<String>>,
}

impl Into<JsonValue> for InfoCommand {
    fn into(self) -> JsonValue {
        let mut cmd = object!{
            cmd: "Info",
            name: self.name,
            version: self.version,
            protocol: UAT_PROTOCOL_VERSION,
        };
        if let Some(features) = self.features {
            cmd["features"] = JsonValue::from(features);
        }
        if let Some(slots) = self.slots {
            cmd["slots"] = JsonValue::from(slots);
        }
        cmd
    }
}

struct VarCommand {
    name: String,
    value: u32,  // TODO: More types
    slot: Option<i32>,
}

impl Into<JsonValue> for VarCommand {
    fn into(self) -> JsonValue {
        let mut cmd = object!{
            cmd: "Var",
            name: self.name,
            value: self.value,
        };
        if let Some(slot) = self.slot {
            cmd["slot"] = JsonValue::from(slot);
        }
        cmd
    }
}

struct ErrorReplyCommand {
}

impl Into<JsonValue> for ErrorReplyCommand {
    fn into(self) -> JsonValue {
        todo!()
    }
}

struct VariableWatch {
    name: String,
    value: u32,  // TODO: More types
}

const UAT_PORT_MAIN: u16 = 65399;
const UAT_PROTOCOL_VERSION: i32 = 0;

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
            let s = json::stringify(vec![InfoCommand {
                name: "Metroid Prime".into(),
                version: Some("0-00".into()),
                features: None,
                slots: None,
            }]);
            println!("{}", s);
            client.lock().unwrap().send_message(&Message::text(s)).unwrap();

            thread::spawn(move || {
                let client = client2;
                loop {
                    let message = receiver.recv().unwrap().into_iter().map(|watch| VarCommand {
                        name: watch.name,
                        value: watch.value,
                        slot: None,
                    }).collect::<Vec::<_>>();
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
                        Ok(ClientCommand::Sync(_)) => vec![ VarCommand { name: "world".into(), value: world, slot: None } ],
                        _ => todo!(),
                    };
                    client.lock().unwrap().send_message(&Message::text(json::stringify(response))).expect("Could not send message");
                }
            }
        });
    }

    Ok(())
}
