use crate::common::{bind_stream, FramedStream};
use crate::message::{Message, MessageError};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use std::io;
use std::marker::PhantomData;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::net::TcpStream;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("error receiving frame: {0}")]
    ReadFrame(io::Error),
    #[error("error sending frame: {0}")]
    SendFrame(io::Error),
    #[error("failed to connect to remote server {0}: {1}")]
    Connect(SocketAddr, io::Error),
    #[error("remote server disconnected")]
    ServerDisconnect,
    #[error("{0}")]
    Message(#[from] MessageError),
}

type Result<T, E = ClientError> = std::result::Result<T, E>;

/// A client for a server using network protocol `T`.
pub struct Client<T> {
    stream: FramedStream,
    pub remote_addr: SocketAddr,
    phantom: PhantomData<T>,
}

impl<T> Client<T>
where
    T: Message,
{
    pub async fn connect(socket: SocketAddr) -> Result<Self> {
        let stream = TcpStream::connect(socket)
            .await
            .map_err(|e| ClientError::Connect(socket, e))?;
        let remote_addr = stream
            .peer_addr()
            .map_err(|e| ClientError::Connect(socket, e))?;
        let framed = bind_stream(stream);
        Ok(Self {
            stream: framed,
            remote_addr,
            phantom: PhantomData,
        })
    }

    async fn recv_frame(&mut self) -> Option<Result<Bytes>> {
        let maybe_frame = self.stream.next().await;
        match maybe_frame {
            Some(Ok(frame)) => Some(Ok(frame.freeze())),
            Some(Err(err)) => Some(Err(ClientError::ReadFrame(err))),
            None => None,
        }
    }

    /// Waits for any inbound network event.
    pub async fn event(&mut self) -> Result<Event<T>> {
        let maybe_frame = self.recv_frame().await;
        match maybe_frame {
            Some(Ok(frame)) => {
                let msg = T::deserialize(frame)?;
                Ok(Event::Message(msg))
            }
            Some(Err(err)) => Ok(Event::Disconnect(Some(err))),
            None => Ok(Event::Disconnect(None)),
        }
    }

    async fn send_frame(&mut self, frame: Bytes) -> Result<()> {
        self.stream
            .send(frame)
            .await
            .map_err(ClientError::SendFrame)?;
        Ok(())
    }

    pub async fn send(&mut self, msg: &T) -> Result<()> {
        let frame = msg.serialize()?;
        self.send_frame(frame).await?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum Event<T>
where
    T: Message,
{
    Message(T),
    Disconnect(Option<ClientError>),
}
