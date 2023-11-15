use std::collections::HashMap;

use crate::frame::AmqpData;

use super::frame::{AmqpFrame, AmqpFramePayload, AmqpMethod};
use super::defines::*;

pub struct FrameWriter;

impl FrameWriter {
    pub fn write_frame(frame: &AmqpFrame) -> Vec<u8> {
        let mut result = Vec::new();

        match &frame.payload {
            AmqpFramePayload::Method(_) => write_u8(&mut result, AMQP_FRAME_TYPE_METHOD),
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
            _ => panic!("Attempting to write unsupported frame type"),
        }
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
