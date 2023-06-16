use crate::common::{bind_stream, FramedStream};
use crate::message::{Message, MessageError};
use bytes::{Bytes, BytesMut};
use futures::{future::select_all, SinkExt, StreamExt};
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::fmt::Display;
use std::io;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::task::Poll;
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio::select;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("failed to read frame: {0}")]
    ReadFrame(io::Error),
    #[error("failed to send frame: {0}")]
    SendFrame(io::Error),
    #[error("failed to accept new connection: {0}")]
    Accept(io::Error),
    #[error("failed to bind to socket {0}: {1}")]
    Bind(SocketAddr, io::Error),
    #[error("failed to perform action for client {0}: client does not exist")]
    ClientDoesNotExist(ClientId),
    #[error("{0}")]
    Message(#[from] MessageError),
}

type Result<T, E = ServerError> = std::result::Result<T, E>;

/// A server for network protocol `T`.
/// To shut the server down, simply drop it.
pub struct Server<T> {
    pub local_addr: SocketAddr,
    listener: TcpListener,
    clients: HashMap<ClientId, Client>,
    phantom: PhantomData<T>,
}

struct Client {
    addr: SocketAddr,
    stream: FramedStream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(u64);

impl ClientId {
    fn gen() -> Self {
        Self(thread_rng().gen())
    }
}

impl Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

impl Client {
    fn new(addr: SocketAddr, stream: TcpStream) -> Self {
        Self {
            addr,
            stream: bind_stream(stream),
        }
    }
}

#[derive(Debug)]
pub enum Event<T>
where
    T: Message,
{
    NewConnection(ClientId),
    Disconnect(ClientId, Option<ServerError>),
    Message(ClientId, T),
}

impl<T> Server<T>
where
    T: Message,
{
    /// Sets up the server and binds it to a socket.
    /// To process any kind of inbound network event, including connection attempts,
    /// you need to `.await` the `event()` method.
    pub async fn host(socket: SocketAddr) -> Result<Self> {
        let listener = TcpListener::bind(socket)
            .await
            .map_err(|e| ServerError::Bind(socket, e))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| ServerError::Bind(socket, e))?;

        Ok(Self {
            listener,
            local_addr,
            clients: HashMap::new(),
            phantom: PhantomData,
        })
    }

    async fn broadcast_frame(
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

    /// Errors indicate that the respective clients are no longer connected.
    // TODO: get rid of nested result somehow
    pub async fn broadcast(
        &mut self,
        message: &T,
    ) -> Result<Result<(), HashMap<ClientId, ServerError>>> {
        let frame = message.serialize()?;
        Ok(self.broadcast_frame(frame).await)
    }

    async fn send_frame(&mut self, client_id: ClientId, frame: Bytes) -> Result<()> {
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

    /// Errors indicate that the respective client is no longer connected.
    pub async fn send(&mut self, client_id: ClientId, message: &T) -> Result<()> {
        let frame = message.serialize()?;
        self.send_frame(client_id, frame).await
    }

    async fn recv_frame(
        clients: &mut HashMap<ClientId, Client>,
    ) -> (ClientId, Option<Result<BytesMut, io::Error>>) {
        if clients.is_empty() {
            // pend forever so that `select_all` doesn't panic
            ForeverPending.await.forever()
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

    /// Waits for any inbound network event to occur.
    /// This method needs to be `.await`ed in order for any inbound events
    /// such as connection attempts and incoming messages to be processed.
    pub async fn event(&mut self) -> Result<Event<T>> {
        select! {
            maybe_connection = self.listener.accept() => {
                let (stream, addr) = maybe_connection.map_err(ServerError::Accept)?;
                let mut id = ClientId::gen();
                while self.clients.contains_key(&id) {
                    id = ClientId::gen();
                }
                let client = Client::new(addr, stream);
                self.clients.insert(id, client);
                Ok(Event::NewConnection(id))
            }
            (client_id, maybe_frame) = Self::recv_frame(&mut self.clients) => {
                match maybe_frame {
                    Some(Ok(frame)) => {
                        let message = T::deserialize(frame.freeze())?;
                        Ok(Event::Message(client_id, message))
                    },
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

struct ForeverPending;

impl ForeverPending {
    /// Call this method only after `.await`ing `ForeverPending`, like so:
    /// ```no_run
    /// ForeverPending.await.forever()
    /// ```
    /// It will pend forever.
    /// # Panics
    /// Always panics.
    fn forever(&self) -> ! {
        panic!("ForeverPending::forever() was called, which should never happen")
    }
}

impl std::future::Future for ForeverPending {
    type Output = ForeverPending;

    fn poll(self: std::pin::Pin<&mut Self>, _: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
