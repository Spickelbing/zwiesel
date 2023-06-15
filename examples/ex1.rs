use bincode::{deserialize, serialize};
use bytes::Bytes;
use server::{Client, Event, Server};
use std::net::{Ipv4Addr, SocketAddrV4};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 4711);
    let server_msg = "Hello, client!";
    let client_msg = "Hello, server!";
    let server_frame: Bytes = serialize(&server_msg)?.into();
    let client_frame: Bytes = serialize(&client_msg)?.into();

    let mut server = Server::host(socket.into()).await?;
    let mut client = Client::connect(socket.into()).await?;

    match server.event().await? {
        Event::NewConnection(client_id) => {
            println!("[server] new connection: {}", client_id);
            println!("[server] sending '{server_msg}' to '{client_id}'");
            server.send_frame(client_id, server_frame.clone()).await?;
        }
        event => {
            println!("[server] unexpected event: {:?}", event);
        }
    }

    {
        let recv_frame = client.recv_frame().await?;
        let recv_msg: String = deserialize(&recv_frame)?;
        println!("[client] received '{recv_msg}'");
        println!("[client] sending '{client_msg}'");
        client.send_frame(client_frame).await?;
    }

    match server.event().await? {
        Event::Frame(client_id, frame) => {
            let recv_msg: String = deserialize(&frame)?;
            println!("[server] received '{recv_msg}' from '{client_id}'");
        }
        event => {
            println!("[server] unexpected event: {:?}", event);
        }
    }

    Ok(())
}
