use crate::common::{bind_stream, FramedStream};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use std::io;
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
}

/// A client that works with framed streams.
pub struct Client {
    stream: FramedStream,
    pub remote_addr: SocketAddr,
}

impl Client {
    pub async fn connect(socket: SocketAddr) -> Result<Self, ClientError> {
        let stream = TcpStream::connect(socket)
            .await
            .map_err(|e| ClientError::Connect(socket, e))?;
        let framed = bind_stream(stream);
        Ok(Self {
            stream: framed,
            remote_addr: socket,
        })
    }

    pub async fn recv_frame(&mut self) -> Result<Bytes, ClientError> {
        let frame = self
            .stream
            .next()
            .await
            .ok_or(ClientError::ServerDisconnect)?
            .map_err(ClientError::ReadFrame)?;
        Ok(frame.into())
    }

    pub async fn send_frame(&mut self, frame: Bytes) -> Result<(), ClientError> {
        self.stream
            .send(frame)
            .await
            .map_err(ClientError::SendFrame)?;
        Ok(())
    }
}