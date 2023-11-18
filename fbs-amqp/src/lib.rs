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