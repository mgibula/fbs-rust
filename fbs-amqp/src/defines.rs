use std::collections::HashMap;
use thiserror::Error;
use crate::buffer::BufferTooShort;

use super::buffer::ReadBuffer;

pub const PROTOCOL_HEADER: &[u8] = b"AMQP\x00\x00\x09\x01";

#[derive(Debug, Clone)]
struct AmqpMethodFrame {
    method: AmqpMethod,
}

#[derive(Debug, Clone)]
struct AmqpHeaderFrame {

}

#[derive(Debug, Clone)]
struct AmqpContentFrame {

}

#[derive(Debug, Clone)]
struct AmqpHeartbeatFrame {

}

#[derive(Debug, Clone)]
enum AmqpFramePayload {
    Method(AmqpMethod),
    Header(AmqpHeaderFrame),
    Content(AmqpContentFrame),
    Heartbeat(AmqpHeartbeatFrame),
}

#[derive(Debug, Clone)]
pub struct AmqpFrame {
    channel: u16,
    payload: AmqpFramePayload,
}

#[derive(Debug, Error)]
pub enum AmqpFrameError {
    #[error("Method error")]
    MethodError(#[from] AmqpMethodError)
}

impl AmqpFrame {
    pub fn new_method_frame(channel: u16, payload: &[u8]) -> Result<Self, AmqpFrameError> {
        Ok(Self { channel, payload: AmqpFramePayload::Method(AmqpMethod::from_buffer(payload)?) })
    }

    pub fn new_header_frame(channel: u16, payload: &[u8]) -> Result<Self, AmqpFrameError> {
        Ok(Self { channel, payload: AmqpFramePayload::Header(AmqpHeaderFrame { }) })
    }

    pub fn new_content_frame(channel: u16, payload: &[u8]) -> Result<Self, AmqpFrameError> {
        Ok(Self { channel, payload: AmqpFramePayload::Content(AmqpContentFrame { }) })
    }

    pub fn new_heartbeat_frame(channel: u16, payload: &[u8]) -> Result<Self, AmqpFrameError> {
        Ok(Self { channel, payload: AmqpFramePayload::Heartbeat(AmqpHeartbeatFrame { }) })
    }
}

#[derive(Debug, Clone)]
enum AmqpMethod {
    Connection(AmqpConnectionMethod),
    // Channel(ChannelMethod),
    // Exchange(ExchangeMethod),
    // Queue(QueueMethod),
    // Basic(BasicMethod),
    // Tx(TxMethod),
}

#[derive(Debug, Clone)]
enum AmqpConnectionMethod {
    Start(u8, u8, u32, String, String),
}

#[derive(Error, Debug)]
pub enum AmqpMethodError {
    #[error("Method unknown")]
    MethodUnknown(u16),
    #[error("Received data format invalid")]
    FormatError(#[from] BufferTooShort),
}

impl AmqpMethod {
    pub fn from_buffer(buffer: &[u8]) -> Result<AmqpMethod, AmqpMethodError> {
        let mut buffer = ReadBuffer::new(buffer);
        let class_id = buffer.read_u16()?;
        let method_id = buffer.read_u16()?;

        match class_id {
            class_id if class_id == AmqpClassId::Connection as u16 => {
                Self::new_connection_method(method_id, &mut buffer)
            },
            class_id if class_id == AmqpClassId::Channel as u16 => {
                Self::new_channel_method(method_id, &mut buffer)
            },
            class_id if class_id == AmqpClassId::Exchange as u16 => {
                Self::new_exchange_method(method_id, &mut buffer)
            },
            class_id if class_id == AmqpClassId::Queue as u16 => {
                Self::new_queue_method(method_id, &mut buffer)
            },
            class_id if class_id == AmqpClassId::Basic as u16 => {
                Self::new_basic_method(method_id, &mut buffer)
            },
            class_id if class_id == AmqpClassId::Tx as u16 => {
                Self::new_tx_method(method_id, &mut buffer)
            },
            _ => return Err(AmqpMethodError::MethodUnknown(class_id)),
        }
    }

    fn new_connection_method(method_id: u16, buffer: &mut ReadBuffer) -> Result<AmqpMethod, AmqpMethodError> {
        match method_id {
            method_id if method_id == AmqpConnectionMethodId::Start as u16 => {
                let major = buffer.read_u8()?;
                let minor = buffer.read_u8()?;

                let table_count = buffer.read_u32()?;
                let field1 = buffer.read_short_string()?;
                Ok(AmqpMethod::Connection(AmqpConnectionMethod::Start(major, minor, table_count, field1, String::new())))
            },
            _ => return Err(AmqpMethodError::MethodUnknown(method_id))
        }
    }

    fn new_channel_method(method_id: u16, buffer: &mut ReadBuffer) -> Result<AmqpMethod, AmqpMethodError> {
        unimplemented!()
    }

    fn new_exchange_method(method_id: u16, buffer: &mut ReadBuffer) -> Result<AmqpMethod, AmqpMethodError> {
        unimplemented!()
    }

    fn new_queue_method(method_id: u16, buffer: &mut ReadBuffer) -> Result<AmqpMethod, AmqpMethodError> {
        unimplemented!()
    }

    fn new_basic_method(method_id: u16, buffer: &mut ReadBuffer) -> Result<AmqpMethod, AmqpMethodError> {
        unimplemented!()
    }

    fn new_tx_method(method_id: u16, buffer: &mut ReadBuffer) -> Result<AmqpMethod, AmqpMethodError> {
        unimplemented!()
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum AmqpFrameType {
    Method          = 1,
    Header          = 2,
    Body            = 3,
    Heartbeat       = 4,
}

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
enum AmqpClassId {
    Connection      = 10,
    Channel         = 20,
    Exchange        = 40,
    Queue           = 50,
    Basic           = 60,
    Tx              = 90,
}

#[derive(Debug, Clone)]
#[repr(u16)]
enum AmqpConnectionMethodId {
    Start           = 10,
    StartOk         = 11,
    Secure          = 20,
    SecureOk        = 21,
    Tune            = 30,
    TuneOk          = 31,
    Open            = 40,
    OpenOk          = 41,
    Close           = 50,
    CloseOk         = 51,
}

#[derive(Debug, Clone)]
enum AmqpChannelMethodId {
    Open            = 10,
    OpenOk          = 11,
    Flow            = 20,
    FlowOk          = 21,
    Close           = 40,
    CloseOk         = 41,
}

#[derive(Debug, Clone)]
enum AmqpExchangeMethodId {
    Declare         = 10,
    DeclareOk       = 11,
    Delete          = 20,
    DeleteOk        = 21,
}

#[derive(Debug, Clone)]
enum AmqpQueueMethodId {
    Declare         = 10,
    DeclareOk       = 11,
    Bind            = 20,
    BindOk          = 21,
    Unbind          = 50,
    UnbindOk        = 51,
    Purge           = 30,
    PurgeOk         = 31,
    Delete          = 40,
    DeleteOk        = 41,
}

#[derive(Debug, Clone)]
enum AmqpBasicMethodId {
    Qos             = 10,
    QosOk           = 11,
    Consume         = 20,
    ConsumeOk       = 21,
    Cancel          = 30,
    CancelOk        = 31,
    Publish         = 40,
    Return          = 50,
    Deliver         = 60,
    Get             = 70,
    GetOk           = 71,
    GetEmpty        = 72,
    Ack             = 80,
    Reject          = 90,
    RecoverAsync    = 100,
    Recover         = 110,
    RecoverOk       = 111,
}

#[derive(Debug, Clone)]
enum AmqpTxMethodId {
    Select          = 10,
    SelectOk        = 11,
    Commit          = 20,
    CommitOk        = 21,
    Rollback        = 30,
    RollbackOk      = 31,
}

#[repr(u8)]
#[derive(Debug, Clone)]
enum AmqpDataType {
    Bool            = 't' as u8,
    I8              = 'b' as u8,
    U8              = 'B' as u8,
    I16             = 'U' as u8,
    U16             = 'u' as u8,
    I32             = 'I' as u8,
    U32             = 'i' as u8,
    I64             = 'L' as u8,
    U64             = 'l' as u8,
    Float           = 'f' as u8,
    Double          = 'd' as u8,
    Decimal         = 'D' as u8,
    ShortString     = 's' as u8,
    LongString      = 'S' as u8,
    FieldArray      = 'A' as u8,
    Timestamp       = 'T' as u8,
    FieldTable      = 'F' as u8,
    None            = 'V' as u8,
}

#[derive(Debug, Clone)]
pub enum AmqpData {
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    Float(f32),
    Double(f64),
    Decimal(u8, u32),
    ShortString(Vec<u8>),
    LongString(Vec<u8>),
    FieldArray(Vec<AmqpData>),
    Timestamp(u64),
    FieldTable(HashMap<String, AmqpData>),
}

impl AmqpData {
    pub fn from_buffer(buffer: &[u8]) -> Result<AmqpData, AmqpMethodError> {
        unimplemented!()
    }
}