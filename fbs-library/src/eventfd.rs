use std::os::fd::{OwnedFd, FromRawFd, AsRawFd, IntoRawFd, RawFd};
use super::system_error::SystemError;

#[derive(Debug, Clone, Copy)]
pub struct EventFdFlags {
    flags: i32,
}

impl EventFdFlags {
    pub fn new() -> Self {
        EventFdFlags { flags: 0 }
    }

    pub fn close_on_exec(mut self, value: bool) -> Self {
        if value {
            self.flags |= libc::EFD_CLOEXEC;
        } else {
            self.flags &= !libc::EFD_CLOEXEC;
        }

        self
    }

    pub fn non_blocking(mut self, value: bool) -> Self {
        if value {
            self.flags |= libc::EFD_NONBLOCK;
        } else {
            self.flags &= !libc::EFD_NONBLOCK;
        }

        self
    }

    pub fn semaphore(mut self, value: bool) -> Self {
        if value {
            self.flags |= libc::EFD_SEMAPHORE;
        } else {
            self.flags &= !libc::EFD_SEMAPHORE;
        }

        self
    }

    pub fn flags(self) -> i32 {
        self.flags
    }
}

pub struct EventFd {
    fd: OwnedFd,
}

impl EventFd {
    pub fn new(initial: u32, flags: EventFdFlags) -> Result<Self, SystemError> {
        unsafe {
            let fd = libc::eventfd(initial, flags.flags());
            match fd {
                -1      => Err(SystemError::new_from_errno()),
                fd => Ok(Self { fd: OwnedFd::from_raw_fd(fd) })
            }
        }
    }

    pub fn write(&self, value: u64) -> bool {
        unsafe {
            let data = value.to_ne_bytes();
            let result = libc::write(self.fd.as_raw_fd(), data.as_ptr() as *const libc::c_void, data.len());
            match result as i32 {
                0 => true,
                libc::EAGAIN => false,
                _ => panic!("Unknown write result for eventdf = {}", result),
            }
        }
    }
}

impl AsRawFd for EventFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl IntoRawFd for EventFd {
    fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}
