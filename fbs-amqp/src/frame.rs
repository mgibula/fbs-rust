use super::defines::PROTOCOL_HEADER;
use super::{AmqpBasicProperties, AmqpData};
use std::collections::HashMap;

pub(super) struct AmqpProtocolHeader;

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
    Heartbeat(),
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
    BasicDeliver(String, u64, bool, String, String),                                // consumer-tag, delivery-tag, redelivered, exchange, routing-key
    BasicAck(u64, bool),                                                            // delivery-tag, multiple
    BasicGet(String, bool),                                                         // queue, no-ack
    BasicGetOk(u64, bool, String, String, u32),                                     // delivery-tag, redelivered, exchange, routing-key, messages
    BasicGetEmpty(),
    BasicReject(u64, bool),                                                         // delivery-tag, requeue
    BasicRecover(bool),                                                             // requeue
    BasicRecoverOk(),
    BasicNack(u64, u8),                                                             // delivery-tag, multiple, requeue
    ConfirmSelect(bool),                                                            // no-wait
    ConfirmSelectOk(),
}
