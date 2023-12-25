use std::collections::HashMap;
use std::rc::Rc;

use super::{AmqpData, AmqpBasicProperties};
use super::connection::WriteBufferManager;
use super::frame::{AmqpFrame, AmqpFramePayload, AmqpMethod};
use super::defines::*;

pub(super) struct FrameWriter;

impl FrameWriter {
    pub(super) fn write_frame(frame: AmqpFrame, buffers: &WriteBufferManager) -> Vec<u8> {
        let mut result = buffers.get_buffer();
        match &frame.payload {
            AmqpFramePayload::Method(_)         => write_u8(&mut result, AMQP_FRAME_TYPE_METHOD),
            AmqpFramePayload::Header(_, _, _)   => write_u8(&mut result, AMQP_FRAME_TYPE_HEADER),
            AmqpFramePayload::Content(_)        => write_u8(&mut result, AMQP_FRAME_TYPE_CONTENT),
            AmqpFramePayload::Heartbeat()       => write_u8(&mut result, AMQP_FRAME_TYPE_HEARTBEAT),
        }

        write_u16(&mut result, frame.channel);

        let size_offset = result.len();
        write_u32(&mut result, 0);   // placeholde for frame size

        FrameWriter::serialize_frame(frame, &mut result, buffers);

        // fill the real size
        let payload_size = (result.len() - size_offset - 4) as u32; // -4 for frame size placeholder
        result[size_offset .. size_offset + 4].copy_from_slice(&payload_size.to_be_bytes());

        // write frame trailing
        write_u8(&mut result, b'\xCE');

        result
    }

    fn serialize_frame(frame: AmqpFrame, target: &mut Vec<u8>, buffers: &WriteBufferManager) {
        match frame.payload {
            AmqpFramePayload::Method(method) => FrameWriter::serialize_method_frame(target, &method),
            AmqpFramePayload::Header(class, size, properties) => FrameWriter::serialize_header_frame(target, class, size, &properties),
            AmqpFramePayload::Content(data) => {
                write_bytes(target, &data);
                buffers.put_buffer(data);
            },
            AmqpFramePayload::Heartbeat() => (),
        }
    }

    fn serialize_header_frame(target: &mut Vec<u8>, class_id: u16, size: u64, properties: &AmqpBasicProperties) {
        write_u16(target, class_id);
        write_u16(target, 0);
        write_u64(target, size);

        // properties_mask - will be filled later
        let mask_offset = target.len();
        write_u16(target, 0);

        let mut properties_mask: u16 = 0;
        match &properties.content_type {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_CONTENT_TYPE_BIT;
                write_short_string(target, value);
            }
        }

        match &properties.content_encoding {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_CONTENT_ENCODING_BIT;
                write_short_string(target, value);
            }
        }

        match &properties.headers {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_HEADERS_BIT;
                write_table(target, value);
            }
        }

        match &properties.delivery_mode {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_DELIVERY_MNODE_BIT;
                write_u8(target, *value);
            }
        }

        match &properties.priority {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_PRIORITY_BIT;
                write_u8(target, *value);
            }
        }

        match &properties.correlation_id {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_CORRELATION_ID_BIT;
                write_short_string(target, value);
            }
        }

        match &properties.reply_to {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_REPLY_TO_BIT;
                write_short_string(target, value);
            }
        }

        match &properties.expiration {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_EXPIRATION_BIT;
                write_short_string(target, value);
            }
        }

        match &properties.message_id {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_MESSAGE_ID_BIT;
                write_short_string(target, value);
            }
        }

        match &properties.timestamp {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_TIMESTAMP_BIT;
                write_u64(target, *value);
            }
        }

        match &properties.message_type {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_TYPE_BIT;
                write_short_string(target, value);
            }
        }

        match &properties.user_id {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_USER_ID_BIT;
                write_short_string(target, value);
            }
        }

        match &properties.app_id {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_APP_ID_BIT;
                write_short_string(target, value);
            }
        }

        match &properties.cluster_id {
            None => (),
            Some(value) => {
                properties_mask |= 1 << AMQP_BASIC_PROPERTY_CLUSTER_ID_BIT;
                write_short_string(target, value);
            }
        }

        // fill the properties_mask
        target[mask_offset .. mask_offset + 2].copy_from_slice(&properties_mask.to_be_bytes());
    }

    fn serialize_method_frame(target: &mut Vec<u8>, method: &AmqpMethod) {
        match method {
            AmqpMethod::ConnectionStartOk(properties, mechanism, response, locale) => {
                write_u16(target, AMQP_CLASS_CONNECTION);
                write_u16(target, AMQP_METHOD_CONNECTION_START_OK);
                write_table(target, properties);
                write_short_string(target, mechanism);
                write_long_string(target, response);
                write_short_string(target, locale);
            },
            AmqpMethod::ConnectionTuneOk(channel_max, frame_max, heartbeat) => {
                write_u16(target, AMQP_CLASS_CONNECTION);
                write_u16(target, AMQP_METHOD_CONNECTION_TUNE_OK);
                write_u16(target, *channel_max);
                write_u32(target, *frame_max);
                write_u16(target, *heartbeat);
            },
            AmqpMethod::ConnectionOpen(vhost) => {
                write_u16(target, AMQP_CLASS_CONNECTION);
                write_u16(target, AMQP_METHOD_CONNECTION_OPEN);
                write_short_string(target, vhost);
                write_short_string(target, "");    // deprecated but necessary
                write_u8(target, 0);         // deprecated but necessary
            },
            AmqpMethod::ConnectionClose(reply_code, reply_text, class_id, method_id) => {
                write_u16(target, AMQP_CLASS_CONNECTION);
                write_u16(target, AMQP_METHOD_CONNECTION_CLOSE);
                write_u16(target, *reply_code);
                write_short_string(target, reply_text);
                write_u16(target, *class_id);
                write_u16(target, *method_id);
            },
            AmqpMethod::ConnectionCloseOk() => {
                write_u16(target, AMQP_CLASS_CONNECTION);
                write_u16(target, AMQP_METHOD_CONNECTION_CLOSE_OK);
            },
            AmqpMethod::ChannelOpen() => {
                write_u16(target, AMQP_CLASS_CHANNEL);
                write_u16(target, AMQP_METHOD_CHANNEL_OPEN);
                write_short_string(target, "");    // deprecated but necessary
            },
            AmqpMethod::ChannelClose(reply_code, reply_text, class_id, method_id) => {
                write_u16(target, AMQP_CLASS_CHANNEL);
                write_u16(target, AMQP_METHOD_CHANNEL_CLOSE);
                write_u16(target, *reply_code);
                write_short_string(target, reply_text);
                write_u16(target, *class_id);
                write_u16(target, *method_id);
            },
            AmqpMethod::ChannelCloseOk() => {
                write_u16(target, AMQP_CLASS_CHANNEL);
                write_u16(target, AMQP_METHOD_CHANNEL_CLOSE_OK);
            },
            AmqpMethod::ChannelFlow(active) => {
                write_u16(target, AMQP_CLASS_CHANNEL);
                write_u16(target, AMQP_METHOD_CHANNEL_FLOW);
                write_u8(target, (*active) as u8);
            },
            AmqpMethod::ChannelFlowOk(active) => {
                write_u16(target, AMQP_CLASS_CHANNEL);
                write_u16(target, AMQP_METHOD_CHANNEL_FLOW_OK);
                write_u8(target, (*active) as u8);
            },
            AmqpMethod::ExchangeDeclare(name, exchange_type, flags, arguments) => {
                write_u16(target, AMQP_CLASS_EXCHANGE);
                write_u16(target, AMQP_METHOD_EXCHANGE_DECLARE);
                write_u16(target, 0);      // deprecated
                write_short_string(target, &name);
                write_short_string(target, &exchange_type);
                write_u8(target, *flags);
                write_table(target, arguments);
            },
            AmqpMethod::ExchangeDelete(name, flags) => {
                write_u16(target, AMQP_CLASS_EXCHANGE);
                write_u16(target, AMQP_METHOD_EXCHANGE_DELETE);
                write_u16(target, 0);      // deprecated
                write_short_string(target, &name);
                write_u8(target, *flags);
            },
            AmqpMethod::QueueDeclare(name, flags, arguments) => {
                write_u16(target, AMQP_CLASS_QUEUE);
                write_u16(target, AMQP_METHOD_QUEUE_DECLARE);
                write_i16(target, 0);
                write_short_string(target, name);
                write_u8(target, *flags);
                write_table(target, arguments);
            },
            AmqpMethod::QueueBind(name, exchange, routing_key, flags, arguments) => {
                write_u16(target, AMQP_CLASS_QUEUE);
                write_u16(target, AMQP_METHOD_QUEUE_BIND);
                write_u16(target, 0);      // deprecated
                write_short_string(target, name);
                write_short_string(target, exchange);
                write_short_string(target, routing_key);
                write_u8(target, *flags);
                write_table(target, arguments);
            },
            AmqpMethod::QueueUnbind(name, exchange, routing_key, arguments) => {
                write_u16(target, AMQP_CLASS_QUEUE);
                write_u16(target, AMQP_METHOD_QUEUE_UNBIND);
                write_u16(target, 0);      // deprecated
                write_short_string(target, name);
                write_short_string(target, exchange);
                write_short_string(target, routing_key);
                write_table(target, arguments);
            },
            AmqpMethod::QueuePurge(name, flags) => {
                write_u16(target, AMQP_CLASS_QUEUE);
                write_u16(target, AMQP_METHOD_QUEUE_PURGE);
                write_u16(target, 0);      // deprecated
                write_short_string(target, name);
                write_u8(target, *flags);
            },
            AmqpMethod::QueueDelete(name, flags) => {
                write_u16(target, AMQP_CLASS_QUEUE);
                write_u16(target, AMQP_METHOD_QUEUE_DELETE);
                write_u16(target, 0);      // deprecated
                write_short_string(target, name);
                write_u8(target, *flags);
            },
            AmqpMethod::BasicQos(size, count, global) => {
                write_u16(target, AMQP_CLASS_BASIC);
                write_u16(target, AMQP_METHOD_BASIC_QOS);
                write_i32(target, *size);
                write_i16(target, *count);
                write_u8(target, (*global) as u8);
            },
            AmqpMethod::BasicConsume(queue, tag, flags, arguments) => {
                write_u16(target, AMQP_CLASS_BASIC);
                write_u16(target, AMQP_METHOD_BASIC_CONSUME);
                write_u16(target, 0);      // deprecated
                write_short_string(target, queue);
                write_short_string(target, tag);
                write_u8(target, *flags);
                write_table(target, arguments);
            },
            AmqpMethod::BasicCancel(tag, flags) => {
                write_u16(target, AMQP_CLASS_BASIC);
                write_u16(target, AMQP_METHOD_BASIC_CANCEL);
                write_short_string(target, tag);
                write_u8(target, *flags);
            },
            AmqpMethod::BasicPublish(exchange, routing_key, flags) => {
                write_u16(target, AMQP_CLASS_BASIC);
                write_u16(target, AMQP_METHOD_BASIC_PUBLISH);
                write_u16(target, 0);      // deprecated
                write_short_string(target, exchange);
                write_short_string(target, routing_key);
                write_u8(target, *flags);
            },
            AmqpMethod::BasicAck(delivery_tag, multiple) => {
                write_u16(target, AMQP_CLASS_BASIC);
                write_u16(target, AMQP_METHOD_BASIC_ACK);
                write_u64(target, *delivery_tag);
                write_u8(target, (*multiple) as u8);
            },
            AmqpMethod::BasicGet(queue, no_ack) => {
                write_u16(target, AMQP_CLASS_BASIC);
                write_u16(target, AMQP_METHOD_BASIC_GET);
                write_u16(target, 0);
                write_short_string(target, queue);
                write_u8(target, (*no_ack) as u8);
            },
            AmqpMethod::BasicReject(delivery_tag, requeue) => {
                write_u16(target, AMQP_CLASS_BASIC);
                write_u16(target, AMQP_METHOD_BASIC_REJECT);
                write_u64(target, *delivery_tag);
                write_u8(target, (*requeue) as u8);
            },
            AmqpMethod::BasicRecover(requeue) => {
                write_u16(target, AMQP_CLASS_BASIC);
                write_u16(target, AMQP_METHOD_BASIC_RECOVER);
                write_u8(target, (*requeue) as u8);
            },
            AmqpMethod::BasicNack(delivery_tag, flags) => {
                write_u16(target, AMQP_CLASS_BASIC);
                write_u16(target, AMQP_METHOD_BASIC_NACK);
                write_u64(target, *delivery_tag);
                write_u8(target, *flags);
            },
            AmqpMethod::ConfirmSelect(no_wait) => {
                write_u16(target, AMQP_CLASS_CONFIRM);
                write_u16(target, AMQP_METHOD_CONFIRM_SELECT);
                write_u8(target, (*no_wait) as u8);
            },
            _ => panic!("Attempting to write unsupported frame type"),
        }
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
    // size placeholder, to be filled later
    let size_offset = buffer.len();
    write_u32(buffer, 0);

    value.iter().for_each(|(key, value)| {
        write_short_string(buffer, key);
        write_value(buffer, value);
    });

    // fill the real size
    let payload_size = (buffer.len() - size_offset - 4) as u32; // -4 for frame size placeholder
    buffer[size_offset .. size_offset + 4].copy_from_slice(&payload_size.to_be_bytes());
}

fn write_array(buffer: &mut Vec<u8>, value: &Vec<AmqpData>) {
    // size placeholder, to be filled later
    let size_offset = buffer.len();
    write_u32(buffer, 0);

    value.iter().for_each(|value| {
        write_value(buffer, value);
    });

    // fill the real size
    let payload_size = (buffer.len() - size_offset - 4) as u32; // -4 for frame size placeholder
    buffer[size_offset .. size_offset + 4].copy_from_slice(&payload_size.to_be_bytes());
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
