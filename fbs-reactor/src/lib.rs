use std::cell::RefCell;
use std::ffi::{CString, OsStr};
use std::rc::Rc;
use std::mem::{self};
use std::task::Waker;
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

enum ReactorOpSQE {
    Unscheduled(io_uring_sqe),
    Scheduled(IoUringSQEPtr),
}

#[derive(Default)]
pub struct ReactorOpParameters {
    path: CString,
    pub buffer: Vec<u8>,
}

impl ReactorOpParameters {
    fn new() -> Self {
        ReactorOpParameters { path: CString::default(), buffer: Vec::default() }
    }
}

struct ReactorOp {
    sqe: ReactorOpSQE,
    parameters: ReactorOpParameters,
    cqe: Option<IoUringCQE>,
    index: usize,
    waker: Option<Waker>,
}

impl ReactorOp {
    fn new() -> Self {
        ReactorOp {
            sqe: ReactorOpSQE::Unscheduled(unsafe { mem::zeroed() }),
            parameters: ReactorOpParameters::new(),
            cqe: None,
            index: 0,
            waker: None
        }
    }
}

#[derive(Clone)]
pub struct ReactorOpPtr {
    ptr: Rc<RefCell<ReactorOp>>,
}

impl ReactorOpPtr {
    pub fn new() -> Self {
        ReactorOpPtr { ptr: Rc::new(RefCell::new(ReactorOp::new())) }
    }

    pub fn fetch_parameters(&mut self) -> ReactorOpParameters {
        std::mem::take(&mut self.ptr.borrow_mut().parameters)
    }

    pub fn prepare_nop(&mut self) {
        match &mut self.ptr.borrow_mut().sqe {
            ReactorOpSQE::Scheduled(_) => panic!("Attempting to prepare already scheduled op"),
            ReactorOpSQE::Unscheduled(sqe) => {
                unsafe {
                    io_uring_prep_nop(sqe);
                }
            }
        }
    }

    pub fn prepare_close(&mut self, fd: i32) {
        match &mut self.ptr.borrow_mut().sqe {
            ReactorOpSQE::Scheduled(_) => panic!("Attempting to prepare already scheduled op"),
            ReactorOpSQE::Unscheduled(sqe) => {
                unsafe {
                    io_uring_prep_close(sqe, fd);
                }
            }
        }
    }

    pub fn prepare_openat2(&mut self, path: &OsStr, flags: i32, mode: u32) {
        let mut op = self.ptr.borrow_mut();
        op.parameters.path = CString::new(path.as_bytes()).expect("Null character in filename");

        match &mut op.sqe {
            ReactorOpSQE::Scheduled(_) => panic!("Attempting to prepare already scheduled op"),
            ReactorOpSQE::Unscheduled(sqe) => {
                unsafe {
                    io_uring_prep_openat(sqe, libc::AT_FDCWD, op.parameters.path.as_ptr(), flags, mode);
                }
            }
        }
    }

    pub fn prepare_socket(&mut self, domain: i32, socket_type: i32, protocol: i32) {
        let mut op = self.ptr.borrow_mut();
        match &mut op.sqe {
            ReactorOpSQE::Scheduled(_) => panic!("Attempting to prepare already scheduled op"),
            ReactorOpSQE::Unscheduled(sqe) => {
                unsafe {
                    io_uring_prep_socket(sqe, domain, socket_type, protocol, 0);
                }
            }
        }
    }

    pub fn prepare_read(&mut self, fd: i32, buffer: Vec<u8>, offset: Option<u64>) {
        let mut op = self.ptr.borrow_mut();
        op.parameters.buffer = buffer;

        match &mut op.sqe {
            ReactorOpSQE::Scheduled(_) => panic!("Attempting to prepare already scheduled op"),
            ReactorOpSQE::Unscheduled(sqe) => {
                unsafe {
                    io_uring_prep_read(sqe, fd, op.parameters.buffer.as_mut_ptr() as *mut libc::c_void, op.parameters.buffer.len() as u32, offset.unwrap_or(u64::MAX));
                }
            }
        }
    }

    fn schedule(&self, mut target_sqe: IoUringSQEPtr, index: usize, waker: Waker) {
        let mut op = self.ptr.borrow_mut();
        match &op.sqe {
            ReactorOpSQE::Unscheduled(sqe) => {
                target_sqe.copy_from(sqe);
                target_sqe.set_data64(index as u64);
            },
            ReactorOpSQE::Scheduled(_) => {
                panic!("Trying to schedule already scheduled op");
            }
        }

        op.index = index;
        op.waker = Some(waker);
        op.sqe = ReactorOpSQE::Scheduled(target_sqe);
    }

    fn opcode(&self) -> u8 {
        let mut op = self.ptr.borrow_mut();
        match &mut op.sqe {
            ReactorOpSQE::Scheduled(sqe) => {
                return sqe.opcode();
            },
            ReactorOpSQE::Unscheduled(sqe) => {
                // opcode is u8 field at offset zero
                return unsafe { *(sqe as *const io_uring_sqe as *const u8) };
            }
        }
    }

    pub fn scheduled(&self) -> bool {
        match self.ptr.borrow().sqe {
            ReactorOpSQE::Scheduled(_) => true,
            ReactorOpSQE::Unscheduled(_) => false,
        }
    }

    pub fn completed(&self) -> bool {
        self.ptr.borrow().cqe.is_some()
    }

    fn set_cqe(&self, cqe: IoUringCQEPtr) {
        self.ptr.borrow_mut().cqe = Some(cqe.copy_from());
    }

    pub fn get_cqe(&self) -> IoUringCQE {
        self.ptr.borrow().cqe.unwrap()
    }

    fn notify_completion(&self) {
        self.ptr.borrow_mut().waker.as_ref().unwrap().wake_by_ref();
    }
}

pub struct Reactor {
    ring: IoUring,
    ops: Vec<Option<ReactorOpPtr>>,
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

    pub fn is_supported(&self, op: &ReactorOpPtr) -> bool {
        self.ring.is_op_supported(op.opcode())
    }

    pub fn schedule(&mut self, op: &ReactorOpPtr, waker: Waker) -> Result<(), ReactorError> {
        if self.ring.sq_space_left() == 0 {
            self.submit();
        }

        let sqe = self.get_sqe()?;

        let index = match self.ops_free_entries.pop() {
            Some(index) => index,
            None => self.ops.len(),
        };

        op.schedule(sqe, index, waker);

        if self.ops.len() == index {
            self.ops.push(Some(op.clone()));
        } else {
            self.ops[index] = Some(op.clone());
        }

        self.in_flight += 1;
        self.uncommited += 1;

        Ok(())
    }

    pub fn pending_ops(&self) -> i32 {
        self.in_flight
    }

    fn get_sqe(&mut self) -> Result<IoUringSQEPtr, ReactorError> {
        self.ring.get_sqe().ok_or_else(|| ReactorError::NoSQEAvailable)
    }

    fn submit(&mut self) -> i32 {
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
        op.set_cqe(cqe);

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

