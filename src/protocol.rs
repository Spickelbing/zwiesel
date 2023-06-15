use bytes::Bytes;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessageError {
    #[error("failed to serialize message")]
    Serialize,
    #[error("failed to deserialize bytes into message")]
    Deserialize,
}

pub trait Message {
    fn serialize(&self) -> Result<Bytes, MessageError>;

    fn deserialize(bytes: Bytes) -> Result<Self, MessageError>
    where
        Self: Sized;
}
