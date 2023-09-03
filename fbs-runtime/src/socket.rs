use std::os::fd::{OwnedFd, FromRawFd};
use thiserror::Error;

#[repr(i32)]
pub enum SocketDomain {
    Inet    = libc::AF_INET,
}

#[repr(i32)]
pub enum SocketType {
    Stream  = libc::SOCK_STREAM,
}

#[derive(Debug, Clone, Copy)]
pub struct SocketFlags {
    flags: i32,
}

impl SocketFlags {
    pub fn new() -> Self {
        SocketFlags { flags: 0 }
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

pub enum SocketOptions {
    ReuseAddr(bool),
}

pub struct Socket {
    fd: OwnedFd,
}

impl Socket {
    pub fn new(domain: SocketDomain, socket_type: SocketType, options: i32) -> Self {
        unsafe {
            let sockfd = libc::socket(domain as libc::c_int, socket_type as libc::c_int | options, 0);

            Self {
                fd: OwnedFd::from_raw_fd(sockfd)
            }
        }
    }

}

pub struct Listener {
    sock: Socket,
}
