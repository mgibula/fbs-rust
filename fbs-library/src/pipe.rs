use std::os::fd::{OwnedFd, FromRawFd};
use super::system_error::SystemError;

#[derive(Debug, Default, Clone, Copy)]
pub struct PipeFlags {
    flags: i32,
}

impl PipeFlags {
    pub fn close_on_exec(&mut self, value: bool) -> Self {
        if value {
            self.flags |= libc::O_CLOEXEC;
        } else {
            self.flags ^= !libc::O_CLOEXEC;
        }

        *self
    }

    pub fn direct(&mut self, value: bool) -> Self {
        if value {
            self.flags |= libc::O_DIRECT;
        } else {
            self.flags ^= !libc::O_DIRECT;
        }

        *self
    }

    pub fn non_blocking(&mut self, value: bool) -> Self {
        if value {
            self.flags |= libc::O_NONBLOCK;
        } else {
            self.flags ^= !libc::O_NONBLOCK;
        }

        *self
    }
}

impl Into<libc::c_int> for PipeFlags {
    fn into(self) -> libc::c_int {
        self.flags as libc::c_int
    }
}

pub fn pipe(flags: PipeFlags) -> Result<(OwnedFd, OwnedFd), SystemError> {
    let mut fd = [-1, 2];

    unsafe {
        let error = libc::pipe2(fd.as_mut_ptr(), flags.into());
        match error {
            0 => Ok((OwnedFd::from_raw_fd(fd[0]), OwnedFd::from_raw_fd(fd[1]))),
            _ => Err(SystemError::new_from_errno())
        }
    }
}

pub fn pipe_atomic_write_size() -> u32 {
    libc::PIPE_BUF as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_create() {
        let pipes = pipe(PipeFlags::default());
        assert_eq!(pipes.is_ok(), true);
    }
}
