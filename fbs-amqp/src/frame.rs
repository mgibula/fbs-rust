use super::defines::*;

pub(crate) struct AmqpProtocolHeader { }

#[derive(Debug, Clone)]
pub(crate) struct AmqpFrame {
    header: FrameHeader,
    payload: FramePayload,
    footer: FrameEnd,
}

impl AmqpFrame {
    pub fn new(header: FrameHeader, payload: FramePayload, footer: FrameEnd) -> Self {
        Self { header, payload, footer }
    }
}

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