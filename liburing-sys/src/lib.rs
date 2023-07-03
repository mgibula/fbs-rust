#![allow(nonstandard_style)]
#![allow(improper_ctypes)]
use libc::statx;
use libc::__u16;
use libc::__u32;
use libc::__u64;
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

