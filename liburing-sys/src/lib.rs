#![allow(nonstandard_style)]
#![allow(improper_ctypes)]

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

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub const IOSQE_FIXED_FILE: u32 = 1 << IOSQE_FIXED_FILE_BIT;
pub const IOSQE_IO_DRAIN: u32 = 1 << IOSQE_IO_DRAIN_BIT;
pub const IOSQE_IO_LINK: u32 = 1 << IOSQE_IO_LINK_BIT;
pub const IOSQE_IO_HARDLINK: u32 = 1 << IOSQE_IO_HARDLINK_BIT;
pub const IOSQE_ASYNC: u32 = 1 << IOSQE_ASYNC_BIT;
pub const IOSQE_BUFFER_SELECT: u32 = 1 << IOSQE_BUFFER_SELECT_BIT;
pub const IOSQE_CQE_SKIP_SUCCESS: u32 = 1 << IOSQE_CQE_SKIP_SUCCESS_BIT;

impl Default for __kernel_timespec {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}