mod client;
mod common;
mod server;

use bincode::{deserialize, serialize};
use bytes::Bytes;
use client::Client;
use server::Server;
use std::net::{Ipv4Addr, SocketAddrV4};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 4711);
    let server_msg = "Hello, client!";
    let client_msg = "Hello, server!";
    let server_frame: Bytes = serialize(&server_msg)?.into();
    let client_frame: Bytes = serialize(&client_msg)?.into();

    let mut server = Server::bind(socket.into()).await?;
    let mut client = Client::connect(socket.into()).await?;

    {
        let client = server.accept().await?;
        println!("[server] sending '{server_msg}' to '{client}'");
        server.send_frame(client, server_frame.clone()).await?;
    }

    {
        let recv_frame = client.recv_frame().await?;
        let recv_msg: String = deserialize(&recv_frame)?;
        println!("[client] received '{recv_msg}'");
        println!("[client] sending '{client_msg}'");
        client.send_frame(client_frame).await?;
    }

    {
        let (client, recv_frame) = server.recv_frame().await?;
        let recv_msg: String = deserialize(&recv_frame)?;
        println!("[server] received '{recv_msg}' from '{client}'");
    }

    Ok(())
}
