pub(crate) mod defines;
pub(crate) mod frame;
pub(crate) mod frame_reader;
pub(crate) mod frame_writer;
pub mod connection;


#[derive(Debug, Default, Clone, Copy)]
pub struct AmqpExchangeFlags {
    flags: u8,
}

impl AmqpExchangeFlags {
    pub fn new() -> Self {
        Self { flags: 0 }
    }

    pub fn durable(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 1;
        } else {
            self.flags &= !(1 << 1);
        }

        self
    }

    pub fn passive(mut self, value: bool) -> Self {
        if value {
            self.flags |= 1 << 0;
        } else {
            self.flags &= !(1 << 0);
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
