use std::{error::Error, fmt::Display, io, net::{SocketAddr, IpAddr, TcpListener, TcpStream}};

use command::{ClientCommand, ServerCommand};
use json::JsonValue;
use websocket::{server::{InvalidConnection, NoTlsAcceptor, WsServer}, sync::{server::upgrade::Buffer, Client as WsClient}, Message, OwnedMessage, WebSocketError, WebSocketResult};

pub mod command;
pub mod variable;

pub const UAT_PORT_MAIN: u16 = 65399;
pub const UAT_PORT_BACKUP: u16 = 44444;
pub const UAT_PROTOCOL_VERSION: i32 = 0;

pub struct Server(WsServer<NoTlsAcceptor, TcpListener>);

pub struct Client(WsClient<TcpStream>);

#[derive(Debug)]
#[allow(dead_code)]
pub enum IncomingConnectionError {
    InvalidConnection(InvalidConnection<TcpStream, Buffer>),
    CouldNotAccept(io::Error),
}

impl Display for IncomingConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConnection(_) => "Invalid connection".fmt(f),
            Self::CouldNotAccept(err) => { "Could not accept: ".fmt(f)?; err.fmt(f) }
        }
    }
}

impl Error for IncomingConnectionError {}

impl Server {
    pub fn new(addr: impl Into<IpAddr>) -> Result<Self, io::Error> {
        let addr = addr.into();
        Ok(Self(websocket::server::sync::Server::bind([
            SocketAddr::new(addr, UAT_PORT_MAIN),
            SocketAddr::new(addr, UAT_PORT_BACKUP),
        ].as_slice())?))
    }

    pub fn accept_clients(self) -> impl Iterator<Item = Result<Client, IncomingConnectionError>> {
        self.0.into_iter().map(|connection| {
            let connection = connection.or_else(|e| Err(IncomingConnectionError::InvalidConnection(e)))?;
            let client = connection.accept().or_else(|e| Err(IncomingConnectionError::CouldNotAccept(e.1)))?;
            client.stream_ref().set_nonblocking(true).map_err(|e| IncomingConnectionError::CouldNotAccept(e))?;
            Ok(Client(client))
        })
    }
}

impl Client {
    fn create_message(message: &[ServerCommand]) -> OwnedMessage {
        Message::text(json::stringify(message)).into()
    }

    fn parse_message(message: OwnedMessage) -> Result<Vec<ClientCommand>, MessageReadError> {
        let data = match message {
            OwnedMessage::Text(text) => text,
            OwnedMessage::Binary(_) => Err(MessageReadError::InvalidMessage)?,
            msg => Err(MessageReadError::HandleMessagePacket(msg))?,
        };
        let frame = json::parse(&data).or_else(|_| Err(MessageReadError::InvalidMessage))?;
        if let JsonValue::Array(commands) = frame {
            Ok(commands.into_iter().map(ClientCommand::from).collect::<Vec<_>>())
        } else {
            Err(MessageReadError::InvalidMessage)
        }
    }

    pub fn receive(&mut self) -> Result<Vec<ClientCommand>, MessageReadError> {
        Self::parse_message(self.0.recv_message()?)
    }

    pub fn send(&mut self, message: &[ServerCommand]) -> WebSocketResult<()> {
        self.0.send_message(&Self::create_message(message))
    }

    pub fn handle_error(&mut self, error: MessageReadError) -> MessageResponse {
        match error {
            MessageReadError::SocketError(e) => MessageResponse::Stop(Some(e)),
            MessageReadError::InvalidMessage => { self.0.send_message(&Message::close()).unwrap(); MessageResponse::Stop(None) }
            MessageReadError::HandleMessagePacket(OwnedMessage::Ping(data)) => { self.0.send_message(&Message::pong(data)).unwrap(); MessageResponse::Continue }
            MessageReadError::HandleMessagePacket(OwnedMessage::Pong(_)) => MessageResponse::Continue,
            MessageReadError::HandleMessagePacket(OwnedMessage::Close(_)) => MessageResponse::Stop(None),
            MessageReadError::HandleMessagePacket(OwnedMessage::Text(_)) => unreachable!(),
            MessageReadError::HandleMessagePacket(OwnedMessage::Binary(_)) => unreachable!(),
        }
    }

}

#[derive(Debug)]
pub enum MessageReadError {
    InvalidMessage,
    SocketError(WebSocketError),
    HandleMessagePacket(OwnedMessage),
}

impl Display for MessageReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMessage => "Invalid message, need to close connection".fmt(f),
            Self::SocketError(err) => err.fmt(f),
            Self::HandleMessagePacket(_) => "Need to handle message packet".fmt(f),
        }
    }
}

impl Error for MessageReadError {}

impl From<WebSocketError> for MessageReadError {
    fn from(value: WebSocketError) -> Self {
        Self::SocketError(value)
    }
}

pub enum MessageResponse {
    Continue,
    Stop(Option<WebSocketError>),
}
