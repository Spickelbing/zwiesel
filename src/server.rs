use bytes::Bytes;
use futures::{future::select_all, SinkExt, StreamExt};
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::mpsc;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub type FramedStream = Framed<TcpStream, LengthDelimitedCodec>;

pub fn bind_stream(stream: TcpStream) -> FramedStream {
    Framed::new(stream, LengthDelimitedCodec::new())
}

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
    #[error("client {0} disconnected")]
    ClientDisconnected(ClientId),
}

type Result<T, E = ServerError> = std::result::Result<T, E>;

/// A server that works with framed streams.
pub struct Server {
    pub local_addr: SocketAddr,
    new_connections_rx: mpsc::Receiver<Result<Client>>,
    clients: HashMap<ClientId, Client>,
}

struct Client {
    addr: SocketAddr,
    stream: FramedStream,
}

impl Client {
    fn new(addr: SocketAddr, stream: FramedStream) -> Self {
        Self { addr, stream }
    }
}

pub type ClientId = u128;

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

    /// In case of errors, this method disconnects the respective clients.
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

    /// In case of an error, this method disconnects the respective client.
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

    /// In case of an error, this method disconnects the respective client.
    pub async fn recv_frame(&mut self) -> Result<(ClientId, Bytes)> {
        let mut clients: Vec<(&ClientId, &mut Client)> = self.clients.iter_mut().collect(); // this is pretty dumb
        let frame_futures = clients.iter_mut().map(|(_, v)| v.stream.next());

        select! {
            next_frame = select_all(frame_futures) => {
                match next_frame {
                    (Some(msg), idx, ..) => {
                        let maybe_msg = msg.map_err(ServerError::ReadFrame);
                        match maybe_msg {
                            Ok(msg) => {
                                return Ok((*clients[idx].0, msg.into()));
                            }
                            Err(err) => {
                                let id = *clients[idx].0;
                                self.clients.remove(&id);
                                return Err(err);
                            }
                        }
                    }
                    (None, idx, ..) => {
                        let id = *clients[idx].0;
                        self.clients.remove(&id);
                        return Err(ServerError::ClientDisconnect(id));
                    }
                }
            }
        }
    }

    // TODO: make it possible to recv and accept at the same time
    pub async fn accept(&mut self) -> Result<ClientId> {
        match self.new_connections_rx.recv().await {
            Some(maybe_client) => {
                let client = maybe_client?;

                let mut rng = thread_rng();
                let mut id: ClientId = rng.gen();
                while self.clients.contains_key(&id) {
                    id = rng.gen();
                }
                self.clients.insert(id, client);

                Ok(id)
            }
            None => {
                unreachable!()
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

async fn try_accept_connection(listener: &mut TcpListener) -> Result<Client> {
    let (stream, addr) = listener.accept().await.map_err(ServerError::Accept)?;
    let stream = bind_stream(stream);
    Ok(Client::new(addr, stream))
}

async fn listen(mut listener: TcpListener, tx: mpsc::Sender<Result<Client>>) {
    loop {
        let maybe_client = try_accept_connection(&mut listener).await;
        if tx.send(maybe_client).await.is_err() {
            break; // channel closed by the server, stop listening
        }
    }
}
