use std::cell::RefCell;
use std::ffi::{CString, OsStr};
use std::rc::Rc;
use std::mem::{self};
use std::os::unix::prelude::OsStrExt;
use std::slice;

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
    result_is_fd: bool,
}

impl ReactorOp {
    fn new() -> Self {
        ReactorOp {
            state: OpState::Unscheduled(),
            parameters: ReactorOpParameters::new(),
            result_is_fd: false,
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
        unsafe {
            let mut sqe: io_uring_sqe = mem::zeroed();
            io_uring_prep_nop(&mut sqe);
            // self.ptr.borrow_mut().state = OpState::Unscheduled(sqe);
        }
    }

    pub fn prepare_close(&mut self, fd: i32) {
        unsafe {
            let mut sqe: io_uring_sqe = mem::zeroed();
            io_uring_prep_close(&mut sqe, fd);
            // self.ptr.borrow_mut().state = OpState::Unscheduled(sqe);
        }
    }

    pub fn prepare_openat2(&mut self, path: &OsStr, flags: i32, mode: u32) {
        let mut op = self.ptr.borrow_mut();
        op.parameters.path = CString::new(path.as_bytes()).expect("Null character in filename");

        unsafe {
            let mut sqe: io_uring_sqe = mem::zeroed();
            io_uring_prep_openat(&mut sqe, libc::AT_FDCWD, op.parameters.path.as_ptr(), flags, mode);
            // self.ptr.borrow_mut().state = OpState::Unscheduled(sqe);
        }
    }

    pub fn prepare_socket(&mut self, domain: i32, socket_type: i32, protocol: i32) {
        unsafe {
            let mut sqe: io_uring_sqe = mem::zeroed();
            io_uring_prep_socket(&mut sqe, domain, socket_type, protocol, 0);
            // self.ptr.borrow_mut().state = OpState::Unscheduled(sqe);
        }
    }

    pub fn prepare_read(&mut self, fd: i32, buffer: Vec<u8>, offset: Option<u64>) {
        let mut op = self.ptr.borrow_mut();
        op.parameters.buffer = buffer;

        unsafe {
            let mut sqe: io_uring_sqe = mem::zeroed();
            io_uring_prep_read(&mut sqe, fd, op.parameters.buffer.as_mut_ptr() as *mut libc::c_void, op.parameters.buffer.len() as u32, offset.unwrap_or(u64::MAX));
            // self.ptr.borrow_mut().state = OpState::Unscheduled(sqe);
        }
    }

    pub fn prepare_write(&mut self, fd: i32, buffer: Vec<u8>, offset: Option<u64>) {
        let mut op = self.ptr.borrow_mut();
        op.parameters.buffer = buffer;

        unsafe {
            let mut sqe: io_uring_sqe = mem::zeroed();
            io_uring_prep_write(&mut sqe, fd, op.parameters.buffer.as_ptr() as *const libc::c_void, op.parameters.buffer.len() as u32, offset.unwrap_or(u64::MAX));
            // self.ptr.borrow_mut().state = OpState::Unscheduled(sqe);
        }
    }

    fn opcode(&self) -> u8 {
        unimplemented!()
        // let mut op = self.ptr.borrow_mut();
        // match &mut op.sqe {
        //     ReactorOpSQE::Scheduled(sqe) => {
        //         return sqe.opcode();
        //     },
        //     ReactorOpSQE::Unscheduled(sqe) => {
        //         // opcode is u8 field at offset zero
        //         return unsafe { *(sqe as *const io_uring_sqe as *const u8) };
        //     }
        // }
    }

    pub fn scheduled(&self) -> bool {
        unimplemented!()
        // match self.ptr.borrow().state {
        //     OpState::InProgress() => true,
        //     _ => false,
        // }
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

    pub fn is_supported(&self, op: &ReactorOpPtr) -> bool {
        self.ring.is_op_supported(op.opcode())
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
dbg!(ops_count);
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
                        rop.result_is_fd = true;
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

