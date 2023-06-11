use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub type FramedStream = Framed<TcpStream, LengthDelimitedCodec>;

pub fn bind_stream(stream: TcpStream) -> FramedStream {
    Framed::new(stream, LengthDelimitedCodec::new())
}
