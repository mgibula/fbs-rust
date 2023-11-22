use std::collections::HashMap;

use super::frame::{AmqpFrame, AmqpFramePayload, AmqpMethod, AmqpData, AmqpBasicProperties};
use super::defines::*;

pub struct FrameWriter;

impl FrameWriter {
    pub fn write_frame(frame: &AmqpFrame) -> Vec<u8> {
        let mut result = Vec::new();

        match &frame.payload {
            AmqpFramePayload::Method(_) => write_u8(&mut result, AMQP_FRAME_TYPE_METHOD),
            AmqpFramePayload::Header(_, _, _) => write_u8(&mut result, AMQP_FRAME_TYPE_HEADER),
            AmqpFramePayload::Content(_) => write_u8(&mut result, AMQP_FRAME_TYPE_CONTENT),
            _ => (),
        }

        write_u16(&mut result, frame.channel);

        let payload = FrameWriter::serialize_frame(frame);
        write_u32(&mut result, payload.len() as u32);
        write_bytes(&mut result, &payload);
        write_u8(&mut result, b'\xCE');

        result
    }

    fn serialize_frame(frame: &AmqpFrame) -> Vec<u8> {
        match &frame.payload {
            AmqpFramePayload::Method(method) => FrameWriter::serialize_method_frame(method),
            AmqpFramePayload::Header(class, size, properties) => FrameWriter::serialize_header_frame(*class, *size, properties),
            AmqpFramePayload::Content(data) => FrameWriter::serialize_content_frame(data),
            _ => panic!("Attempting to write unsupported frame type"),
        }
    }

    fn serialize_content_frame(data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }

    fn serialize_header_frame(class_id: u16, size: u64, properties: &AmqpBasicProperties) -> Vec<u8> {
        let mut properties_mask: u16 = 0;
        let mut buffer = Vec::new();
        match &properties.content_type {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_CONTENT_TYPE_BIT;
                write_short_string(&mut buffer, value);
            }
        }

        match &properties.content_encoding {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_CONTENT_ENCODING_BIT;
                write_short_string(&mut buffer, value);
            }
        }

        match &properties.headers {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_HEADERS_BIT;
                write_table(&mut buffer, value);
            }
        }

        match &properties.delivery_mode {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_DELIVERY_MNODE_BIT;
                write_u8(&mut buffer, *value);
            }
        }

        match &properties.priority {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_PRIORITY_BIT;
                write_u8(&mut buffer, *value);
            }
        }

        match &properties.correlation_id {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_CORRELATION_ID_BIT;
                write_short_string(&mut buffer, value);
            }
        }

        match &properties.reply_to {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_REPLY_TO_BIT;
                write_short_string(&mut buffer, value);
            }
        }

        match &properties.expiration {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_EXPIRATION_BIT;
                write_short_string(&mut buffer, value);
            }
        }

        match &properties.message_id {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_MESSAGE_ID_BIT;
                write_short_string(&mut buffer, value);
            }
        }

        match &properties.timestamp {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_TIMESTAMP_BIT;
                write_u64(&mut buffer, *value);
            }
        }

        match &properties.message_type {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_TYPE_BIT;
                write_short_string(&mut buffer, value);
            }
        }

        match &properties.user_id {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_USER_ID_BIT;
                write_short_string(&mut buffer, value);
            }
        }

        match &properties.app_id {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_APP_ID_BIT;
                write_short_string(&mut buffer, value);
            }
        }

        match &properties.cluster_id {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_CLUSTER_ID_BIT;
                write_short_string(&mut buffer, value);
            }
        }

        let mut result = Vec::new();
        write_u16(&mut result, class_id);
        write_u16(&mut result, 0);
        write_u64(&mut result, size);
        write_u16(&mut result, properties_mask);
        write_bytes(&mut result, &buffer);

        result
    }

    fn serialize_method_frame(method: &AmqpMethod) -> Vec<u8> {
        let mut result = Vec::new();
        match method {
            AmqpMethod::ConnectionStartOk(properties, mechanism, response, locale) => {
                write_u16(&mut result, AMQP_CLASS_CONNECTION);
                write_u16(&mut result, AMQP_METHOD_CONNECTION_START_OK);
                write_table(&mut result, properties);
                write_short_string(&mut result, mechanism);
                write_long_string(&mut result, response);
                write_short_string(&mut result, locale);
            },
            AmqpMethod::ConnectionTuneOk(channel_max, frame_max, heartbeat) => {
                write_u16(&mut result, AMQP_CLASS_CONNECTION);
                write_u16(&mut result, AMQP_METHOD_CONNECTION_TUNE_OK);
                write_u16(&mut result, *channel_max);
                write_u32(&mut result, *frame_max);
                write_u16(&mut result, *heartbeat);
            },
            AmqpMethod::ConnectionOpen(vhost) => {
                write_u16(&mut result, AMQP_CLASS_CONNECTION);
                write_u16(&mut result, AMQP_METHOD_CONNECTION_OPEN);
                write_short_string(&mut result, vhost);
                write_short_string(&mut result, "");    // deprecated but necessary
                write_u8(&mut result, 0);         // deprecated but necessary
            },
            AmqpMethod::ConnectionClose(reply_code, reply_text, class_id, method_id) => {
                write_u16(&mut result, AMQP_CLASS_CONNECTION);
                write_u16(&mut result, AMQP_METHOD_CONNECTION_CLOSE);
                write_u16(&mut result, *reply_code);
                write_short_string(&mut result, reply_text);
                write_u16(&mut result, *class_id);
                write_u16(&mut result, *method_id);
            },
            AmqpMethod::ConnectionCloseOk() => {
                write_u16(&mut result, AMQP_CLASS_CONNECTION);
                write_u16(&mut result, AMQP_METHOD_CONNECTION_CLOSE_OK);
            },
            AmqpMethod::ChannelOpen() => {
                write_u16(&mut result, AMQP_CLASS_CHANNEL);
                write_u16(&mut result, AMQP_METHOD_CHANNEL_OPEN);
                write_short_string(&mut result, "");    // deprecated but necessary
            },
            AmqpMethod::ChannelClose(reply_code, reply_text, class_id, method_id) => {
                write_u16(&mut result, AMQP_CLASS_CHANNEL);
                write_u16(&mut result, AMQP_METHOD_CHANNEL_CLOSE);
                write_u16(&mut result, *reply_code);
                write_short_string(&mut result, reply_text);
                write_u16(&mut result, *class_id);
                write_u16(&mut result, *method_id);
            },
            AmqpMethod::ChannelCloseOk() => {
                write_u16(&mut result, AMQP_CLASS_CHANNEL);
                write_u16(&mut result, AMQP_METHOD_CHANNEL_CLOSE_OK);
            },
            AmqpMethod::ChannelFlow(active) => {
                write_u16(&mut result, AMQP_CLASS_CHANNEL);
                write_u16(&mut result, AMQP_METHOD_CHANNEL_FLOW);
                write_u8(&mut result, (*active) as u8);
            },
            AmqpMethod::ChannelFlowOk(active) => {
                write_u16(&mut result, AMQP_CLASS_CHANNEL);
                write_u16(&mut result, AMQP_METHOD_CHANNEL_FLOW_OK);
                write_u8(&mut result, (*active) as u8);
            },
            AmqpMethod::ExchangeDeclare(name, exchange_type, flags, arguments) => {
                write_u16(&mut result, AMQP_CLASS_EXCHANGE);
                write_u16(&mut result, AMQP_METHOD_EXCHANGE_DECLARE);
                write_u16(&mut result, 0);      // deprecated
                write_short_string(&mut result, &name);
                write_short_string(&mut result, &exchange_type);
                write_u8(&mut result, *flags);
                write_table(&mut result, arguments);
            },
            AmqpMethod::ExchangeDelete(name, flags) => {
                write_u16(&mut result, AMQP_CLASS_EXCHANGE);
                write_u16(&mut result, AMQP_METHOD_EXCHANGE_DELETE);
                write_u16(&mut result, 0);      // deprecated
                write_short_string(&mut result, &name);
                write_u8(&mut result, *flags);
            },
            AmqpMethod::QueueDeclare(name, flags, arguments) => {
                write_u16(&mut result, AMQP_CLASS_QUEUE);
                write_u16(&mut result, AMQP_METHOD_QUEUE_DECLARE);
                write_i16(&mut result, 0);
                write_short_string(&mut result, name);
                write_u8(&mut result, *flags);
                write_table(&mut result, arguments);
            },
            AmqpMethod::QueueBind(name, exchange, routing_key, flags, arguments) => {
                write_u16(&mut result, AMQP_CLASS_QUEUE);
                write_u16(&mut result, AMQP_METHOD_QUEUE_BIND);
                write_u16(&mut result, 0);      // deprecated
                write_short_string(&mut result, name);
                write_short_string(&mut result, exchange);
                write_short_string(&mut result, routing_key);
                write_u8(&mut result, *flags);
                write_table(&mut result, arguments);
            },
            AmqpMethod::QueueUnbind(name, exchange, routing_key, arguments) => {
                write_u16(&mut result, AMQP_CLASS_QUEUE);
                write_u16(&mut result, AMQP_METHOD_QUEUE_UNBIND);
                write_u16(&mut result, 0);      // deprecated
                write_short_string(&mut result, name);
                write_short_string(&mut result, exchange);
                write_short_string(&mut result, routing_key);
                write_table(&mut result, arguments);
            },
            AmqpMethod::QueuePurge(name, flags) => {
                write_u16(&mut result, AMQP_CLASS_QUEUE);
                write_u16(&mut result, AMQP_METHOD_QUEUE_PURGE);
                write_u16(&mut result, 0);      // deprecated
                write_short_string(&mut result, name);
                write_u8(&mut result, *flags);
            },
            AmqpMethod::QueueDelete(name, flags) => {
                write_u16(&mut result, AMQP_CLASS_QUEUE);
                write_u16(&mut result, AMQP_METHOD_QUEUE_DELETE);
                write_u16(&mut result, 0);      // deprecated
                write_short_string(&mut result, name);
                write_u8(&mut result, *flags);
            },
            AmqpMethod::BasicQos(size, count, global) => {
                write_u16(&mut result, AMQP_CLASS_BASIC);
                write_u16(&mut result, AMQP_METHOD_BASIC_QOS);
                write_i32(&mut result, *size);
                write_i16(&mut result, *count);
                write_u8(&mut result, (*global) as u8);
            },
            AmqpMethod::BasicConsume(queue, tag, flags, arguments) => {
                write_u16(&mut result, AMQP_CLASS_BASIC);
                write_u16(&mut result, AMQP_METHOD_BASIC_CONSUME);
                write_u16(&mut result, 0);      // deprecated
                write_short_string(&mut result, queue);
                write_short_string(&mut result, tag);
                write_u8(&mut result, *flags);
                write_table(&mut result, arguments);
            },
            AmqpMethod::BasicCancel(tag, flags) => {
                write_u16(&mut result, AMQP_CLASS_BASIC);
                write_u16(&mut result, AMQP_METHOD_BASIC_CANCEL);
                write_short_string(&mut result, tag);
                write_u8(&mut result, *flags);
            },
            AmqpMethod::BasicPublish(exchange, routing_key, flags) => {
                write_u16(&mut result, AMQP_CLASS_BASIC);
                write_u16(&mut result, AMQP_METHOD_BASIC_PUBLISH);
                write_u16(&mut result, 0);      // deprecated
                write_short_string(&mut result, exchange);
                write_short_string(&mut result, routing_key);
                write_u8(&mut result, *flags);
            },
            AmqpMethod::BasicAck(delivery_tag, multiple) => {
                write_u16(&mut result, AMQP_CLASS_BASIC);
                write_u16(&mut result, AMQP_METHOD_BASIC_ACK);
                write_u64(&mut result, *delivery_tag);
                write_u8(&mut result, (*multiple) as u8);
            },
            AmqpMethod::BasicGet(queue, no_ack) => {
                write_u16(&mut result, AMQP_CLASS_BASIC);
                write_u16(&mut result, AMQP_METHOD_BASIC_GET);
                write_u16(&mut result, 0);
                write_short_string(&mut result, queue);
                write_u8(&mut result, (*no_ack) as u8);
            },
            AmqpMethod::BasicReject(delivery_tag, requeue) => {
                write_u16(&mut result, AMQP_CLASS_BASIC);
                write_u16(&mut result, AMQP_METHOD_BASIC_REJECT);
                write_u64(&mut result, *delivery_tag);
                write_u8(&mut result, (*requeue) as u8);
            },
            _ => panic!("Attempting to write unsupported frame type"),
        }

        result
    }
}

fn write_u8(buffer: &mut Vec<u8>, value: u8) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn write_i8(buffer: &mut Vec<u8>, value: i8) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn write_u16(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn write_i16(buffer: &mut Vec<u8>, value: i16) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn write_u32(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn write_i32(buffer: &mut Vec<u8>, value: i32) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn write_u64(buffer: &mut Vec<u8>, value: u64) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn write_i64(buffer: &mut Vec<u8>, value: i64) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn write_f32(buffer: &mut Vec<u8>, value: f32) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn write_f64(buffer: &mut Vec<u8>, value: f64) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn write_bytes(buffer: &mut Vec<u8>, value: &[u8]) {
    buffer.extend_from_slice(value);
}

fn write_short_string(buffer: &mut Vec<u8>, value: &str) {
    assert!(value.len() < u8::MAX as usize);

    write_u8(buffer, value.len() as u8);
    write_bytes(buffer, value.as_bytes());
}

fn write_long_string(buffer: &mut Vec<u8>, value: &str) {
    write_u32(buffer, value.len() as u32);
    write_bytes(buffer, value.as_bytes());
}

fn write_table(buffer: &mut Vec<u8>, value: &HashMap<String, AmqpData>) {
    let mut tmp = Vec::new();

    value.iter().for_each(|(key, value)| {
        write_short_string(&mut tmp, key);
        write_value(&mut tmp, value);
    });

    write_u32(buffer, tmp.len() as u32);
    write_bytes(buffer, &tmp);
}

fn write_array(buffer: &mut Vec<u8>, value: &Vec<AmqpData>) {
    let mut tmp = Vec::new();

    value.iter().for_each(|value| {
        write_value(&mut tmp, value);
    });

    write_u32(buffer, tmp.len() as u32);
    write_bytes(buffer, &tmp);
}

fn write_value(buffer: &mut Vec<u8>, value: &AmqpData) {
    match &value {
        AmqpData::None => {
            write_u8(buffer, b'V');
        },
        AmqpData::Bool(value) => {
            write_u8(buffer, b't');
            write_u8(buffer, *value as u8);
        },
        AmqpData::I8(value) => {
            write_u8(buffer, b't');
            write_i8(buffer, *value);
        },
        AmqpData::U8(value) => {
            write_u8(buffer, b'B');
            write_u8(buffer, *value);
        },
        AmqpData::I16(value) => {
            write_u8(buffer, b'U');
            write_i16(buffer, *value);
        },
        AmqpData::U16(value) => {
            write_u8(buffer, b'u');
            write_u16(buffer, *value);
        },
        AmqpData::I32(value) => {
            write_u8(buffer, b'I');
            write_i32(buffer, *value);
        },
        AmqpData::U32(value) => {
            write_u8(buffer, b'i');
            write_u32(buffer, *value);
        },
        AmqpData::I64(value) => {
            write_u8(buffer, b'L');
            write_i64(buffer, *value);
        },
        AmqpData::U64(value) => {
            write_u8(buffer, b'l');
            write_u64(buffer, *value);
        },
        AmqpData::Float(value) => {
            write_u8(buffer, b'f');
            write_f32(buffer, *value);
        },
        AmqpData::Double(value) => {
            write_u8(buffer, b'd');
            write_f64(buffer, *value);
        },
        AmqpData::Decimal(scale, value) => {
            write_u8(buffer, b'D');
            write_u8(buffer, *scale);
            write_u32(buffer, *value);
        },
        AmqpData::ShortString(value) => {
            write_u8(buffer, b's');
            write_short_string(buffer, value);
        },
        AmqpData::LongString(value) => {
            write_u8(buffer, b'S');
            write_long_string(buffer, value);
        },
        AmqpData::Timestamp(value) => {
            write_u8(buffer, b'T');
            write_u64(buffer, *value);
        },
        AmqpData::FieldArray(value) => {
            write_u8(buffer, b'A');
            write_array(buffer, value);
        },
        AmqpData::FieldTable(value) => {
            write_u8(buffer, b'F');
            write_table(buffer, value);
        }
    }
}
