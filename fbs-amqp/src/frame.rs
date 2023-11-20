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
    Header(u16, u64, AmqpBasicProperties),
    Content(Vec<u8>),
    Heartbeat(AmqpHeartbeatFrame),
}

#[derive(Debug, Default, Clone)]
pub struct AmqpBasicProperties {
    pub content_type: Option<String>,                   // bit 15
    pub content_encoding: Option<String>,               // bit 14
    pub headers: Option<HashMap<String, AmqpData>>,     // bit 13
    pub delivery_mode: Option<u8>,                      // bit 12
    pub priority: Option<u8>,                           // bit 11
    pub correlation_id: Option<String>,                 // bit 10
    pub reply_to: Option<String>,                       // bit 9
    pub expiration: Option<String>,                     // bit 8
    pub message_id: Option<String>,                     // bit 7
    pub timestamp: Option<u64>,                         // bit 6
    pub message_type: Option<String>,                   // bit 5
    pub user_id: Option<String>,                        // bit 4
    pub app_id: Option<String>,                         // bit 3
    pub cluster_id: Option<String>,                     // bit 2
}

#[derive(Debug, Default, Clone)]
pub struct AmqpMessage {
    pub properties: AmqpBasicProperties,
    pub content: Vec<u8>,
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
    BasicPublish(String, String, u8),                                               // exchange, routing-key, flags
    BasicReturn(i16, String, String, String),                                       // return-code, reply-text, exchange, routing-key
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
    #[error("Invalid frame type - {0}")]
    InvalidFrameType(u8),
    #[error("Invalid class/method - {0}/{1}")]
    InvalidClassMethod(u16, u16),
    #[error("Invalid string utf-8 format")]
    InvalidStringFormat(#[from] FromUtf8Error),
    #[error("Invalid field type - {0}")]
    InvalidFieldType(u8),
}
