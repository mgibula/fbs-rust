use std::os::fd::{OwnedFd, FromRawFd, AsRawFd, IntoRawFd, RawFd};
use super::sigset::SignalSet;
use super::system_error::SystemError;

#[derive(Debug, Clone, Copy)]
pub struct SignalFdFlags {
    flags: i32,
}

impl SignalFdFlags {
    pub fn new() -> Self {
        SignalFdFlags { flags: 0 }
    }

    pub fn close_on_exec(&mut self, value: bool) -> &mut Self {
        if value {
            self.flags |= libc::SFD_CLOEXEC;
        } else {
            self.flags ^= !libc::SFD_CLOEXEC;
        }

        self
    }

    pub fn non_blocking(&mut self, value: bool) -> &mut Self {
        if value {
            self.flags |= libc::SFD_NONBLOCK;
        } else {
            self.flags ^= !libc::SFD_NONBLOCK;
        }

        self
    }

    pub fn flags(&self) -> i32 {
        self.flags
    }
}

pub struct SignalFd {
    fd: OwnedFd,
}

impl SignalFd {
    pub fn new(mask: &SignalSet, flags: i32) -> Result<Self, SystemError> {
        unsafe {
            let fd = libc::signalfd(-1, mask.as_ptr(), flags);
            match fd {
                -1 => Err(SystemError::new_from_errno()),
                fd => Ok(Self { fd: OwnedFd::from_raw_fd(fd) })
            }
        }
    }

    pub fn non_blocking(&self) -> bool {
        unsafe {
            let flags = libc::fcntl(self.fd.as_raw_fd(), libc::F_GETFL);
            (flags & libc::O_NONBLOCK) > 0
        }
    }

    pub fn close_on_exec(&self) -> bool {
        unsafe {
            let flags = libc::fcntl(self.fd.as_raw_fd(), libc::F_GETFD);
            (flags & libc::FD_CLOEXEC) > 0
        }
    }

    pub fn set_signal_mask(&mut self, mask: &SignalSet) -> Result<(), SystemError> {
        let mut flags = SignalFdFlags::new();
        flags.non_blocking(self.non_blocking());
        flags.close_on_exec(self.close_on_exec());

        unsafe {
            let fd = libc::signalfd(-1, mask.as_ptr(), flags.flags());
            match fd {
                -1 => Err(SystemError::new_from_errno()),
                _ => Ok(())
            }
        }
    }
}

impl AsRawFd for SignalFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl IntoRawFd for SignalFd {
    fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sigset::SignalSet;

    #[test]
    fn signalfd_create() {
        let mask = SignalSet::full();
        let signalfd = SignalFd::new(&mask, SignalFdFlags::new().flags()).unwrap();

        assert_eq!(signalfd.non_blocking(), false);
        assert_eq!(signalfd.close_on_exec(), false);
    }

    #[test]
    fn signalfd_create2() {
        let mask = SignalSet::full();
        let signalfd = SignalFd::new(&mask, SignalFdFlags::new().close_on_exec(true).non_blocking(true).flags()).unwrap();

        assert_eq!(signalfd.non_blocking(), true);
        assert_eq!(signalfd.close_on_exec(), true);
    }

    #[test]
    fn signalfd_change() {
        let mask = SignalSet::full();
        let mask2 = SignalSet::empty();
        let mut signalfd = SignalFd::new(&mask, SignalFdFlags::new().close_on_exec(true).non_blocking(true).flags()).unwrap();

        assert_eq!(signalfd.non_blocking(), true);
        assert_eq!(signalfd.close_on_exec(), true);

        let err = signalfd.set_signal_mask(&mask2);
        assert_eq!(err.is_ok(), true);
        assert_eq!(signalfd.non_blocking(), true);
        assert_eq!(signalfd.close_on_exec(), true);
    }
}
