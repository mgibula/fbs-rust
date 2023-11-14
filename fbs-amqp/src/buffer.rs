use std::collections::HashMap;
use thiserror::Error;

use crate::defines::AmqpData;

pub struct ReadBuffer<'buffer> {
    data: &'buffer [u8],
}

#[derive(Error, Debug)]
#[error("Buffer too short")]
pub struct BufferTooShort;

impl<'buffer> ReadBuffer<'buffer> {
    pub fn new(data: &'buffer[u8]) -> ReadBuffer<'buffer> {
        Self { data }
    }

    pub fn read_u8(&mut self) -> Result<u8, BufferTooShort> {
        if self.data.len() < 1 {
            return Err(BufferTooShort);
        }

        let mut buffer: [u8; 1] = [0; 1];
        buffer.copy_from_slice(&self.data[..1]);
        self.data = &self.data[1..];

        Ok(u8::from_be_bytes(buffer))
    }

    pub fn read_u16(&mut self) -> Result<u16, BufferTooShort> {
        if self.data.len() < 2 {
            return Err(BufferTooShort);
        }

        let mut buffer: [u8; 2] = [0; 2];
        buffer.copy_from_slice(&self.data[..2]);
        self.data = &self.data[2..];

        Ok(u16::from_be_bytes(buffer))
    }

    pub fn read_u32(&mut self) -> Result<u32, BufferTooShort> {
        if self.data.len() < 4 {
            return Err(BufferTooShort);
        }

        let mut buffer: [u8; 4] = [0; 4];
        buffer.copy_from_slice(&self.data[..4]);
        self.data = &self.data[4..];

        Ok(u32::from_be_bytes(buffer))
    }

    pub fn read_bytes(&mut self, target: &mut [u8]) -> Result<(), BufferTooShort> {
        if self.data.len() < target.len() {
            return Err(BufferTooShort);
        }

        let length = target.len();
        target.copy_from_slice(&self.data[..length]);
        self.data = &self.data[length..];

        Ok(())
    }

    pub fn read_short_string(&mut self) -> Result<String, BufferTooShort> {
        let length = self.read_u8()? as usize;
        let mut buffer = Vec::with_capacity(length);
        buffer.resize(length, b'\x00');

        self.read_bytes(&mut buffer)?;

        // TODO - remove unwrap
        Ok(String::from_utf8(buffer).unwrap())
    }

    pub fn read_table(&mut self) -> Result<HashMap<String, AmqpData>, BufferTooShort> {
        let bytes_to_read = self.read_u32()?;

        let key = self.read_short_string()?;
        let value_type = self.read_u8()?;

        unimplemented!()
    }
}
