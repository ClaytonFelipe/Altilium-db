use tokio::net::TcpStream;
use tokio::io::{BufReader, AsyncReadExt};
use bytes::{BytesMut, Buf}; // Adicione a crate `bytes` ao seu Cargo.toml

pub struct Connection {
    stream: BufReader<TcpStream>,
    buffer: BytesMut,
}

impl Connection {
    pub fn new(socket: TcpStream) -> Self {
        Self {
            stream: BufReader::new(socket),
            buffer: BytesMut::with_capacity(4096),
        }
    }

    pub async fn read_frame(&mut self) -> Result<Option<RespValue>, Box<dyn std::error::Error>> {
        loop {
            // Tenta fazer o parse de um frame do buffer atual
            if let Ok((remaining, frame)) = parse_resp(&self.buffer) {
                let len = self.buffer.len() - remaining.len();
                self.buffer.advance(len); // Consome o frame do buffer
                return Ok(Some(frame));
            }

            // Se não há dados suficientes, leia mais do socket
            if self.stream.read_buf(&mut self.buffer).await? == 0 {
                // O cliente fechou a conexão
                return if self.buffer.is_empty() {
                    Ok(None)
                } else {
                    Err("connection reset by peer".into())
                };
            }
        }
    }
}