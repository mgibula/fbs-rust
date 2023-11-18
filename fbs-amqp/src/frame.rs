use super::defines::PROTOCOL_HEADER;
use std::{collections::HashMap, string::FromUtf8Error};
use thiserror::Error;

pub(crate) struct AmqpProtocolHeader { }

impl AmqpProtocolHeader {
    pub fn new() -> Self {
        Self { }
    }
}

impl Into<Vec<u8>> for AmqpProtocolHeader {
    fn into(self) -> Vec<u8> {
        PROTOCOL_HEADER.into()
    }
}

#[derive(Debug, Clone)]
pub struct AmqpFrame {
    pub channel: u16,
    pub payload: AmqpFramePayload,
}

#[derive(Debug, Clone)]
pub enum AmqpFramePayload {
    Method(AmqpMethod),
    Header(AmqpHeaderFrame),
    Content(AmqpContentFrame),
    Heartbeat(AmqpHeartbeatFrame),
}

#[derive(Debug, Clone)]
pub enum AmqpMethod {
    ConnectionStart(u8, u8, HashMap<String, AmqpData>, String, String),             // version-major, version-minor, server-properties, mechanisms, locales
    ConnectionStartOk(HashMap<String, AmqpData>, String, String, String),           // client-properties, mechanism, response, locale
    ConnectionTune(u16, u32, u16),                                                  // channel-max, frame-max, heartbeat
    ConnectionTuneOk(u16, u32, u16),                                                // channel-max, frame-max, heartbeat
    ConnectionOpen(String),                                                         // virtual host
    ConnectionOpenOk(),
    ConnectionClose(u16, String, u16, u16),                                         // reply-code, reply-text, class-id, method-id
    ConnectionCloseOk(),
    ChannelOpen(),
    ChannelOpenOk(),
    ChannelClose(u16, String, u16, u16),                                            // reply-code, reply-text, class-id, method-id
    ChannelCloseOk(),
    ChannelFlow(bool),                                                              // active
    ChannelFlowOk(bool),                                                            // active
    ExchangeDeclare(String, String, u8, HashMap<String, AmqpData>),                 // name, type, flags, arguments
    ExchangeDeclareOk(),
    ExchangeDelete(String, u8),                                                     // name, flags
    ExchangeDeleteOk(),
    QueueDeclare(String, u8, HashMap<String, AmqpData>),                            // name, flags, arguments
    QueueDeclareOk(String, i32, i32),                                               // name, messages, consumers
    QueueBind(String, String, String, u8, HashMap<String, AmqpData>),               // name, exchange, routing-key, flags, arguments
    QueueBindOk(),
    QueueUnbind(String, String, String, HashMap<String, AmqpData>),                 // name, exchange, routing-key, arguments
    QueueUnbindOk(),
    QueuePurge(String, u8),                                                         // name, flags
    QueuePurgeOk(i32),                                                              // messages
    QueueDelete(String, u8),                                                        // name, flags
    QueueDeleteOk(i32),                                                             // messages
    BasicQos(i32, i16, bool),                                                       // size, count, global
    BasicQosOk(),
    BasicConsume(String, String, u8, HashMap<String, AmqpData>),                    // queue, tag, flags, arguments
    BasicConsumeOk(String),                                                         // tag
    BasicCancel(String, u8),                                                        // tag, no-wait
    BasicCancelOk(String),                                                          // tag
}

#[derive(Debug, Clone)]
pub struct AmqpHeaderFrame {

}

#[derive(Debug, Clone)]
pub struct AmqpContentFrame {

}

#[derive(Debug, Clone)]
pub struct AmqpHeartbeatFrame {

}

#[derive(Debug, Clone)]
pub enum AmqpData {
    None,
    Bool(bool),
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
    ShortString(String),
    LongString(String),
    FieldArray(Vec<AmqpData>),
    Timestamp(u64),
    FieldTable(HashMap<String, AmqpData>),
}

#[derive(Error, Debug, Clone)]
pub enum AmqpFrameError {
    #[error("Buffer too short")]
    BufferTooShort,
    #[error("Invalid frame type")]
    InvalidFrameType(u8),
    #[error("Invalid class/method")]
    InvalidClassMethod(u16, u16),
    #[error("Invalid string utf-8 format")]
    InvalidStringFormat(#[from] FromUtf8Error),
    #[error("Invalid field type")]
    InvalidFieldType(u8),
}
