use std::collections::HashMap;

const PROTOCOL_HEADER: &[u8] = b"AMQP\0\091";

struct FrameHeader {
    frame_type: u8,
    channel: u16,
    size: u32,
}

struct FramePayload {
    payload: Vec<u8>,
}

struct FrameEnd {
    frame_end: u8,
}

#[non_exhaustive]
enum AmqpMethod {
    Connection      = 10,
    Channel         = 20,
    Exchange        = 40,
    Queue           = 50,
    Basic           = 60,
    Tx              = 90,
}

#[non_exhaustive]
enum ConnectionMethod {
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

#[non_exhaustive]
enum ChannelMethod {
    Open            = 10,
    OpenOk          = 11,
    Flow            = 20,
    FlowOk          = 21,
    Close           = 40,
    CloseOk         = 41,
}

#[non_exhaustive]
enum ExchangeMethod {
    Declare         = 10,
    DeclareOk       = 11,
    Delete          = 20,
    DeleteOk        = 21,
}

#[non_exhaustive]
enum QueueMethod {
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

#[non_exhaustive]
enum BasicMethod {
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

#[non_exhaustive]
enum TxMethod {
    Select          = 10,
    SelectOk        = 11,
    Commit          = 20,
    CommitOk        = 21,
    Rollback        = 30,
    RollbackOk      = 31,
}

#[repr(u8)]
#[non_exhaustive]
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

enum AmqpData {
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