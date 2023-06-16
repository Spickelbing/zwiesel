use bincode::{deserialize, serialize};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddrV4};
use zwiesel::{Client, ClientEvent, Message, MessageError, Server, ServerEvent};

#[derive(Debug, Serialize, Deserialize)]
enum MySuperNiceProtocol {
    StringMessage(String),
}

impl Message for MySuperNiceProtocol {
    fn serialize(&self) -> Result<Bytes, MessageError> {
        Ok(serialize(self).map_err(|_| MessageError::Serialize)?.into())
    }

    fn deserialize(message: Bytes) -> Result<Self, MessageError> {
        deserialize(&message).map_err(|_| MessageError::Deserialize)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 4711);
    let server_msg = MySuperNiceProtocol::StringMessage("Hello, client!".to_string());
    let client_msg = MySuperNiceProtocol::StringMessage("Hello, server!".to_string());

    let mut server: Server<MySuperNiceProtocol> = Server::host(socket.into()).await?;
    let mut client: Client<MySuperNiceProtocol> = Client::connect(socket.into()).await?;

    println!("[server] waiting for connection...");
    match server.event().await? {
        ServerEvent::NewConnection(client_id) => {
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
        match client.event().await? {
            ClientEvent::Message(msg) => {
                println!("[client] received '{msg:?}'");
                println!("[client] sending '{client_msg:?}'");
                client.send(&client_msg).await?;
            }
            event => {
                println!("[client] unexpected event: {event:?}");
            }
        }
    }

    println!("[server] waiting for message...");
    match server.event().await? {
        ServerEvent::Message(client_id, msg) => {
            println!("[server] received '{msg:?}' from '{client_id}'");
        }
        event => {
            println!("[server] unexpected event: {:?}", event);
        }
    }

    Ok(())
}
