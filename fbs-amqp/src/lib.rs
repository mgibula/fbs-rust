use std::collections::HashMap;
use std::string::FromUtf8Error;
use fbs_library::system_error::SystemError;
use fbs_runtime::resolver::ResolveAddressError;
use thiserror::Error;

mod defines;
mod frame;
mod frame_reader;
mod frame_writer;
mod connection;
mod channel;

pub type AmqpConsumer = Box<dyn Fn(u64, bool, String, String, AmqpMessage)>;
pub type AmqpConfirmAckCallback = Box<dyn Fn(u64, bool)>;
pub type AmqpConfirmNackCallback = Box<dyn Fn(u64, AmqpNackFlags)>;

pub use connection::{AmqpConnection, AmqpConnectionParams};
pub use channel::{AmqpChannel, AmqpChannelPublisher};

#[derive(Error, Debug, Clone)]
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
    FrameEndInvalid,
    #[error("Frame error: {0}")]
    FrameError(#[from] AmqpFrameError),
    #[error("Connection closed by server - {1}")]
    ConnectionClosedByServer(u16, String, u16, u16),
    #[error("Protocol error")]
    ProtocolError(&'static str),
    #[error("Channel closed by server - {1}")]
    ChannelClosedByServer(u16, String, u16, u16),
    #[error("Invalid parameters")]
    InvalidParameters,
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

#[derive(Debug, Default, Clone, Copy)]
pub struct AmqpExchangeFlags {
    flags: u8,
}

impl AmqpExchangeFlags {
    pub fn new() -> Self {
        Self { flags: 0 }
    }

    pub fn passive(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 0;
        } else {
            self.flags &= !(1 << 0);
        }

        self
    }

    pub fn durable(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 1;
        } else {
            self.flags &= !(1 << 1);
        }

        self
    }

    pub fn no_wait(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 4;
        } else {
            self.flags &= !(1 << 4);
        }

        self
    }

    fn has_no_wait(self) -> bool {
        (self.flags & (1 << 4)) != 0
    }
}

impl Into<u8> for AmqpExchangeFlags {
    fn into(self) -> u8 {
        self.flags
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AmqpDeleteExchangeFlags {
    flags: u8,
}

impl AmqpDeleteExchangeFlags {
    pub fn new() -> Self {
        Self { flags: 0 }
    }

    pub fn if_unused(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 0;
        } else {
            self.flags &= !(1 << 0);
        }

        self
    }

    pub fn no_wait(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 1;
        } else {
            self.flags &= !(1 << 1);
        }

        self
    }

    fn has_no_wait(self) -> bool {
        (self.flags & (1 << 1)) != 0
    }
}

impl Into<u8> for AmqpDeleteExchangeFlags {
    fn into(self) -> u8 {
        self.flags
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AmqpQueueFlags {
    flags: u8,
}

impl AmqpQueueFlags {
    pub fn new() -> Self {
        Self { flags: 0 }
    }

    pub fn passive(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 0;
        } else {
            self.flags &= !(1 << 0);
        }

        self
    }

    pub fn durable(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 1;
        } else {
            self.flags &= !(1 << 1);
        }

        self
    }

    pub fn exclusive(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 2;
        } else {
            self.flags &= !(1 << 2);
        }

        self
    }

    pub fn auto_delete(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 3;
        } else {
            self.flags &= !(1 << 3);
        }

        self
    }

    pub fn no_wait(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 4;
        } else {
            self.flags &= !(1 << 4);
        }

        self
    }

    fn has_no_wait(self) -> bool {
        (self.flags & (1 << 4)) != 0
    }
}

impl Into<u8> for AmqpQueueFlags {
    fn into(self) -> u8 {
        self.flags
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AmqpDeleteQueueFlags {
    flags: u8,
}

impl AmqpDeleteQueueFlags {
    pub fn new() -> Self {
        Self { flags: 0 }
    }

    pub fn if_unused(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 0;
        } else {
            self.flags &= !(1 << 0);
        }

        self
    }

    pub fn if_empty(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 1;
        } else {
            self.flags &= !(1 << 1);
        }

        self
    }

    pub fn no_wait(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 2;
        } else {
            self.flags &= !(1 << 2);
        }

        self
    }

    fn has_no_wait(self) -> bool {
        (self.flags & (1 << 2)) != 0
    }
}

impl Into<u8> for AmqpDeleteQueueFlags {
    fn into(self) -> u8 {
        self.flags
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AmqpConsumeFlags {
    flags: u8,
}

impl AmqpConsumeFlags {
    pub fn new() -> Self {
        Self { flags: 0 }
    }

    pub fn no_local(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 0;
        } else {
            self.flags &= !(1 << 0);
        }

        self
    }

    pub fn no_ack(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 1;
        } else {
            self.flags &= !(1 << 1);
        }

        self
    }

    pub fn exclusive(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 2;
        } else {
            self.flags &= !(1 << 2);
        }

        self
    }

    pub fn no_wait(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 3;
        } else {
            self.flags &= !(1 << 3);
        }

        self
    }

    fn has_no_wait(self) -> bool {
        (self.flags & (1 << 3)) != 0
    }
}

impl Into<u8> for AmqpConsumeFlags {
    fn into(self) -> u8 {
        self.flags
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AmqpPublishFlags {
    flags: u8,
}

impl AmqpPublishFlags {
    pub fn new() -> Self {
        Self { flags: 0 }
    }

    pub fn mandatory(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 0;
        } else {
            self.flags &= !(1 << 0);
        }

        self
    }

    pub fn immediate(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 1;
        } else {
            self.flags &= !(1 << 1);
        }

        self
    }
}

impl Into<u8> for AmqpPublishFlags {
    fn into(self) -> u8 {
        self.flags
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AmqpNackFlags {
    flags: u8,
}

impl AmqpNackFlags {
    pub fn new() -> Self {
        Self { flags: 0 }
    }

    pub fn multiple(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 0;
        } else {
            self.flags &= !(1 << 0);
        }

        self
    }

    pub fn requeue(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 1;
        } else {
            self.flags &= !(1 << 1);
        }

        self
    }
}

impl Into<u8> for AmqpNackFlags {
    fn into(self) -> u8 {
        self.flags
    }
}

impl From<u8> for AmqpNackFlags {
    fn from(value: u8) -> Self {
        Self { flags: value }
    }
}