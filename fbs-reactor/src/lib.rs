use std::cell::RefCell;
use std::ops::IndexMut;
use std::rc::Rc;
use std::mem;
use std::task::Waker;

use liburing_sys::*;
use io_uring::*;
use thiserror::Error;

mod io_uring;

#[derive(Error, Debug)]
pub enum ReactorError {
    #[error("io_uring has no more SQEs available")]
    NoSQEAvailable,
}

pub struct ReactorOp(io_uring_sqe);

impl ReactorOp {
    pub fn new() -> Self {
        let mut result : ReactorOp = unsafe { mem::zeroed() };
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

    pub fn get_index(&self) -> usize {
        self.ptr.borrow().index
    }
}

struct OpDescriptor {
    sqe: IoUringSQE,
    cqe: Option<IoUringCQE>,
    index: usize,
    waker: Waker,
    result_is_fd: bool,
    ts: Option<__kernel_timespec>,
}

pub struct Reactor {
    ring: IoUring,
    ops: Vec<Option<OpDescriptorPtr>>,
    ops_free_entries: Vec<usize>,
    in_flight: i32,
}

impl Reactor {
    pub fn new() -> Result<Self, IoUringCreateError> {
        let params = IoUringParams {
            sq_entries: 16,
            cq_entries: 64,
        };

        Ok(Reactor { ring: IoUring::new(params)?, ops: vec![], ops_free_entries: vec![], in_flight: 0 })
    }

    pub fn schedule(&mut self, op: &mut ReactorOp, waker: Waker) -> Result<OpDescriptorPtr, ReactorError> {
        if self.ring.sq_space_left() == 0 {
            self.submit();
        }

        let mut desc = self.create_op_descriptor(waker)?;
        op.set_data64(desc.get_index() as u64);
        desc.copy_from(op);

        self.in_flight += 1;
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
            ptr: Rc::new(RefCell::new(OpDescriptor { sqe: self.get_sqe()?, cqe: None, result_is_fd: false, ts: None, index, waker }))
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

    fn submit(&mut self) -> i32 {
        self.ring.submit()
    }
}

