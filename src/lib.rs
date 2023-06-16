mod client;
mod common;
mod message;
mod server;

pub use client::{Client, ClientError, Event as ClientEvent};
pub use message::{Message, MessageError};
pub use server::{ClientId, Event as ServerEvent, Server, ServerError};

#[cfg(test)]
mod tests {
    /* use super::*;

    #[test]
    fn it_works() {} */
}
