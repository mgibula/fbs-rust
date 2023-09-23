use std::ffi::CString;
use std::time::Duration;

use liburing_sys::*;
use io_uring::*;
use thiserror::Error;
use fbs_library::socket_address::{SocketIpAddress, SocketAddressBinary};

pub use io_uring::IoUringCQE;

mod io_uring;

#[derive(Error, Debug)]
pub enum ReactorError {
    #[error("io_uring has no more SQEs available")]
    NoSQEAvailable,
}

const CQE_CANCEL_CQE: u64 = u64::MAX;
const CQE_TIMEOUT_CQE: u64 = u64::MAX - 1;

pub type OpCompletion = Option<Box<dyn Fn(IoUringCQE, ReactorOpParameters)>>;

pub struct IOUringReq {
    pub op: IOUringOp,
    pub completion: OpCompletion,
    pub timeout: Option<Duration>,
}

#[non_exhaustive]
pub struct IOUringOpType;

impl IOUringOpType {
    pub const NOP: u32 = io_uring_op_IORING_OP_NOP;
    pub const CLOSE: u32 = io_uring_op_IORING_OP_CLOSE;
    pub const OPEN: u32 = io_uring_op_IORING_OP_OPENAT;
    pub const READ: u32 = io_uring_op_IORING_OP_READ;
    pub const WRITE: u32 = io_uring_op_IORING_OP_WRITE;
    pub const SOCKET: u32 = io_uring_op_IORING_OP_SOCKET;
    pub const ACCEPT: u32 = io_uring_op_IORING_OP_ACCEPT;
    pub const CONNECT: u32 = io_uring_op_IORING_OP_CONNECT;
    pub const TIMEOUT: u32 = io_uring_op_IORING_OP_TIMEOUT;
}

pub enum IOUringOp {
    InProgress((u64, usize)),

    Nop(),
    Close(i32),                         // fd
    Open(CString, i32, u32),            // path, flags, mode
    Read(i32, Vec<u8>, Option<u64>),    // fd, buffer, offset
    Write(i32, Vec<u8>, Option<u64>),   // fd, buffer, offset
    Socket(i32, i32, i32),
    Accept(i32, i32),
    Connect(i32, SocketIpAddress),
    Sleep(Duration),
}

#[derive(Default)]
pub struct ReactorOpParameters {
    timeout: __kernel_timespec,
    path: CString,
    address: SocketAddressBinary,
    pub buffer: Vec<u8>,
}

impl ReactorOpParameters {
    fn reset(&mut self) {
        self.timeout = unsafe { std::mem::zeroed() };
        self.address = SocketAddressBinary::default();
        self.buffer.clear();
        self.path = CString::default();
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
    seq: u64,
}

impl ReactorOp {
    fn new(seq: u64) -> Self {
        ReactorOp {
            state: OpState::Unscheduled(),
            parameters: ReactorOpParameters::default(),
            seq,
        }
    }

    fn reset(&mut self) {
        self.state = OpState::Unscheduled();
        self.parameters.reset();
    }
}

struct ReactorOpPtr {
    ptr: Box<ReactorOp>,
}

impl ReactorOpPtr {
    pub fn new(seq: u64) -> Self {
        ReactorOpPtr { ptr: Box::new(ReactorOp::new(seq)) }
    }

    fn complete_op(&mut self, cqe: IoUringCQE, params: ReactorOpParameters) {
        let completion = std::mem::replace(&mut self.ptr.state, OpState::Completed());
        if let OpState::Scheduled(Some(completion)) = completion {
            completion(cqe, params);
        }
    }

    fn reset(&mut self) {
        self.ptr.reset()
    }

    fn seq_number(&self) -> u64 {
        self.ptr.seq
    }
}

pub struct Reactor {
    ring: IoUring,
    ops: Vec<Option<ReactorOpPtr>>,
    ops_free_entries: Vec<usize>,
    in_flight: u32,
    uncommited: u32,
    rop_cache: Vec<ReactorOpPtr>,
    seq: u64,
}

impl Reactor {
    pub fn new() -> Result<Self, IoUringCreateError> {
        let params = IoUringParams {
            sq_entries: 16,
            cq_entries: 64,
        };

        Ok(Reactor { ring: IoUring::new(params)?, ops: vec![], ops_free_entries: vec![], in_flight: 0, uncommited: 0, rop_cache: vec![], seq: 0 })
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

    fn get_rop(&mut self) -> ReactorOpPtr {
        self.seq += 1;
        match self.rop_cache.pop() {
            Some(mut rop) => { rop.ptr.seq = self.seq; rop },
            None => ReactorOpPtr::new(self.seq)
        }
    }

    pub fn cancel_op(&mut self, cancel_tags: &[(u64, usize)]) {
        // Cancelled op may still be waiting for submission
        self.submit().expect("Error on submit");

        cancel_tags.into_iter().for_each(|(seq, index)| {
            let seq = *seq;
            let index = *index;

            if self.ops.len() <= index {
                return;
            }

            let op = &self.ops[index];
            let op = match op {
                None => return,
                Some(ptr) => ptr,
            };

            if op.ptr.seq != seq {
                return;
            }

            self.enqueue_cancel(index);
        });

        // Fire up cancellations immediately
        self.submit().expect("Error on submit");
    }

    fn enqueue_cancel(&mut self, index: usize) {
        let sqe = self.get_sqe().expect("Can't get SQE from io_uring");

        unsafe {
            io_uring_prep_cancel64(sqe.ptr, index as u64, 0);
            io_uring_sqe_set_data64(sqe.ptr, CQE_CANCEL_CQE);
            io_uring_sqe_set_flags(sqe.ptr, 0); // IOSQE_CQE_SKIP_SUCCESS seems to be not supported by cancel op
        }
    }

    pub fn schedule_linked2(&mut self, ops: &mut [&mut IOUringReq]) {
        let ops_count = ops.len() as u32;

        if self.ring.sq_space_left() < ops_count {
            self.submit().expect("Error on submit");
        }

        if self.ring.sq_space_left() < ops_count {
            panic!("Not enough SQE entries after ring has been flushed");
        }

        self.in_flight += ops_count;

        ops.into_iter().enumerate().for_each(|(op_index, req)| {
            let op_index = op_index as u32;
            let sqe = self.get_sqe().expect("Can't get SQE from io_uring");
            let index = self.get_next_index();

            let mut rop = self.get_rop();
            let requested = std::mem::replace(&mut req.op, IOUringOp::InProgress((rop.seq_number(), index)));

            unsafe {
                let parameters = &mut rop.ptr.parameters;
                match requested {
                    IOUringOp::Nop() => {
                        io_uring_prep_nop(sqe.ptr);
                    },
                    IOUringOp::Close(fd) => {
                        io_uring_prep_close(sqe.ptr, fd);
                    },
                    IOUringOp::Open(path, flags, mode) => {
                        parameters.path = path;

                        io_uring_prep_openat(sqe.ptr, libc::AT_FDCWD, parameters.path.as_ptr(), flags, mode);
                    },
                    IOUringOp::Read(fd, buffer, offset) => {
                        parameters.buffer = buffer;

                        io_uring_prep_read(sqe.ptr, fd, parameters.buffer.as_mut_ptr() as *mut libc::c_void, parameters.buffer.len() as u32, offset.unwrap_or(u64::MAX));
                    },
                    IOUringOp::Write(fd, buffer, offset) => {
                        parameters.buffer = buffer;

                        io_uring_prep_write(sqe.ptr, fd, parameters.buffer.as_ptr() as *mut libc::c_void, parameters.buffer.len() as u32, offset.unwrap_or(u64::MAX));
                    },
                    IOUringOp::Socket(domain, socket_type, protocol) => {
                        io_uring_prep_socket(sqe.ptr, domain, socket_type, protocol, 0);
                    },
                    IOUringOp::Accept(fd, flags) => {
                        io_uring_prep_accept(sqe.ptr, fd, std::ptr::null_mut(), std::ptr::null_mut(), flags);
                    },
                    IOUringOp::Connect(fd, address) => {
                        parameters.address = address.to_binary();

                        io_uring_prep_connect(sqe.ptr, fd, parameters.address.sockaddr_ptr(), parameters.address.length() as u32);
                    },
                    IOUringOp::Sleep(timeout) => {
                        parameters.timeout.tv_sec = timeout.as_secs() as i64;
                        parameters.timeout.tv_nsec = timeout.subsec_nanos() as i64;
                        req.timeout = None; // timeout on sleep makes no sense, and more importantly, uses same timeout field in parameters struct

                        io_uring_prep_timeout(sqe.ptr, &mut parameters.timeout, 0, 0);
                    },
                    IOUringOp::InProgress(_) => panic!("op already scheduled"),
                }

                rop.ptr.state = OpState::Scheduled(req.completion.take());

                let mut flags = 0;
                if op_index != ops_count - 1 || req.timeout.is_some() {
                    flags |= IOSQE_IO_LINK;
                }

                io_uring_sqe_set_data64(sqe.ptr, index as u64);
                io_uring_sqe_set_flags(sqe.ptr, flags);

                if let Some(timeout) = req.timeout {
                    self.enqueue_timeout(timeout, parameters, op_index == ops_count - 1);
                }
            }

            self.ops[index] = Some(rop);
        });

    }

    fn enqueue_timeout(&mut self, timeout: Duration, parameters: &mut ReactorOpParameters, is_last: bool) {
        let sqe = self.get_sqe().expect("Can't get SQE from io_uring");
        let mut flags = IOSQE_CQE_SKIP_SUCCESS;
        if !is_last {
            flags |= IOSQE_IO_LINK;
        }

        unsafe {
            parameters.timeout.tv_sec = timeout.as_secs() as i64;
            parameters.timeout.tv_nsec = timeout.subsec_nanos() as i64;

            io_uring_prep_link_timeout(sqe.ptr, &mut parameters.timeout, 0);
            io_uring_sqe_set_data64(sqe.ptr, CQE_TIMEOUT_CQE);
            io_uring_sqe_set_flags(sqe.ptr, flags);
        }
    }

    fn retire_rop(&mut self, mut rop: ReactorOpPtr) {
        rop.reset();
        self.rop_cache.push(rop)
    }

    pub fn pending_ops(&self) -> u32 {
        self.in_flight
    }

    fn get_sqe(&mut self) -> Result<IoUringSQEPtr, ReactorError> {
        let result = self.ring.get_sqe().ok_or_else(|| ReactorError::NoSQEAvailable);
        if result.is_ok() {
            self.uncommited += 1;
        }

        result
    }

    fn submit(&mut self) -> Result<i32, IoUringError> {
        let mut result = 0;

        if self.uncommited > 0 {
            result = self.ring.submit()?;
            self.uncommited = 0;
        }

        Ok(result)
    }

    pub fn process_ops(&mut self) -> Result<bool, IoUringError> {
        if self.in_flight == 0 {
            return Ok(false);
        }

        let handled = self.process_completed_ops();
        if !handled {
            self.submit()?;
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
        let index = cqe.get_data64();
        let index = index as usize;

        match index as u64 {
            CQE_TIMEOUT_CQE => (),
            CQE_CANCEL_CQE => (),
            index => {
                let index = index as usize;
                let mut rop = self.ops[index].take().expect("io_uring returned completed op with incorrect index");

                self.in_flight -= 1;
                self.ops_free_entries.push(index);

                let params = std::mem::take(&mut rop.ptr.parameters);
                rop.complete_op(cqe.copy_from(), params);
                self.retire_rop(rop);
            },
        }

        self.ring.cqe_seen(cqe);
    }

    fn wait_for_completion(&mut self) -> Result<(), IoUringError> {
        let cqe = self.ring.wait_cqe()?;
        self.process_cqe(cqe);
        Ok(())
    }
}

