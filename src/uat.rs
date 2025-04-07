use std::{io::{self, ErrorKind}, net::{IpAddr, SocketAddr, TcpListener, TcpStream}};

use command::{ClientCommand, ErrorReplyCommand, ErrorReplyReason, ServerCommand};
use websocket::{server::{NoTlsAcceptor, WsServer}, sync::Client as WsClient, Message, OwnedMessage, WebSocketError, WebSocketResult};

pub mod command;
pub mod variable;

pub const UAT_PORT_MAIN: u16 = 65399;
pub const UAT_PORT_BACKUP: u16 = 44444;
pub const UAT_PROTOCOL_VERSION: i32 = 0;

pub struct Server(WsServer<NoTlsAcceptor, TcpListener>);

pub struct Client{
    client: WsClient<TcpStream>,
    shut_down: bool,
}

impl Server {
    pub fn new(addr: impl Into<IpAddr>) -> Result<Self, io::Error> {
        let addr = addr.into();
        let addresses = [
            SocketAddr::new(addr, UAT_PORT_MAIN),
            SocketAddr::new(addr, UAT_PORT_BACKUP),
        ];
        let server = websocket::server::sync::Server::bind(addresses.as_slice())?;
        Ok(Self(server))
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }

    pub fn accept_clients(self) -> impl Iterator<Item = io::Result<Client>> {
        self.0.into_iter().filter_map(Result::ok).map(|connection| {
            let client = connection.accept().map_err(|(_, err)| err)?;
            Client::new(client)
        })
    }
}

impl Client {
    fn new(client: WsClient<TcpStream>) -> io::Result<Self> {
        client.set_nonblocking(true)?;
        Ok(Self { client, shut_down: false })
    }

    fn convert_websocket_error(err: WebSocketError) -> io::Error {
        match err {
            WebSocketError::ProtocolError(msg) => io::Error::new(ErrorKind::InvalidData, msg),
            WebSocketError::DataFrameError(msg) => io::Error::new(ErrorKind::InvalidData, msg),
            WebSocketError::NoDataAvailable => io::Error::new(ErrorKind::WouldBlock, "no data to receive"),
            WebSocketError::IoError(err) => err,
            WebSocketError::Utf8Error(err) => io::Error::new(ErrorKind::InvalidData, err),
            WebSocketError::Other(err) => io::Error::new(ErrorKind::Other, err),
        }
    }

    pub fn receive(&mut self) -> io::Result<Vec<Result<ClientCommand, ErrorReplyCommand>>> {
        let data = match self.client.recv_message() {
            Ok(OwnedMessage::Text(text)) => text,
            Ok(OwnedMessage::Ping(data)) => {
                self.client.send_message(&Message::pong(data)).map_err(Self::convert_websocket_error)?;
                return Ok(vec![]);
            }
            Ok(OwnedMessage::Pong(_)) => return Ok(vec![]),
            Ok(OwnedMessage::Binary(_)) => Err(io::Error::new(ErrorKind::InvalidData, "expected text data, got binary"))?,
            Ok(OwnedMessage::Close(_)) => Err(io::Error::new(ErrorKind::ConnectionAborted, "client closed connection"))?,
            Err(err) => Err(Self::convert_websocket_error(err))?,
        };
        let json = match json::parse(&data) {
            Ok(data) => data,
            Err(err) => Err(io::Error::new(ErrorKind::InvalidData, err))?,
        };
        if !json.is_array() {
            return Ok(vec![Err(ErrorReplyCommand::with_description("", ErrorReplyReason::BadValue, Some("expected array")))]);
        }
        Ok(json.members().map(ClientCommand::try_from).collect())
    }

    pub fn send(&mut self, message: &[ServerCommand]) -> WebSocketResult<()> {
        self.client.send_message(&Message::text(json::stringify(message)))
    }

    pub fn shutdown(&mut self) -> io::Result<()> {
        self.shut_down = true;
        self.client.shutdown()
    }

    pub fn connected(&self) -> bool {
        return !self.shut_down;
    }

}
