#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PollMask {
    mask: i16
}

impl PollMask {
    pub fn empty(&self) -> bool {
        self.mask == 0
    }

    pub fn read(&mut self, value: bool) -> Self {
        if value {
            self.mask |= libc::POLLIN;
        } else {
            self.mask ^= !libc::POLLIN;
        }

        *self
    }

    pub fn write(&mut self, value: bool) -> Self {
        if value {
            self.mask |= libc::POLLOUT;
        } else {
            self.mask ^= !libc::POLLOUT;
        }

        *self
    }
}

impl Into<i16> for PollMask {
    fn into(self) -> i16 {
        self.mask
    }
}

impl Into<i32> for PollMask {
    fn into(self) -> i32 {
        self.mask as i32
    }
}

impl Into<u16> for PollMask {
    fn into(self) -> u16 {
        self.mask as u16
    }
}

impl Into<u32> for PollMask {
    fn into(self) -> u32 {
        self.mask as u32
    }
}