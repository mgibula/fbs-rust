use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::rc::Rc;
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

pub type OpCompletion = Option<Box<dyn Fn(IoUringCQE, ReactorOpParameters)>>;

pub struct IOUringReq {
    pub completion: OpCompletion,
    pub op: IOUringOp,
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
    InProgress(),

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
    Completed(i32),
}

struct ReactorOp {
    state: Cell<OpState>,
    parameters: RefCell<ReactorOpParameters>,
}

impl ReactorOp {
    fn new() -> Self {
        ReactorOp {
            state: Cell::new(OpState::Unscheduled()),
            parameters: RefCell::new(ReactorOpParameters::default()),
        }
    }

    fn reset(&self) {
        self.state.set(OpState::Unscheduled());
        self.parameters.borrow_mut().reset();
    }
}

#[derive(Clone)]
struct ReactorOpPtr {
    ptr: Rc<ReactorOp>,
}

impl ReactorOpPtr {
    pub fn new() -> Self {
        ReactorOpPtr { ptr: Rc::new(ReactorOp::new()) }
    }

    pub fn result_code(&self) -> Option<i32> {
        let old = self.ptr.state.replace(OpState::Unscheduled());
        match old {
            OpState::Completed(result) => Some(result),
            _ => {
                self.ptr.state.set(old);
                None
            },
        }
    }

    fn complete_op(&mut self, cqe: IoUringCQE, params: ReactorOpParameters) {
        let completion = self.ptr.state.replace(OpState::Completed(cqe.result));
        if let OpState::Scheduled(Some(completion)) = completion {
            completion(cqe, params);
        }
    }

    fn reset(&self) {
        self.ptr.reset()
    }
}

pub struct Reactor {
    ring: IoUring,
    ops: Vec<Option<ReactorOpPtr>>,
    ops_free_entries: Vec<usize>,
    in_flight: u32,
    uncommited: u32,
    rop_cache: Vec<ReactorOpPtr>,
}

impl Reactor {
    pub fn new() -> Result<Self, IoUringCreateError> {
        let params = IoUringParams {
            sq_entries: 16,
            cq_entries: 64,
        };

        Ok(Reactor { ring: IoUring::new(params)?, ops: vec![], ops_free_entries: vec![], in_flight: 0, uncommited: 0, rop_cache: vec![] })
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
        match self.rop_cache.pop() {
            Some(rop) => rop,
            None => ReactorOpPtr::new()
        }
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
            let sqe = self.get_sqe().expect("Can't get SQE from io_uring");
            let index = self.get_next_index();

            let rop = self.get_rop();
            let requested = std::mem::replace(&mut req.op, IOUringOp::InProgress());

            unsafe {
                let mut parameters = rop.ptr.parameters.borrow_mut();
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

                        io_uring_prep_timeout(sqe.ptr, &mut parameters.timeout, 0, 0);
                    },
                    IOUringOp::InProgress() => panic!("op already scheduled"),
                }

                rop.ptr.state.set(OpState::Scheduled(req.completion.take()));

                let mut flags = 0;
                if op_index as u32 != ops_count - 1 {
                    flags |= IOSQE_IO_LINK;
                }

                io_uring_sqe_set_data64(sqe.ptr, index as u64);
                io_uring_sqe_set_flags(sqe.ptr, flags);
            }

            self.ops[index] = Some(rop);
        });

    }

    fn retire_rop(&mut self, rop: ReactorOpPtr) {
        rop.reset();
        self.rop_cache.push(rop)
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
        let mut rop = self.ops[index].take().expect("io_uring returned completed op with incorrect index");

        self.ops_free_entries.push(index);
        self.ring.cqe_seen(cqe);

        let params = rop.ptr.parameters.take();
        rop.complete_op(cqe.copy_from(), params);
        self.retire_rop(rop);
    }

    fn wait_for_completion(&mut self) -> Result<(), IoUringError> {
        let cqe = self.ring.wait_cqe()?;
        self.process_cqe(cqe);
        Ok(())
    }
}

