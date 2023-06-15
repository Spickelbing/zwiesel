mod client;
mod common;
mod server;

pub use client::{Client, ClientError};
pub use server::{ClientId, Event, Server, ServerError};

#[cfg(test)]
mod tests {
    /* use super::*;

    #[test]
    fn it_works() {} */
}
