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
    ConnectionStart(u8, u8, HashMap<String, AmqpData>, String, String),     // version-major, version-minor, server-properties, mechanisms, locales
    ConnectionStartOk(HashMap<String, AmqpData>, String, String, String),   // client-properties, mechanism, response, locale
    ConnectionTune(u16, u32, u16),                                          // channel-max, frame-max, heartbeat
    ConnectionTuneOk(u16, u32, u16),                                        // channel-max, frame-max, heartbeat
    ConnectionOpen(String),                                                 // virtual host
    ConnectionOpenOk(),
    ConnectionClose(u16, String, u16, u16),                                 // reply-code, reply-text, class-id, method-id
    ConnectionCloseOk(),
    ChannelOpen(),
    ChannelOpenOk(),
    ChannelClose(u16, String, u16, u16),                                    // reply-code, reply-text, class-id, method-id
    ChannelCloseOk(),
    ChannelFlow(bool),                                                      // active
    ChannelFlowOk(bool),                                                    // active
    // Channel(ChannelMethod),
    // Exchange(ExchangeMethod),
    // Queue(QueueMethod),
    // Basic(BasicMethod),
    // Tx(TxMethod),
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

#[derive(Error, Debug)]
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
