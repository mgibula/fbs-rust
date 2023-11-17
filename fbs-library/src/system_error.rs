
use std::error::Error;
use std::fmt::{Formatter, Display};

#[derive(Debug, Clone, Copy)]
pub struct SystemError (i32);

impl Display for SystemError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for SystemError {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for SystemError { }

impl Error for SystemError { }

impl SystemError {
    pub fn new(code: i32) -> Self {
        Self { 0: code }
    }

    pub fn new_from_errno() -> Self {
        let error = std::io::Error::last_os_error();
        match error.raw_os_error() {
            None => Self { 0: 0 },
            Some(code) => Self { 0: code }
        }
    }

    #[inline]
    pub fn cancelled(&self) -> bool {
        match self.0 {
            libc::ECANCELED => true,
            _ => false,
        }
    }

    #[inline]
    pub fn timed_out(&self) -> bool {
        match self.0 {
            libc::ETIMEDOUT => true,
            _ => false,
        }
    }

    pub fn errno(&self) -> i32 {
        self.0
    }
}