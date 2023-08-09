use std::cell::RefCell;
use std::ffi::{CString, OsStr};
use std::rc::Rc;
use std::mem::{self, ManuallyDrop};
use std::task::Waker;
use std::ops::Drop;
use std::os::unix::prelude::OsStrExt;

use liburing_sys::*;
use io_uring::*;
use thiserror::Error;

pub use io_uring::IoUringCQE;

mod io_uring;

#[derive(Error, Debug)]
pub enum ReactorError {
    #[error("io_uring has no more SQEs available")]
    NoSQEAvailable,
}

#[repr(u8)]
enum ReactorOpExtraParamsType {
    None,
    OpenAt2,
}

#[repr(C)]
struct ReactorOpExtraParamsOpenAt2 {
    how: libc::open_how,
    path: CString,
}

#[repr(C)]
union ReactorOpExtraParamsUnion {
    openat2: mem::ManuallyDrop<ReactorOpExtraParamsOpenAt2>,
}

#[repr(C)]
struct ReactorOpExtraParams {
    tag: ReactorOpExtraParamsType,
    value: ReactorOpExtraParamsUnion,
}

impl ReactorOpExtraParams {
    fn new() -> ReactorOpExtraParams {
        ReactorOpExtraParams {
            tag: ReactorOpExtraParamsType::None,
            value: unsafe { mem::zeroed() },
        }
    }
}

impl Drop for ReactorOpExtraParams {
    fn drop(&mut self) {
        unsafe {
            match self.tag {
                ReactorOpExtraParamsType::None => {},
                ReactorOpExtraParamsType::OpenAt2 => {
                    ManuallyDrop::drop(&mut self.value.openat2);
                }
            }
        }
    }
}

pub struct ReactorOp(io_uring_sqe, ReactorOpExtraParams);

impl ReactorOp {
    pub fn new() -> Self {
        let mut result = ReactorOp {
            0: unsafe { mem::zeroed() },
            1: ReactorOpExtraParams::new(),
        };

        result.prepare_nop();
        result
    }

    pub fn prepare_nop(&mut self) {
        unsafe {
            io_uring_prep_nop(&mut self.0)
        }
    }

    pub fn prepare_close(&mut self, fd: i32) {
        unsafe {
            io_uring_prep_close(&mut self.0, fd)
        }
    }

    pub fn prepare_openat2(&mut self, path: &OsStr, flags: i32, mode: u32) {
        unsafe {
            self.1.tag = ReactorOpExtraParamsType::OpenAt2;
            self.1.value.openat2 = ManuallyDrop::new(ReactorOpExtraParamsOpenAt2 { how: mem::zeroed(), path: CString::new(path.as_bytes()).expect("Null character in filename") });

            io_uring_prep_openat(&mut self.0, libc::AT_FDCWD, self.1.value.openat2.path.as_ptr(), flags, mode);
        }
    }

    pub fn set_data64(&mut self, value: u64) {
        unsafe {
            io_uring_sqe_set_data64(&mut self.0, value)
        }
    }

    pub fn set_flags(&mut self, flags: u32) {
        unsafe {
            io_uring_sqe_set_flags(&mut self.0, flags)
        }
    }
}


#[derive(Clone)]
pub struct OpDescriptorPtr {
    ptr: Rc<RefCell<OpDescriptor>>,
}

impl OpDescriptorPtr {
    pub fn copy_from(&mut self, op: &ReactorOp) {
        self.ptr.borrow_mut().sqe.copy_from(&op.0);
    }

    pub fn completed(&self) -> bool {
        self.ptr.borrow().cqe.is_some()
    }

    pub(self) fn set_cqe(&self, cqe: &IoUringCQEPtr) {
        self.ptr.borrow_mut().cqe = Some(cqe.copy_from());
    }

    pub fn get_cqe(&self) -> IoUringCQE {
        self.ptr.borrow().cqe.unwrap()
    }

    pub fn get_index(&self) -> usize {
        self.ptr.borrow().index
    }

    pub(self) fn notify_completion(&self) {
        self.ptr.borrow_mut().waker.wake_by_ref();
    }
}

struct OpDescriptor {
    sqe: IoUringSQE,
    cqe: Option<IoUringCQE>,
    index: usize,
    waker: Waker,
}

pub struct Reactor {
    ring: IoUring,
    ops: Vec<Option<OpDescriptorPtr>>,
    ops_free_entries: Vec<usize>,
    in_flight: i32,
    uncommited: i32,
}

impl Reactor {
    pub fn new() -> Result<Self, IoUringCreateError> {
        let params = IoUringParams {
            sq_entries: 16,
            cq_entries: 64,
        };

        Ok(Reactor { ring: IoUring::new(params)?, ops: vec![], ops_free_entries: vec![], in_flight: 0, uncommited: 0 })
    }

    pub fn schedule(&mut self, op: &mut ReactorOp, waker: Waker) -> Result<OpDescriptorPtr, ReactorError> {
        if self.ring.sq_space_left() == 0 {
            self.submit();
        }

        let mut desc = self.create_op_descriptor(waker)?;
        op.set_data64(desc.get_index() as u64);
        desc.copy_from(op);

        self.in_flight += 1;
        self.uncommited += 1;
        Ok(desc)
    }

    pub fn pending_ops(&self) -> i32 {
        self.in_flight
    }

    fn create_op_descriptor(&mut self, waker: Waker) -> Result<OpDescriptorPtr, ReactorError> {
        let index = match self.ops_free_entries.pop() {
            Some(index) => index,
            None => self.ops.len(),
        };

        let result = OpDescriptorPtr {
            ptr: Rc::new(RefCell::new(OpDescriptor { sqe: self.get_sqe()?, cqe: None, index, waker }))
        };

        if self.ops.len() == index {
            self.ops.push(Some(result.clone()));
        } else {
            self.ops[index] = Some(result.clone());
        }

        Ok(result)
    }

    fn get_sqe(&mut self) -> Result<IoUringSQE, ReactorError> {
        self.ring.get_sqe().ok_or_else(|| ReactorError::NoSQEAvailable)
    }

    pub fn submit(&mut self) -> i32 {
        let mut result = 0;

        if self.uncommited > 0 {
            result = self.ring.submit();
            self.uncommited = 0;
        }

        result
    }

    pub fn process_ops(&mut self) -> Result<bool, IoUringError> {
        if self.in_flight == 0 {
            return Ok(false);
        }

        let handled = self.process_completed_ops();
        if !handled {
            self.submit();
            self.wait_for_completion()?;
        }

        Ok(true)
    }

    fn process_completed_ops(&mut self) -> bool {
        let mut handled = false;
        while let Some(cqe) = self.ring.peek_cqe() {
            self.process_cqe(cqe);
            handled = true;
        }

        handled
    }

    fn process_cqe(&mut self, cqe: IoUringCQEPtr) {
        self.in_flight -= 1;

        let index = cqe.get_data64() as usize;
        let op = self.ops[index].take().expect("io_uring returned completed op with incorrect index");
        op.set_cqe(&cqe);

        self.ops_free_entries.push(index);
        self.ring.cqe_seen(cqe);

        op.notify_completion();
    }

    fn wait_for_completion(&mut self) -> Result<(), IoUringError> {
        let cqe = self.ring.wait_cqe()?;
        self.process_cqe(cqe);
        Ok(())
    }
}

