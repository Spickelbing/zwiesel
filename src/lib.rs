mod client;
mod common;
mod protocol;
mod server;

pub use client::{Client, ClientError};
pub use protocol::{Message, MessageError};
pub use server::{ClientId, Event, Server, ServerError};

#[cfg(test)]
mod tests {
    /* use super::*;

    #[test]
    fn it_works() {} */
}
