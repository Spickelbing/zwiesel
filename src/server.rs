use crate::common::{bind_stream, FramedStream};
use bytes::{Bytes, BytesMut};
use futures::{future::select_all, SinkExt, StreamExt};
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::select;
use tokio::sync::mpsc;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("error receiving frame: {0}")]
    ReadFrame(io::Error),
    #[error("error sending frame: {0}")]
    SendFrame(io::Error),
    #[error("failed to accept new connection: {0}")]
    Accept(io::Error),
    #[error("failed to bind to socket {0}: {1}")]
    Bind(SocketAddr, io::Error),
    #[error("client does not exist: {0}")]
    ClientDoesNotExist(ClientId),
}

type Result<T, E = ServerError> = std::result::Result<T, E>;

/// A server that works with framed streams.
/// To shut the server down, drop it.
pub struct Server {
    pub local_addr: SocketAddr,
    new_connections_rx: mpsc::Receiver<Result<Connection>>,
    clients: HashMap<ClientId, Client>,
}

struct Client {
    addr: SocketAddr,
    stream: FramedStream,
}

pub type ClientId = u128;

impl Client {
    fn new(addr: SocketAddr, stream: FramedStream) -> Self {
        Self { addr, stream }
    }
}

#[derive(Debug)]
pub enum Event {
    NewConnection(ClientId),
    Disconnect(ClientId, Option<ServerError>),
    Frame(ClientId, Bytes),
}

impl Server {
    pub async fn bind(socket: SocketAddr) -> Result<Self> {
        let listener = TcpListener::bind(socket)
            .await
            .map_err(|e| ServerError::Bind(socket, e))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| ServerError::Bind(socket, e))?;
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(listen(listener, tx));

        Ok(Self {
            local_addr,
            new_connections_rx: rx,
            clients: HashMap::new(),
        })
    }

    /// Errors indicate that the respective clients are no longer connected.
    pub async fn broadcast_frame(
        &mut self,
        frame: Bytes,
    ) -> Result<(), HashMap<ClientId, ServerError>> {
        let mut errs = HashMap::new();

        for (&id, client) in self.clients.iter_mut() {
            let maybe_err = client
                .stream
                .send(frame.clone())
                .await
                .map_err(ServerError::SendFrame);
            if let Err(err) = maybe_err {
                errs.insert(id, err);
            }
        }

        if errs.is_empty() {
            Ok(())
        } else {
            for id in errs.keys() {
                self.clients.remove(id);
            }
            Err(errs)
        }
    }

    /// Errors indicate that the respective client is no longer connected.
    pub async fn send_frame(&mut self, client_id: ClientId, frame: Bytes) -> Result<()> {
        let client = self
            .clients
            .get_mut(&client_id)
            .ok_or(ServerError::ClientDoesNotExist(client_id))?;
        let maybe_err = client
            .stream
            .send(frame.clone())
            .await
            .map_err(ServerError::SendFrame);
        match maybe_err {
            Ok(()) => Ok(()),
            Err(err) => {
                self.clients.remove(&client_id);
                Err(err)
            }
        }
    }

    async fn recv_frame(
        clients: &mut HashMap<ClientId, Client>,
    ) -> (ClientId, Option<Result<BytesMut, io::Error>>) {
        if clients.is_empty() {
            // pend forever so that `select_all` doesn't panic
            let (_tx, mut rx) = mpsc::channel::<bool>(1);
            rx.recv().await;
        }

        let mut clients: Vec<(&ClientId, &mut Client)> = clients.iter_mut().collect(); // this is pretty dumb
        let frame_futures = clients.iter_mut().map(|(_, v)| v.stream.next());

        select! {
            maybe_frame = select_all(frame_futures) => {
                let (maybe_frame, idx, _) = maybe_frame;
                let id = *clients[idx].0; // this is why I needed the vec above
                (id, maybe_frame)
            }
        }
    }

    async fn accept(rx: &mut mpsc::Receiver<Result<Connection>>) -> Result<Connection> {
        match rx.recv().await {
            Some(maybe_connection) => maybe_connection,
            None => unreachable!(),
        }
    }

    pub async fn event(&mut self) -> Result<Event> {
        select! {
            maybe_connection = Self::accept(&mut self.new_connections_rx) => {
                let connection = maybe_connection?;
                let mut rng = thread_rng();
                let mut id: ClientId = rng.gen();
                while self.clients.contains_key(&id) { // this really is a bit dumb, too
                    id = rng.gen();
                }
                let client = Client::new(connection.addr, connection.stream);
                self.clients.insert(id, client);
                Ok(Event::NewConnection(id))
            }
            (client_id, maybe_frame) = Self::recv_frame(&mut self.clients) => {
                match maybe_frame {
                    Some(Ok(frame)) => Ok(Event::Frame(client_id, frame.freeze())),
                    Some(Err(err)) => {
                        self.clients.remove(&client_id);
                        Ok(Event::Disconnect(client_id, Some(ServerError::ReadFrame(err))))
                    }
                    None => {
                        self.clients.remove(&client_id);
                        Ok(Event::Disconnect(client_id, None))
                    }
                }
            }
        }
    }

    pub fn disconnect(&mut self, client_id: ClientId) -> Result<()> {
        if !self.clients.contains_key(&client_id) {
            return Err(ServerError::ClientDoesNotExist(client_id));
        }
        self.clients.remove(&client_id);
        Ok(())
    }

    pub fn disconnect_all(&mut self) {
        self.clients.clear();
    }

    pub fn clients(&self) -> Vec<ClientId> {
        self.clients.keys().copied().collect()
    }
}

struct Connection {
    addr: SocketAddr,
    stream: FramedStream,
}

impl Connection {
    fn new(addr: SocketAddr, stream: FramedStream) -> Self {
        Self { addr, stream }
    }
}

async fn try_accept_connection(listener: &mut TcpListener) -> Result<Connection> {
    let (stream, addr) = listener.accept().await.map_err(ServerError::Accept)?;
    let stream = bind_stream(stream);
    Ok(Connection::new(addr, stream))
}

async fn listen(mut listener: TcpListener, tx: mpsc::Sender<Result<Connection>>) {
    loop {
        let maybe_client = try_accept_connection(&mut listener).await;
        if tx.send(maybe_client).await.is_err() {
            break; // channel closed by the server, stop listening
        }
    }
}
