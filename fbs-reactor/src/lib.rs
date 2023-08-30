use std::cell::RefCell;
use std::ffi::{CString, OsStr};
use std::rc::Rc;
use std::mem::{self};
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

pub type OpCompletion = Option<Box<dyn Fn(IoUringCQE, ReactorOpParameters)>>;

pub struct IOUringReq {
    pub completion: OpCompletion,
    pub op: IOUringOp,
}

#[non_exhaustive]
pub struct IOUringOpType;

impl IOUringOpType {
    pub const Nop: u32 = io_uring_op_IORING_OP_NOP;
    pub const Close: u32 = io_uring_op_IORING_OP_CLOSE;
    pub const Open: u32 = io_uring_op_IORING_OP_OPENAT;
    pub const Read: u32 = io_uring_op_IORING_OP_READ;
    pub const Write: u32 = io_uring_op_IORING_OP_WRITE;
    pub const Socket: u32 = io_uring_op_IORING_OP_SOCKET;
}

pub enum IOUringOp {
    InProgress(ReactorOpPtr),

    Nop(),
    Close(i32),                         // fd
    Open(CString, i32, u32),            // path, flags, mode
    Read(i32, Vec<u8>, Option<u64>),    // fd, buffer, offset
    Write(i32, Vec<u8>, Option<u64>),   // fd, buffer, offset
    Socket(i32, i32, i32),
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

enum OpState {
    Unscheduled(),
    Scheduled(OpCompletion),
    Completed(),
}

struct ReactorOp {
    state: OpState,
    parameters: ReactorOpParameters,
}

impl ReactorOp {
    fn new() -> Self {
        ReactorOp {
            state: OpState::Unscheduled(),
            parameters: ReactorOpParameters::new(),
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

    pub fn completed(&self) -> bool {
        if let OpState::Completed() = self.ptr.borrow().state {
            return true;
        }

        false
    }

    fn complete_op(&mut self, cqe: IoUringCQE, params: ReactorOpParameters) {
        let completion = std::mem::replace(&mut self.ptr.borrow_mut().state, OpState::Completed());
        if let OpState::Scheduled(Some(completion)) = completion {
            completion(cqe, params);
        }
    }
}

pub struct Reactor {
    ring: IoUring,
    ops: Vec<Option<ReactorOpPtr>>,
    ops_free_entries: Vec<usize>,
    in_flight: u32,
    uncommited: u32,
}

impl Reactor {
    pub fn new() -> Result<Self, IoUringCreateError> {
        let params = IoUringParams {
            sq_entries: 16,
            cq_entries: 64,
        };

        Ok(Reactor { ring: IoUring::new(params)?, ops: vec![], ops_free_entries: vec![], in_flight: 0, uncommited: 0 })
    }

    pub fn is_supported(&self, opcode: u32) -> bool {
        self.ring.is_op_supported(opcode)
    }

    fn get_next_index(&mut self) -> usize {
        let index = match self.ops_free_entries.pop() {
            Some(index) => index,
            None => {
                self.ops.push(None);
                self.ops.len() - 1
            }
        };

        index
    }

    pub fn schedule_linked2(&mut self, ops: &mut [IOUringReq]) {
        let ops_count = ops.len() as u32;

        if self.ring.sq_space_left() < ops_count {
            self.submit();
        }

        if self.ring.sq_space_left() < ops_count {
            panic!("Not enough SQE entries after ring has been flushed");
        }

        self.in_flight += ops_count;
        self.uncommited += ops_count;

        ops.into_iter().enumerate().for_each(|(op_index, req)| {
            let rop = ReactorOpPtr::new();

            let sqe = self.get_sqe().expect("Can't get SQE from io_uring");
            let index = self.get_next_index();

            unsafe {
                let mut rop = rop.ptr.borrow_mut();
                match &mut req.op {
                    IOUringOp::Nop() => {
                        io_uring_prep_nop(sqe.ptr);
                    },
                    IOUringOp::Close(fd) => {
                        io_uring_prep_close(sqe.ptr, *fd);
                    },
                    IOUringOp::Open(path, flags, mode) => {
                        rop.parameters.path = CString::new(path.as_bytes()).expect("Null character in filename");

                        io_uring_prep_openat(sqe.ptr, libc::AT_FDCWD, rop.parameters.path.as_ptr(), *flags, *mode);
                    },
                    IOUringOp::Read(fd, buffer, offset) => {
                        rop.parameters.buffer = std::mem::take(buffer);

                        io_uring_prep_read(sqe.ptr, *fd, rop.parameters.buffer.as_mut_ptr() as *mut libc::c_void, rop.parameters.buffer.len() as u32, offset.unwrap_or(u64::MAX));
                    },
                    IOUringOp::Write(fd, buffer, offset) => {
                        rop.parameters.buffer = std::mem::take(buffer);

                        io_uring_prep_write(sqe.ptr, *fd, rop.parameters.buffer.as_ptr() as *mut libc::c_void, rop.parameters.buffer.len() as u32, offset.unwrap_or(u64::MAX));
                    },
                    IOUringOp::Socket(domain, socket_type, protocol) => {
                        io_uring_prep_socket(sqe.ptr, *domain, *socket_type, *protocol, 0);
                    },
                    IOUringOp::InProgress(_) => panic!("op already scheduled"),
                }

                rop.state = OpState::Scheduled(req.completion.take());

                let mut flags = 0;
                if op_index as u32 != ops_count - 1 {
                    flags |= IOSQE_IO_LINK;
                }

                io_uring_sqe_set_data64(sqe.ptr, index as u64);
                io_uring_sqe_set_flags(sqe.ptr, flags);
            }

            self.ops[index] = Some(rop.clone());
            req.op = IOUringOp::InProgress(rop);
        });

    }

    pub fn pending_ops(&self) -> u32 {
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
        let mut op = self.ops[index].take().expect("io_uring returned completed op with incorrect index");

        self.ops_free_entries.push(index);
        self.ring.cqe_seen(cqe);

        let params = std::mem::take(&mut op.ptr.borrow_mut().parameters);
        op.complete_op(cqe.copy_from(), params);
    }

    fn wait_for_completion(&mut self) -> Result<(), IoUringError> {
        let cqe = self.ring.wait_cqe()?;
        self.process_cqe(cqe);
        Ok(())
    }
}

