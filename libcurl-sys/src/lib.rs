#![allow(nonstandard_style)]

use libc::fd_set;
use libc::time_t;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
