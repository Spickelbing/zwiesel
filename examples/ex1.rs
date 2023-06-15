use bincode::{deserialize, serialize};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use server::{Client, Event, Message, MessageError, Server};
use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Debug, Serialize, Deserialize)]
enum MySuperNiceProtocol {
    StringMessage(String),
}

impl Message for MySuperNiceProtocol {
    fn serialize(&self) -> Result<Bytes, MessageError> {
        Ok(serialize(self).map_err(|_| MessageError::Serialize)?.into())
    }

    fn deserialize(message: Bytes) -> Result<Self, MessageError> {
        Ok(deserialize(&message).map_err(|_| MessageError::Deserialize)?)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 4711);
    let server_msg = MySuperNiceProtocol::StringMessage("Hello, client!".to_string());
    let client_msg = MySuperNiceProtocol::StringMessage("Hello, server!".to_string());

    let mut server: Server<MySuperNiceProtocol> = Server::host(socket.into()).await?;
    let mut client: Client<MySuperNiceProtocol> = Client::connect(socket.into()).await?;

    println!("[server] waiting for any network event...");
    match server.event().await? {
        Event::NewConnection(client_id) => {
            println!("[server] new connection: {}", client_id);
            println!("[server] sending '{server_msg:?}' to '{client_id}'");
            server.send(client_id, &server_msg).await?;
        }
        event => {
            println!("[server] unexpected event: {:?}", event);
        }
    }

    {
        println!("[client] waiting for message...");
        let msg = client.recv().await?;
        println!("[client] received '{msg:?}'");
        println!("[client] sending '{client_msg:?}'");
        client.send(&client_msg).await?;
    }

    println!("[server] waiting for any network event...");
    match server.event().await? {
        Event::Message(client_id, msg) => {
            println!("[server] received '{msg:?}' from '{client_id}'");
        }
        event => {
            println!("[server] unexpected event: {:?}", event);
        }
    }

    Ok(())
}
