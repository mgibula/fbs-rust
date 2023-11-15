use std::cmp::min;
use std::collections::HashMap;

use fbs_library::socket::{Socket, SocketDomain, SocketType, SocketFlags};
use fbs_library::system_error::SystemError;
use fbs_runtime::{async_connect, async_write, async_read_into};
use fbs_runtime::resolver::{resolve_address, ResolveAddressError};

use super::frame::{AmqpProtocolHeader, AmqpFrame, AmqpFrameError, AmqpFramePayload, AmqpMethod};
use super::frame_reader::AmqpFrameReader;
use super::frame_writer::FrameWriter;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AmqpConnectionError {
    #[error("AMQP address incorrect")]
    AddressIncorrect(#[from] ResolveAddressError),
    #[error("Connect error")]
    ConnectError(SystemError),
    #[error("Write error")]
    WriteError(SystemError),
    #[error("Read error")]
    ReadError(SystemError),
    #[error("Connection closed")]
    ConnectionClosed,
    #[error("Invalid type frame")]
    FrameTypeUnknown(u8),
    #[error("Invalid frame end")]
    ProtocolFrameEndInvalid,
    #[error("Frame error")]
    FrameError(#[from] AmqpFrameError)
}

pub struct AmqpConnection {
    fd: Socket,
    address: String,
    read_buffer: Vec<u8>,
    read_offset: usize,
}

impl AmqpConnection {
    pub fn new(address: String) -> Self {
        AmqpConnection {
            fd: Socket::new(SocketDomain::Inet, SocketType::Stream, SocketFlags::new().close_on_exec(true).flags()),
            address,
            read_buffer: Vec::with_capacity(4096),
            read_offset: 0,
        }
    }

    pub async fn connect(&mut self) -> Result<(), AmqpConnectionError> {
        let address = resolve_address(self.address.as_str(), Some(5672)).await?;
        let connected = async_connect(&self.fd, address).await;
        match connected {
            Ok(_) => (),
            Err(error) => return Err(AmqpConnectionError::ConnectError(error)),
        };

        let written = async_write(&self.fd, AmqpProtocolHeader::new().into(), None).await;
        match written {
            Ok(_) => (),
            Err((error, _)) => return Err(AmqpConnectionError::WriteError(error)),
        }

        let frame = self.read_frame().await?;
        dbg!(frame);

        let mut sasl = String::new();
        sasl.push('\x00');
        sasl.push_str("guest");
        sasl.push('\x00');
        sasl.push_str("guest");

        let response = AmqpFrame {
            channel: 0,
            payload: AmqpFramePayload::Method(AmqpMethod::ConnectionStartOk(HashMap::new(), "PLAIN".to_string(), sasl, String::new())),
        };

        self.write_frame(&response).await?;

        Ok(())
    }

    async fn fill_buffer(&mut self) -> Result<usize, AmqpConnectionError> {
        if self.read_offset < self.read_buffer.len() {
            return Ok(self.read_buffer.len() - self.read_offset);
        }

        self.read_offset = 0;
        let result = async_read_into(&self.fd, std::mem::take(&mut self.read_buffer), None).await;
        match result {
            Err((error, _)) => return Err(AmqpConnectionError::ReadError(error)),
            Ok(buffer) => self.read_buffer = buffer,
        }

        if self.read_buffer.is_empty() {
            return Err(AmqpConnectionError::ConnectionClosed);
        }

        Ok(self.read_buffer.len())
    }

    async fn read_bytes(&mut self, mut target: &mut [u8]) -> Result<(), AmqpConnectionError> {
        let mut target_size = target.len();

        while target_size > 0 {
            let bytes_available = self.fill_buffer().await?;

            let to_copy = min(target_size, bytes_available);
            target.clone_from_slice(&self.read_buffer[self.read_offset .. self.read_offset + to_copy]);
            self.read_offset += to_copy;
            target_size -= to_copy;

            target = &mut target[to_copy..];
        }

        Ok(())
    }

    async fn read_u8(&mut self) -> Result<u8, AmqpConnectionError> {
        let mut bytes: [u8; 1] = [0; 1];
        self.read_bytes(&mut bytes).await?;

        Ok(u8::from_be_bytes(bytes))
    }

    async fn read_u16(&mut self) -> Result<u16, AmqpConnectionError> {
        let mut bytes: [u8; 2] = [0; 2];
        self.read_bytes(&mut bytes).await?;

        Ok(u16::from_be_bytes(bytes))
    }

    async fn read_u32(&mut self) -> Result<u32, AmqpConnectionError> {
        let mut bytes: [u8; 4] = [0; 4];
        self.read_bytes(&mut bytes).await?;

        Ok(u32::from_be_bytes(bytes))
    }

    async fn read_frame(&mut self) -> Result<AmqpFrame, AmqpConnectionError> {
        let frame_type = self.read_u8().await?;
        let channel = self.read_u16().await?;
        let payload_size = self.read_u32().await? as usize;

        let mut payload = Vec::with_capacity(payload_size);
        payload.resize(payload_size, b'\x00');

        self.read_bytes(&mut payload).await?;
        let frame_end = self.read_u8().await?;

        let mut reader = AmqpFrameReader::new(&payload);
        Ok(reader.read_frame(frame_type, channel)?)
    }

    async fn write_frame(&mut self, frame: &AmqpFrame) -> Result<(), AmqpConnectionError> {
        let data = FrameWriter::write_frame(frame);
        let result = async_write(&self.fd, data, None).await;

        match result {
            Ok(_) => (),
            Err((error, _)) => return Err(AmqpConnectionError::WriteError(error)),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fbs_runtime::async_run;

    #[test]
    fn bad_connect_test() {
        async_run(async {
            let mut amqp = AmqpConnection::new("asd".to_string());
            let result = amqp.connect().await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn good_connect_test() {
        async_run(async {
            let mut amqp = AmqpConnection::new("localhost".to_string());
            let _ = amqp.connect().await;
        });
    }
}
