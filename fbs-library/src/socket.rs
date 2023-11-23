use std::mem::size_of;
use std::os::fd::{OwnedFd, FromRawFd, AsRawFd, RawFd, IntoRawFd};
use std::io::Error;

use super::socket_address::SocketIpAddress;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SocketError {
    #[error("System error")]
    SystemError(#[from] std::io::Error),
}


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

#[derive(Debug)]
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

    pub fn listen(&self, address: &SocketIpAddress, backlog: i32) -> Result<(), SocketError> {
        let binary = address.to_binary();
        unsafe {
            let error = libc::bind(self.fd.as_raw_fd(), binary.sockaddr_ptr(), binary.length() as u32);
            if error != 0 {
                return Err(SocketError::SystemError(Error::last_os_error()));
            }

            let error = libc::listen(self.fd.as_raw_fd(), backlog);
            if error != 0 {
                return Err(SocketError::SystemError(Error::last_os_error()));
            }

            Ok(())
        }
    }

    pub fn set_option(&self, option: SocketOptions) -> Result<(), SocketError> {
        match option {
            SocketOptions::ReuseAddr(value) => {
                unsafe {
                    let value: libc::c_int = value as libc::c_int;
                    let error = libc::setsockopt(self.as_raw_fd(), libc::SOL_SOCKET, libc::SO_REUSEADDR, &value as *const i32 as *const libc::c_void, size_of::<libc::c_int>() as u32);
                    if error != 0 {
                        return Err(SocketError::SystemError(Error::last_os_error()));
                    }
                }
            }
        }

        Ok(())
    }

    pub fn shutdown(&self, read_end: bool, write_end: bool) -> Result<(), SocketError> {
        unsafe {
            let mut how = 0;
            how = match (read_end, write_end) {
                (true, false) => libc::SHUT_RD,
                (false, true) => libc::SHUT_WR,
                (true, true) => libc::SHUT_RDWR,
                (_, _) => return Ok(())
            };

            libc::shutdown(self.as_raw_fd(), how);
        }

        Ok(())
    }
}

impl AsRawFd for Socket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl IntoRawFd for Socket {
    fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}

impl FromRawFd for Socket {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd: OwnedFd::from_raw_fd(fd) }
    }
}