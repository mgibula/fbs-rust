
use std::error::Error;
use std::fmt::{Formatter, Display};

#[derive(Debug)]
pub struct SystemError (std::io::Error);

impl Display for SystemError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for SystemError {
    fn eq(&self, other: &Self) -> bool {
        self.0.kind() == other.0.kind()
    }
}

impl Eq for SystemError { }

impl Error for SystemError {
}

impl SystemError {
    pub fn new(code: i32) -> Self {
        Self{0: std::io::Error::from_raw_os_error(code)}
    }

    #[inline]
    pub fn cancelled(&self) -> bool {
        match self.0.raw_os_error() {
            Some(libc::ECANCELED) => true,
            _ => false,
        }
    }

    #[inline]
    pub fn timed_out(&self) -> bool {
        match self.0.raw_os_error() {
            Some(libc::ETIMEDOUT) => true,
            _ => false,
        }
    }

    pub fn errno(&self) -> i32 {
        match self.0.raw_os_error() {
            Some(value) => value,
            None => 0,
        }
    }

    pub fn kind(&self) -> std::io::ErrorKind {
        self.0.kind()
    }
}