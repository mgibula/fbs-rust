#![allow(nonstandard_style)]
#![allow(improper_ctypes)]
use core::panic;
use std::mem;
use std::ops::Drop;
use std::ptr;

use libc::statx;
use libc::__u8;
use libc::__u16;
use libc::__u32;
use libc::__u64;
use libc::__s32;
use libc::__s64;
use libc::mode_t;
use libc::epoll_event;
use libc::open_how;
use libc::msghdr;
use libc::cmsghdr;
use libc::socklen_t;
use libc::sockaddr;
use libc::off_t;
use libc::iovec;
use libc::sigset_t;
use libc::cpu_set_t;
use thiserror::Error;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub struct IoUringParams {
    sq_entries: u32,
    cq_entries: u32,
}

pub struct IoUring {
    ring: io_uring,
    created: bool,
}

pub struct IoUringSQE {
    sqe: *mut io_uring_sqe,
}

pub struct IoUringCQE {
    cqe: *mut io_uring_cqe,
}

#[derive(Error, Debug)]
pub enum IoUringCreateError
{
    #[error("invalid arguments specified")]
    InvalidArguments,
    #[error("file descriptors limit reached")]
    DescriptorLimit,
    #[error("IORING_SETUP_SQPOLL specified, but privileges are insufficient")]
    InsufficientPrivileges,
}

#[derive(Error, Debug)]
pub enum IoUringError {
    #[error("kernel asked to retry request")]
    TryAgain,
    #[error("invalid arguments specified")]
    InvalidArguments,
}

impl Drop for IoUring {
    fn drop(&mut self) {
        if self.created {
            unsafe { io_uring_queue_exit(&mut self.ring) }
        }
    }
}

impl IoUring {
    pub fn new(params: IoUringParams) -> Result<Self, IoUringCreateError> {
        unsafe {
            let mut result = IoUring {
                ring: io_uring {
                    _bindgen_opaque_blob: mem::zeroed(),
                },
                created: false,
            };

            let mut raw_params: io_uring_params = mem::zeroed();
            raw_params.cq_entries = params.cq_entries;
            raw_params.flags = IORING_SETUP_CQSIZE | IORING_SETUP_CLAMP;

            let errno = io_uring_queue_init_params(params.sq_entries, &mut result.ring, &mut raw_params);
            match -errno {
                0 => {},
                libc::EFAULT => panic!("io_uring_queue_init_params EFAULT"),
                libc::ENOMEM => panic!("io_uring_queue_init_params ENOMEM"),
                libc::EINVAL => return Err(IoUringCreateError::InvalidArguments),
                libc::EMFILE => return Err(IoUringCreateError::DescriptorLimit),
                libc::ENFILE => return Err(IoUringCreateError::DescriptorLimit),
                libc::EPERM => return Err(IoUringCreateError::InsufficientPrivileges),
                _ => panic!("Unexpected error: {}", errno),
            }

            result.created = true;
            Ok(result)
        }
    }

    pub fn sq_space_left(&self) -> u32 {
        unsafe { io_uring_sq_space_left(&self.ring) }
    }

    pub fn submit(&mut self) -> i32 {
        unsafe { io_uring_submit(&mut self.ring) }
    }

    pub fn get_sqe(&mut self) -> IoUringSQE {
        unsafe {
            IoUringSQE {
                sqe: io_uring_get_sqe(&mut self.ring),
            }
        }
    }

    pub fn peek_cqe(&mut self) -> Option<IoUringCQE> {
        unsafe {
            let mut ptr: *mut io_uring_cqe = ptr::null_mut();
            let result = io_uring_peek_cqe(&mut self.ring, &mut ptr);
            match result {
                0 => Some(IoUringCQE { cqe: ptr }),
                _ => None,
            }
        }
    }

    pub fn wait_cqe(&mut self) -> Result<IoUringCQE, IoUringError> {
        unsafe {
            let mut ptr: *mut io_uring_cqe = ptr::null_mut();
            let errno = io_uring_wait_cqe(&mut self.ring, &mut ptr);
            match -errno {
                0 => Ok(IoUringCQE { cqe: ptr }),
                libc::EAGAIN => return Err(IoUringError::TryAgain),
                libc::EBUSY => return Err(IoUringError::TryAgain),
                libc::EINTR => return Err(IoUringError::TryAgain),
                libc::EBADF => return Err(IoUringError::InvalidArguments),
                libc::EBADFD => return Err(IoUringError::InvalidArguments),
                libc::EINVAL => return Err(IoUringError::InvalidArguments),
                libc::EFAULT => return Err(IoUringError::InvalidArguments),
                libc::EBADR => panic!("CQE were dropped by kernel due to low memory condition"),
                libc::ENXIO => return Err(IoUringError::InvalidArguments),
                libc::EOPNOTSUPP => return Err(IoUringError::InvalidArguments),
                _ => panic!("Unexpected error: {}", errno),
            }
        }
    }

    pub fn cqe_seen(&mut self, entry: IoUringCQE) {
        unsafe {
            io_uring_cqe_seen(&mut self.ring, entry.cqe)
        }
    }
}

impl IoUringSQE {
    pub fn set_data64(&mut self, value: u64) {
        unsafe {
            io_uring_sqe_set_data64(self.sqe, value)
        }
    }

    pub fn set_flags(&mut self, flags: u32) {
        unsafe {
            io_uring_sqe_set_flags(self.sqe, flags)
        }
    }
}

impl IoUringCQE {
    pub fn get_data64(&self) -> u64 {
        unsafe {
            io_uring_cqe_get_data64(self.cqe)
        }
    }

    pub fn get_result(&self) -> i32 {
        unsafe {
            (*self.cqe).res
        }
    }

    pub fn get_flags(&self) -> u32 {
        unsafe {
            (*self.cqe).flags
        }
    }
}

