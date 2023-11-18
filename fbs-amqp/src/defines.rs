use thiserror::Error;

pub const PROTOCOL_HEADER: &[u8] = b"AMQP\x00\x00\x09\x01";

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum AmqpFrameType {
    Method          = 1,
    Header          = 2,
    Body            = 3,
    Heartbeat       = 4,
}

pub const AMQP_FRAME_TYPE_METHOD: u8            = 1;
pub const AMQP_FRAME_TYPE_HEADER: u8            = 2;
pub const AMQP_FRAME_TYPE_CONTENT: u8           = 3;
pub const AMQP_FRAME_TYPE_HEARTBEAT: u8         = 4;

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

pub const AMQP_CLASS_CONNECTION: u16            = 10;
pub const AMQP_CLASS_CHANNEL: u16               = 20;
pub const AMQP_CLASS_EXCHANGE: u16              = 40;
pub const AMQP_CLASS_QUEUE: u16                 = 50;
pub const AMQP_CLASS_BASIC: u16                 = 60;
pub const AMQP_CLASS_TX: u16                    = 90;

#[derive(Debug, Clone, Copy)]
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

pub const AMQP_METHOD_CONNECTION_START: u16     = 10;
pub const AMQP_METHOD_CONNECTION_START_OK: u16  = 11;
pub const AMQP_METHOD_CONNECTION_SECURE: u16    = 20;
pub const AMQP_METHOD_CONNECTION_SECURE_OK: u16 = 21;
pub const AMQP_METHOD_CONNECTION_TUNE: u16      = 30;
pub const AMQP_METHOD_CONNECTION_TUNE_OK: u16   = 31;
pub const AMQP_METHOD_CONNECTION_OPEN: u16      = 40;
pub const AMQP_METHOD_CONNECTION_OPEN_OK: u16   = 41;
pub const AMQP_METHOD_CONNECTION_CLOSE: u16     = 50;
pub const AMQP_METHOD_CONNECTION_CLOSE_OK: u16  = 51;

#[derive(Debug, Clone, Copy)]
enum AmqpChannelMethodId {
    Open            = 10,
    OpenOk          = 11,
    Flow            = 20,
    FlowOk          = 21,
    Close           = 40,
    CloseOk         = 41,
}

pub const AMQP_METHOD_CHANNEL_OPEN: u16         = 10;
pub const AMQP_METHOD_CHANNEL_OPEN_OK: u16      = 11;
pub const AMQP_METHOD_CHANNEL_FLOW: u16         = 20;
pub const AMQP_METHOD_CHANNEL_FLOW_OK: u16      = 21;
pub const AMQP_METHOD_CHANNEL_CLOSE: u16        = 40;
pub const AMQP_METHOD_CHANNEL_CLOSE_OK: u16     = 41;

#[derive(Debug, Clone, Copy)]
enum AmqpExchangeMethodId {
    Declare         = 10,
    DeclareOk       = 11,
    Delete          = 20,
    DeleteOk        = 21,
}

pub const AMQP_METHOD_EXCHANGE_DECLARE: u16     = 10;
pub const AMQP_METHOD_EXCHANGE_DECLARE_OK: u16  = 11;
pub const AMQP_METHOD_EXCHANGE_DELETE: u16      = 20;
pub const AMQP_METHOD_EXCHANGE_DELETE_OK: u16   = 21;

#[derive(Debug, Clone, Copy)]
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

pub const AMQP_METHOD_QUEUE_DECLARE: u16        = 10;
pub const AMQP_METHOD_QUEUE_DECLARE_OK: u16     = 11;
pub const AMQP_METHOD_QUEUE_BIND: u16           = 20;
pub const AMQP_METHOD_QUEUE_BIND_OK: u16        = 21;
pub const AMQP_METHOD_QUEUE_UNBIND: u16         = 50;
pub const AMQP_METHOD_QUEUE_UNBIND_OK: u16      = 51;
pub const AMQP_METHOD_QUEUE_PURGE: u16          = 30;
pub const AMQP_METHOD_QUEUE_PURGE_OK: u16       = 31;
pub const AMQP_METHOD_QUEUE_DELETE: u16         = 40;
pub const AMQP_METHOD_QUEUE_DELETE_OK: u16      = 41;

#[derive(Debug, Clone, Copy)]
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

pub const AMQP_METHOD_BASIC_QOS: u16            = 10;
pub const AMQP_METHOD_BASIC_QOS_OK: u16         = 11;
pub const AMQP_METHOD_BASIC_CONSUME: u16        = 20;
pub const AMQP_METHOD_BASIC_CONSUME_OK: u16     = 21;
pub const AMQP_METHOD_BASIC_CANCEL: u16         = 30;
pub const AMQP_METHOD_BASIC_CANCEL_OK: u16      = 31;
pub const AMQP_METHOD_BASIC_PUBLISH: u16        = 40;
pub const AMQP_METHOD_BASIC_RETURN: u16         = 50;
pub const AMQP_METHOD_BASIC_DELIVER: u16        = 60;
pub const AMQP_METHOD_BASIC_GET: u16            = 70;
pub const AMQP_METHOD_BASIC_GET_OK: u16         = 71;
pub const AMQP_METHOD_BASIC_GET_EMPTY: u16      = 72;
pub const AMQP_METHOD_BASIC_ACK: u16            = 80;
pub const AMQP_METHOD_BASIC_REJECT: u16         = 90;
pub const AMQP_METHOD_BASIC_RECOVERY_ASYNC: u16 = 100;
pub const AMQP_METHOD_BASIC_RECOVER: u16        = 110;
pub const AMQP_METHOD_BASIC_RECOVER_OK: u16     = 111;

#[derive(Debug, Clone, Copy)]
enum AmqpTxMethodId {
    Select          = 10,
    SelectOk        = 11,
    Commit          = 20,
    CommitOk        = 21,
    Rollback        = 30,
    RollbackOk      = 31,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
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

