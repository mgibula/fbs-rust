
#[repr(i32)]
pub enum SocketDomain {
    Inet    = libc::AF_INET,
}

#[repr(i32)]
pub enum SocketType {
    Stream  = libc::SOCK_STREAM,
}

#[derive(Debug, Clone, Copy)]
pub struct SocketOptions {
    flags: i32,
}

impl SocketOptions {
    pub fn new() -> Self {
        SocketOptions { flags: 0 }
    }

    pub fn close_on_exec(&mut self, value: bool) -> &mut Self {
        if value {
            self.flags |= libc::SOCK_CLOEXEC;
        } else {
            self.flags ^= !libc::SOCK_CLOEXEC;
        }

        self
    }

    pub fn non_blocking(&mut self, value: bool) -> &mut Self {
        if value {
            self.flags |= libc::SOCK_NONBLOCK;
        } else {
            self.flags ^= !libc::SOCK_NONBLOCK;
        }

        self
    }

    pub fn flags(&self) -> i32 {
        self.flags
    }
}
