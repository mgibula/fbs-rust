use core::panic;
use std::mem;
use std::ops::Drop;
use std::ptr;

use liburing_sys::*;
use thiserror::Error;
use fbs_library::system_error::SystemError;

#[derive(Debug, Clone, Copy)]
pub struct IoUringParams {
    pub sq_entries: u32,
    pub cq_entries: u32,
}

pub struct IoUring {
    ring: io_uring,
    created: bool,
    probe: *mut io_uring_probe,
}

#[derive(Debug, Clone, Copy)]
pub struct IoUringSQEPtr {
    pub ptr: *mut io_uring_sqe,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IoUringCQE {
    pub result: i32,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct IoUringCQEPtr {
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
    #[error("sqe submit error")]
    SubmitError(SystemError),
    #[error("cqe wait error")]
    WaitError(SystemError),
}

impl Drop for IoUring {
    fn drop(&mut self) {
        if self.created {
            unsafe {
                io_uring_free_probe(self.probe);
                io_uring_queue_exit(&mut self.ring);
            }
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
                probe: std::ptr::null_mut(),
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

            result.probe = io_uring_get_probe_ring(&mut result.ring);
            result.created = true;

            Ok(result)
        }
    }

    pub fn is_op_supported(&self, opcode: u32) -> bool {
        unsafe { io_uring_opcode_supported(self.probe, opcode as libc::c_int) > 0 }
    }

    pub fn sq_space_left(&self) -> u32 {
        unsafe { io_uring_sq_space_left(&self.ring) }
    }

    pub fn submit(&mut self) -> Result<i32, IoUringError> {
        unsafe {
            let result = io_uring_submit(&mut self.ring);
            if result >= 0 {
                return Ok(result);
            } else {
                return Err(IoUringError::SubmitError(SystemError::new(-result)));
            }
        }
    }

    pub fn get_sqe(&mut self) -> Option<IoUringSQEPtr> {
        unsafe {
            let ptr = io_uring_get_sqe(&mut self.ring);
            if !ptr.is_null() {
                return Some(IoUringSQEPtr { ptr });
            }

            return None;
        }
    }

    pub fn peek_cqe(&mut self) -> Option<IoUringCQEPtr> {
        unsafe {
            let mut ptr: *mut io_uring_cqe = ptr::null_mut();
            let result = io_uring_peek_cqe(&mut self.ring, &mut ptr);
            match result {
                0 => Some(IoUringCQEPtr { cqe: ptr }),
                _ => None,
            }
        }
    }

    pub fn wait_cqe(&mut self) -> Result<IoUringCQEPtr, IoUringError> {
        unsafe {
            let mut ptr: *mut io_uring_cqe = ptr::null_mut();
            let errno = io_uring_wait_cqe(&mut self.ring, &mut ptr);
            match -errno {
                0 => Ok(IoUringCQEPtr { cqe: ptr }),
                libc::EAGAIN | libc::EBUSY | libc::EINTR => return Err(IoUringError::TryAgain),
                _ => return Err(IoUringError::WaitError(SystemError::new(errno))),
            }
        }
    }

    pub fn cqe_seen(&mut self, entry: IoUringCQEPtr) {
        unsafe {
            io_uring_cqe_seen(&mut self.ring, entry.cqe)
        }
    }
}

impl IoUringCQEPtr {
    pub fn get_data64(&self) -> u64 {
        unsafe {
            io_uring_cqe_get_data64(self.cqe)
        }
    }

    #[inline]
    pub fn copy_from(&self) -> IoUringCQE {
        IoUringCQE {
            result: self.get_result(),
            flags: self.get_flags()
        }
    }

    #[inline]
    pub fn get_result(&self) -> i32 {
        unsafe {
            (*self.cqe).res
        }
    }

    #[inline]
    pub fn get_flags(&self) -> u32 {
        unsafe {
            (*self.cqe).flags
        }
    }
}
