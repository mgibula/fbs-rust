use std::collections::HashMap;

use super::frame::{AmqpFrameError, AmqpFrame, AmqpFramePayload, AmqpMethod, AmqpData, AmqpBasicProperties};
use super::defines::*;

pub struct AmqpFrameReader<'buffer> {
    data: &'buffer [u8],
}

impl<'buffer> AmqpFrameReader<'buffer> {
    pub fn new(data: &'buffer[u8]) -> AmqpFrameReader<'buffer> {
        Self { data }
    }

    pub fn read_frame(&mut self, frame_type: u8, channel: u16) -> Result<AmqpFrame, AmqpFrameError> {
        match frame_type {
            AMQP_FRAME_TYPE_METHOD => Ok(AmqpFrame { channel, payload: AmqpFramePayload::Method(self.read_method_frame()?) }),
            AMQP_FRAME_TYPE_HEADER => Ok(AmqpFrame { channel, payload: self.read_header_frame()? }),
            AMQP_FRAME_TYPE_CONTENT => Ok(AmqpFrame { channel, payload: self.read_content_frame()? }),
            _ => Err(AmqpFrameError::InvalidFrameType(frame_type)),
        }
    }

    fn read_content_frame(&mut self) -> Result<AmqpFramePayload, AmqpFrameError> {
        Ok(AmqpFramePayload::Content(self.read_remaining_bytes()))
    }

    fn read_header_frame(&mut self) -> Result<AmqpFramePayload, AmqpFrameError> {
        let class_id = self.read_u16()?;
        let _ = self.read_u16()?;
        let size = self.read_u64()?;
        let properties_mask = self.read_u16()?;
        let mut properties = AmqpBasicProperties::default();

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_CONTENT_TYPE_BIT)) != 0 {
            properties.content_type = Some(self.read_short_string()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_CONTENT_ENCODING_BIT)) != 0 {
            properties.content_encoding = Some(self.read_short_string()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_HEADERS_BIT)) != 0 {
            properties.headers = Some(self.read_table()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_DELIVERY_MNODE_BIT)) != 0 {
            properties.delivery_mode = Some(self.read_u8()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_PRIORITY_BIT)) != 0 {
            properties.priority = Some(self.read_u8()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_CORRELATION_ID_BIT)) != 0 {
            properties.correlation_id = Some(self.read_short_string()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_REPLY_TO_BIT)) != 0 {
            properties.reply_to = Some(self.read_short_string()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_EXPIRATION_BIT)) != 0 {
            properties.expiration = Some(self.read_short_string()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_MESSAGE_ID_BIT)) != 0 {
            properties.message_id = Some(self.read_short_string()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_TIMESTAMP_BIT)) != 0 {
            properties.timestamp = Some(self.read_u64()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_TYPE_BIT)) != 0 {
            properties.message_type = Some(self.read_short_string()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_USER_ID_BIT)) != 0 {
            properties.user_id = Some(self.read_short_string()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_APP_ID_BIT)) != 0 {
            properties.app_id = Some(self.read_short_string()?);
        }

        if (properties_mask & (1 << AMQP_BASIC_PROPERTY_CLUSTER_ID_BIT)) != 0 {
            properties.cluster_id = Some(self.read_short_string()?);
        }

        Ok(AmqpFramePayload::Header(class_id, size, properties))
    }

    fn read_method_frame(&mut self) -> Result<AmqpMethod, AmqpFrameError> {
        let class_id = self.read_u16()?;
        let method_id = self.read_u16()?;

        match (class_id, method_id) {
            (AMQP_CLASS_CONNECTION, AMQP_METHOD_CONNECTION_START) => {
                let major = self.read_u8()?;
                let minor = self.read_u8()?;

                let properties = self.read_table()?;
                let mechanisms = self.read_long_string()?;
                let locales = self.read_long_string()?;
                Ok(AmqpMethod::ConnectionStart(major, minor, properties, mechanisms, locales))
            },
            (AMQP_CLASS_CONNECTION, AMQP_METHOD_CONNECTION_TUNE) => {
                let channel_max = self.read_u16()?;
                let frame_max = self.read_u32()?;
                let heartbeat = self.read_u16()?;
                Ok(AmqpMethod::ConnectionTune(channel_max, frame_max, heartbeat))
            },
            (AMQP_CLASS_CONNECTION, AMQP_METHOD_CONNECTION_OPEN_OK) => {
                Ok(AmqpMethod::ConnectionOpenOk())
            },
            (AMQP_CLASS_CONNECTION, AMQP_METHOD_CONNECTION_CLOSE) => {
                let reply_code = self.read_u16()?;
                let reply_text = self.read_short_string()?;
                let class_id = self.read_u16()?;
                let method_id = self.read_u16()?;

                Ok(AmqpMethod::ConnectionClose(reply_code, reply_text, class_id, method_id))
            },
            (AMQP_CLASS_CONNECTION, AMQP_METHOD_CONNECTION_CLOSE_OK) => {
                Ok(AmqpMethod::ConnectionCloseOk())
            },
            (AMQP_CLASS_CHANNEL, AMQP_METHOD_CHANNEL_OPEN_OK) => {
                let _ = self.read_long_string()?;   // deprecated arg
                Ok(AmqpMethod::ChannelOpenOk())
            },
            (AMQP_CLASS_CHANNEL, AMQP_METHOD_CHANNEL_CLOSE) => {
                let reply_code = self.read_u16()?;
                let reply_text = self.read_short_string()?;
                let class_id = self.read_u16()?;
                let method_id = self.read_u16()?;

                Ok(AmqpMethod::ChannelClose(reply_code, reply_text, class_id, method_id))
            },
            (AMQP_CLASS_CHANNEL, AMQP_METHOD_CHANNEL_CLOSE_OK) => {
                Ok(AmqpMethod::ChannelCloseOk())
            },
            (AMQP_CLASS_CHANNEL, AMQP_METHOD_CHANNEL_FLOW) => {
                let active = self.read_u8()?;
                Ok(AmqpMethod::ChannelFlow(active > 0))
            },
            (AMQP_CLASS_CHANNEL, AMQP_METHOD_CHANNEL_FLOW_OK) => {
                let active = self.read_u8()?;
                Ok(AmqpMethod::ChannelFlowOk(active > 0))
            },
            (AMQP_CLASS_EXCHANGE, AMQP_METHOD_EXCHANGE_DECLARE_OK) => {
                Ok(AmqpMethod::ExchangeDeclareOk())
            },
            (AMQP_CLASS_EXCHANGE, AMQP_METHOD_EXCHANGE_DELETE_OK) => {
                Ok(AmqpMethod::ExchangeDeleteOk())
            },
            (AMQP_CLASS_QUEUE, AMQP_METHOD_QUEUE_DECLARE_OK) => {
                let name = self.read_short_string()?;
                let message_count = self.read_i32()?;
                let consumer_count = self.read_i32()?;
                Ok(AmqpMethod::QueueDeclareOk(name, message_count, consumer_count))
            },
            (AMQP_CLASS_QUEUE, AMQP_METHOD_QUEUE_BIND_OK) => {
                Ok(AmqpMethod::QueueBindOk())
            },
            (AMQP_CLASS_QUEUE, AMQP_METHOD_QUEUE_UNBIND_OK) => {
                Ok(AmqpMethod::QueueUnbindOk())
            },
            (AMQP_CLASS_QUEUE, AMQP_METHOD_QUEUE_PURGE_OK) => {
                let messages = self.read_i32()?;
                Ok(AmqpMethod::QueuePurgeOk(messages))
            },
            (AMQP_CLASS_QUEUE, AMQP_METHOD_QUEUE_DELETE_OK) => {
                let messages = self.read_i32()?;
                Ok(AmqpMethod::QueueDeleteOk(messages))
            },
            (AMQP_CLASS_BASIC, AMQP_METHOD_BASIC_QOS_OK) => {
                Ok(AmqpMethod::BasicQosOk())
            },
            (AMQP_CLASS_BASIC, AMQP_METHOD_BASIC_CONSUME_OK) => {
                let tag = self.read_short_string()?;
                Ok(AmqpMethod::BasicConsumeOk(tag))
            },
            (AMQP_CLASS_BASIC, AMQP_METHOD_BASIC_CANCEL_OK) => {
                let tag = self.read_short_string()?;
                Ok(AmqpMethod::BasicCancelOk(tag))
            },
            (AMQP_CLASS_BASIC, AMQP_METHOD_BASIC_RETURN) => {
                let code = self.read_i16()?;
                let reply_text = self.read_short_string()?;
                let exchange = self.read_short_string()?;
                let routing_key = self.read_short_string()?;
                Ok(AmqpMethod::BasicReturn(code, reply_text, exchange, routing_key))
            },
            (AMQP_CLASS_BASIC, AMQP_METHOD_BASIC_DELIVER) => {
                let consumer_tag = self.read_short_string()?;
                let delivery_tag = self.read_u64()?;
                let redelivered = self.read_u8()?;
                let exchange = self.read_short_string()?;
                let routing_key = self.read_short_string()?;
                Ok(AmqpMethod::BasicDeliver(consumer_tag, delivery_tag, redelivered != 0, exchange, routing_key))
            },
            (_, _) => Err(AmqpFrameError::InvalidClassMethod(class_id, method_id))
        }
    }

    fn read_u8(&mut self) -> Result<u8, AmqpFrameError> {
        if self.data.len() < 1 {
            return Err(AmqpFrameError::BufferTooShort);
        }

        let mut buffer: [u8; 1] = [0; 1];
        buffer.copy_from_slice(&self.data[..1]);
        self.data = &self.data[1..];

        Ok(u8::from_be_bytes(buffer))
    }

    fn read_i8(&mut self) -> Result<i8, AmqpFrameError> {
        if self.data.len() < 1 {
            return Err(AmqpFrameError::BufferTooShort);
        }

        let mut buffer: [u8; 1] = [0; 1];
        buffer.copy_from_slice(&self.data[..1]);
        self.data = &self.data[1..];

        Ok(i8::from_be_bytes(buffer))
    }

    fn read_u16(&mut self) -> Result<u16, AmqpFrameError> {
        if self.data.len() < 2 {
            return Err(AmqpFrameError::BufferTooShort);
        }

        let mut buffer: [u8; 2] = [0; 2];
        buffer.copy_from_slice(&self.data[..2]);
        self.data = &self.data[2..];

        Ok(u16::from_be_bytes(buffer))
    }

    fn read_i16(&mut self) -> Result<i16, AmqpFrameError> {
        if self.data.len() < 2 {
            return Err(AmqpFrameError::BufferTooShort);
        }

        let mut buffer: [u8; 2] = [0; 2];
        buffer.copy_from_slice(&self.data[..2]);
        self.data = &self.data[2..];

        Ok(i16::from_be_bytes(buffer))
    }

    fn read_u32(&mut self) -> Result<u32, AmqpFrameError> {
        if self.data.len() < 4 {
            return Err(AmqpFrameError::BufferTooShort);
        }

        let mut buffer: [u8; 4] = [0; 4];
        buffer.copy_from_slice(&self.data[..4]);
        self.data = &self.data[4..];

        Ok(u32::from_be_bytes(buffer))
    }

    fn read_i32(&mut self) -> Result<i32, AmqpFrameError> {
        if self.data.len() < 4 {
            return Err(AmqpFrameError::BufferTooShort);
        }

        let mut buffer: [u8; 4] = [0; 4];
        buffer.copy_from_slice(&self.data[..4]);
        self.data = &self.data[4..];

        Ok(i32::from_be_bytes(buffer))
    }

    fn read_u64(&mut self) -> Result<u64, AmqpFrameError> {
        if self.data.len() < 8 {
            return Err(AmqpFrameError::BufferTooShort);
        }

        let mut buffer: [u8; 8] = [0; 8];
        buffer.copy_from_slice(&self.data[..8]);
        self.data = &self.data[8..];

        Ok(u64::from_be_bytes(buffer))
    }

    fn read_i64(&mut self) -> Result<i64, AmqpFrameError> {
        if self.data.len() < 8 {
            return Err(AmqpFrameError::BufferTooShort);
        }

        let mut buffer: [u8; 8] = [0; 8];
        buffer.copy_from_slice(&self.data[..8]);
        self.data = &self.data[8..];

        Ok(i64::from_be_bytes(buffer))
    }

    fn read_f32(&mut self) -> Result<f32, AmqpFrameError> {
        if self.data.len() < 4 {
            return Err(AmqpFrameError::BufferTooShort);
        }

        let mut buffer: [u8; 4] = [0; 4];
        buffer.copy_from_slice(&self.data[..4]);
        self.data = &self.data[4..];

        Ok(f32::from_be_bytes(buffer))
    }

    fn read_f64(&mut self) -> Result<f64, AmqpFrameError> {
        if self.data.len() < 8 {
            return Err(AmqpFrameError::BufferTooShort);
        }

        let mut buffer: [u8; 8] = [0; 8];
        buffer.copy_from_slice(&self.data[..8]);
        self.data = &self.data[8..];

        Ok(f64::from_be_bytes(buffer))
    }

    fn read_bytes(&mut self, target: &mut [u8]) -> Result<(), AmqpFrameError> {
        if self.data.len() < target.len() {
            return Err(AmqpFrameError::BufferTooShort);
        }

        let length = target.len();
        target.copy_from_slice(&self.data[..length]);
        self.data = &self.data[length..];

        Ok(())
    }

    fn read_remaining_bytes(&mut self) -> Vec<u8> {
        let result = self.data.to_vec();
        self.data = &self.data[0..0];

        result
    }

    fn read_short_string(&mut self) -> Result<String, AmqpFrameError> {
        let length = self.read_u8()? as usize;
        let mut buffer = Vec::with_capacity(length);
        buffer.resize(length, b'\x00');

        self.read_bytes(&mut buffer)?;

        Ok(String::from_utf8(buffer)?)
    }

    fn read_long_string(&mut self) -> Result<String, AmqpFrameError> {
        let length = self.read_u32()? as usize;
        let mut buffer = Vec::with_capacity(length);
        buffer.resize(length, b'\x00');

        self.read_bytes(&mut buffer)?;

        Ok(String::from_utf8(buffer)?)
    }

    fn bytes_available(&self) -> usize {
        self.data.len()
    }

    fn read_table(&mut self) -> Result<HashMap<String, AmqpData>, AmqpFrameError> {
        let mut bytes_to_read = self.read_u32()? as usize;
        let mut result = HashMap::new();

        while bytes_to_read > 0 {
            let bytes_before = self.bytes_available();
            let key = self.read_short_string()?;

            let value_type = self.read_u8()?;
            let value = self.read_value(value_type)?;

            result.insert(key, value);
            bytes_to_read -= bytes_before - self.bytes_available();
        }

        Ok(result)
    }

    fn read_array(&mut self) -> Result<Vec<AmqpData>, AmqpFrameError> {
        let mut bytes_to_read = self.read_u32()? as usize;
        let mut result = Vec::new();

        while bytes_to_read > 0 {
            let bytes_before = self.bytes_available();

            let value_type = self.read_u8()?;
            let value = self.read_value(value_type)?;

            result.push(value);
            bytes_to_read -= bytes_before - self.bytes_available();
        }

        Ok(result)
    }

    fn read_value(&mut self, value_type: u8) -> Result<AmqpData, AmqpFrameError> {
        match value_type {
            b't' => Ok(AmqpData::Bool(self.read_u8()? > 0)),
            b'b' => Ok(AmqpData::I8(self.read_i8()?)),
            b'B' => Ok(AmqpData::U8(self.read_u8()?)),
            b'U' => Ok(AmqpData::I16(self.read_i16()?)),
            b'u' => Ok(AmqpData::U16(self.read_u16()?)),
            b'I' => Ok(AmqpData::I32(self.read_i32()?)),
            b'i' => Ok(AmqpData::U32(self.read_u32()?)),
            b'L' => Ok(AmqpData::I64(self.read_i64()?)),
            b'l' => Ok(AmqpData::U64(self.read_u64()?)),
            b'f' => Ok(AmqpData::Float(self.read_f32()?)),
            b'd' => Ok(AmqpData::Double(self.read_f64()?)),
            b'D' => Ok(AmqpData::Decimal(self.read_u8()?, self.read_u32()?)),
            b's' => Ok(AmqpData::ShortString(self.read_short_string()?)),
            b'S' => Ok(AmqpData::LongString(self.read_long_string()?)),
            b'T' => Ok(AmqpData::Timestamp(self.read_u64()?)),
            b'V' => Ok(AmqpData::None),
            b'F' => Ok(AmqpData::FieldTable(self.read_table()?)),
            b'A' => Ok(AmqpData::FieldArray(self.read_array()?)),
            _ => Err(AmqpFrameError::InvalidFieldType(value_type))
        }
    }

}
