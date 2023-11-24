pub const PROTOCOL_HEADER: &[u8] = b"AMQP\x00\x00\x09\x01";

pub const AMQP_FRAME_TYPE_METHOD: u8            = 1;
pub const AMQP_FRAME_TYPE_HEADER: u8            = 2;
pub const AMQP_FRAME_TYPE_CONTENT: u8           = 3;
pub const AMQP_FRAME_TYPE_HEARTBEAT: u8         = 8;    // rabbitmq uses 8

pub const AMQP_CLASS_CONNECTION: u16            = 10;
pub const AMQP_CLASS_CHANNEL: u16               = 20;
pub const AMQP_CLASS_EXCHANGE: u16              = 40;
pub const AMQP_CLASS_QUEUE: u16                 = 50;
pub const AMQP_CLASS_BASIC: u16                 = 60;
pub const AMQP_CLASS_CONFIRM: u16               = 85;
pub const AMQP_CLASS_TX: u16                    = 90;

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

pub const AMQP_METHOD_CHANNEL_OPEN: u16         = 10;
pub const AMQP_METHOD_CHANNEL_OPEN_OK: u16      = 11;
pub const AMQP_METHOD_CHANNEL_FLOW: u16         = 20;
pub const AMQP_METHOD_CHANNEL_FLOW_OK: u16      = 21;
pub const AMQP_METHOD_CHANNEL_CLOSE: u16        = 40;
pub const AMQP_METHOD_CHANNEL_CLOSE_OK: u16     = 41;

pub const AMQP_METHOD_EXCHANGE_DECLARE: u16     = 10;
pub const AMQP_METHOD_EXCHANGE_DECLARE_OK: u16  = 11;
pub const AMQP_METHOD_EXCHANGE_DELETE: u16      = 20;
pub const AMQP_METHOD_EXCHANGE_DELETE_OK: u16   = 21;

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
pub const AMQP_METHOD_BASIC_NACK: u16           = 120;

pub const AMQP_METHOD_CONFIRM_SELECT: u16       = 10;
pub const AMQP_METHOD_CONFIRM_SELECT_OK: u16    = 11;

pub const AMQP_BASIC_PROPERTY_CONTENT_TYPE_BIT: u8      = 15;
pub const AMQP_BASIC_PROPERTY_CONTENT_ENCODING_BIT: u8  = 14;
pub const AMQP_BASIC_PROPERTY_HEADERS_BIT: u8           = 13;
pub const AMQP_BASIC_PROPERTY_DELIVERY_MNODE_BIT: u8    = 12;
pub const AMQP_BASIC_PROPERTY_PRIORITY_BIT: u8          = 11;
pub const AMQP_BASIC_PROPERTY_CORRELATION_ID_BIT: u8    = 10;
pub const AMQP_BASIC_PROPERTY_REPLY_TO_BIT: u8          = 9;
pub const AMQP_BASIC_PROPERTY_EXPIRATION_BIT: u8        = 8;
pub const AMQP_BASIC_PROPERTY_MESSAGE_ID_BIT: u8        = 7;
pub const AMQP_BASIC_PROPERTY_TIMESTAMP_BIT: u8         = 6;
pub const AMQP_BASIC_PROPERTY_TYPE_BIT: u8              = 5;
pub const AMQP_BASIC_PROPERTY_USER_ID_BIT: u8           = 4;
pub const AMQP_BASIC_PROPERTY_APP_ID_BIT: u8            = 3;
pub const AMQP_BASIC_PROPERTY_CLUSTER_ID_BIT: u8        = 2;
