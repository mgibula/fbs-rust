use std::os::fd::IntoRawFd;
use std::{ffi::CString, mem::ManuallyDrop};
use std::time::Duration;
use std::alloc::Layout;

use liburing_sys::*;
use io_uring::*;
use thiserror::Error;
use fbs_library::socket_address::{SocketIpAddress, SocketAddressBinary};
use fbs_library::poll::PollMask;

pub use io_uring::IoUringCQE;

mod io_uring;

#[derive(Error, Debug)]
pub enum ReactorError {
    #[error("io_uring has no more SQEs available")]
    NoSQEAvailable,
}

const CQE_CANCEL_CQE: u64 = u64::MAX;
const CQE_TIMEOUT_CQE: u64 = u64::MAX - 1;
const CQE_INVALID: u64 = u64::MAX - 2;

pub type OpCompletion = Option<Box<dyn FnOnce(IoUringCQE, ReactorOpParameters)>>;

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

pub struct Buffer {
    ptr: *mut u8,
    size: usize,
    capacity: usize,
    layout: Layout,
}

impl Buffer {
    fn is_valid(&self) -> bool {
        !self.ptr.is_null()
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    fn size(&self) -> usize {
        self.size
    }

    fn capacity(&self) -> usize {
        self.capacity
    }

    fn clear(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                std::alloc::dealloc(self.ptr, self.layout)
            }

            self.ptr = std::ptr::null_mut();
            self.size = 0;
            self.capacity = 0;
            self.layout = Layout::new::<u8>();
        }
    }

    pub fn new_struct<T: Copy>() -> Self {
        let layout = Layout::new::<T>();
        Self {
            ptr: unsafe { std::alloc::alloc_zeroed(layout) },
            size: std::mem::size_of::<T>(),
            capacity: std::mem::size_of::<T>(),
            layout,
        }
    }

    pub fn from_vec<T: Copy>(buffer: Vec<T>) -> Self {
        let mut buffer = ManuallyDrop::new(buffer);

        Self {
            ptr: buffer.as_mut_ptr() as *mut u8,
            size: buffer.len() * std::mem::size_of::<T>(),
            capacity: buffer.capacity() * std::mem::size_of::<T>(),
            layout: Layout::new::<T>(),
        }
    }

    pub unsafe fn to_struct<T: Copy>(self, bytes: usize) -> T {
        assert!(bytes == std::mem::size_of::<T>());
        unsafe { std::ptr::read(self.ptr as *mut T) }

        // self.ptr is still set, so destructor will clear it
    }

    pub unsafe fn to_vec<T: Copy>(mut self, bytes: usize) -> Vec<T> {
        if !self.is_valid() {
            return Vec::new();
        }

        assert!(self.size % std::mem::size_of::<T>() == 0);
        assert!(self.capacity % std::mem::size_of::<T>() == 0);

        let result = unsafe { Vec::from_raw_parts(self.ptr as *mut T, bytes / std::mem::size_of::<T>(), self.capacity / std::mem::size_of::<T>()) };

        // self.ptr is still set but ownership is transfered to return value, so need to clear it to avoid double free in a destructor
        self.ptr = std::ptr::null_mut();
        result
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                std::alloc::dealloc(self.ptr, self.layout)
            }
        }
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self { ptr: std::ptr::null_mut(), size: 0, capacity: 0, layout: Layout::new::<u8>() }
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub struct MaybeFd(i32);

impl MaybeFd {
    pub fn new<T: IntoRawFd>(fd: T) -> Self {
        Self(fd.into_raw_fd())
    }

    fn take_fd(&mut self) -> i32 {
        let result = self.0;
        self.0 = -1;
        result
    }
}

impl Default for MaybeFd {
    fn default() -> Self {
        Self(-1)
    }
}

impl Drop for MaybeFd {
    fn drop(&mut self) {
        if self.0 >= 0 {
            unsafe { libc::close(self.0); }
        }
    }
}

pub enum IOUringOp {
    InProgress((u64, usize)),

    Nop(),
    Close(MaybeFd),                    // fd
    Open(CString, i32, u32),           // path, flags, mode
    Read(i32, Buffer, Option<u64>),    // fd, buffer, offset
    Write(i32, Buffer, Option<u64>),   // fd, buffer, offset
    Socket(i32, i32, i32),
    Accept(i32, i32),
    Connect(i32, SocketIpAddress),
    Sleep(Duration),
    Cancel(u64, usize),
    SleepUpdate((u64, usize), Duration),
    Poll(i32, PollMask),
    PollUpdate((u64, usize), PollMask),
}

#[derive(Default)]
pub struct ReactorOpParameters {
    timeout: __kernel_timespec,
    path: CString,
    address: SocketAddressBinary,
    pub buffer: Buffer,
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

            if self.cancel_token_is_valid(seq, index) {
                self.enqueue_cancel(index);
            }
        });

        // Fire up cancellations immediately
        self.submit().expect("Error on submit");
    }

    fn cancel_token_is_valid(&self, seq: u64, index: usize) -> bool {
        if self.ops.len() <= index {
            return false;
        }

        let op = &self.ops[index];
        let op = match op {
            None => return false,
            Some(ptr) => ptr,
        };

        return op.ptr.seq == seq;
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
            let mut requested = std::mem::replace(&mut req.op, IOUringOp::InProgress((rop.seq_number(), index)));

            unsafe {
                let parameters = &mut rop.ptr.parameters;
                match requested {
                    IOUringOp::Nop() => {
                        io_uring_prep_nop(sqe.ptr);
                    },
                    IOUringOp::Close(ref mut fd) => {
                        io_uring_prep_close(sqe.ptr, fd.take_fd());
                    },
                    IOUringOp::Open(path, flags, mode) => {
                        parameters.path = path;

                        io_uring_prep_openat(sqe.ptr, libc::AT_FDCWD, parameters.path.as_ptr(), flags, mode);
                    },
                    IOUringOp::Read(fd, buffer, offset) => {
                        parameters.buffer = buffer;

                        io_uring_prep_read(sqe.ptr, fd, parameters.buffer.as_mut_ptr() as *mut libc::c_void, parameters.buffer.capacity() as u32, offset.unwrap_or(u64::MAX));
                    },
                    IOUringOp::Write(fd, buffer, offset) => {
                        parameters.buffer = buffer;

                        io_uring_prep_write(sqe.ptr, fd, parameters.buffer.as_ptr() as *mut libc::c_void, parameters.buffer.size() as u32, offset.unwrap_or(u64::MAX));
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
                    IOUringOp::Cancel(seq, index) => {
                        let user_data = match self.cancel_token_is_valid(seq, index) {
                            true => index as u64,
                            false => CQE_INVALID,
                        };

                        io_uring_prep_cancel64(sqe.ptr, user_data, 0);
                    },
                    IOUringOp::SleepUpdate((seq, index), timeout) => {
                        parameters.timeout.tv_sec = timeout.as_secs() as i64;
                        parameters.timeout.tv_nsec = timeout.subsec_nanos() as i64;
                        req.timeout = None; // timeout on sleep makes no sense, and more importantly, uses same timeout field in parameters struct

                        let user_data = match self.cancel_token_is_valid(seq, index) {
                            true => index as u64,
                            false => CQE_INVALID,
                        };

                        io_uring_prep_timeout_update(sqe.ptr, &mut parameters.timeout, user_data, 0);
                    },
                    IOUringOp::Poll(fd, mask) => {
                        io_uring_prep_poll_add(sqe.ptr, fd, mask.into())
                    },
                    IOUringOp::PollUpdate((seq, index), mask) => {
                        let user_data = match self.cancel_token_is_valid(seq, index) {
                            true => index as u64,
                            false => CQE_INVALID,
                        };

                        io_uring_prep_poll_update(sqe.ptr, user_data, user_data, mask.into(), IORING_POLL_UPDATE_EVENTS);
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
            CQE_INVALID => (),
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

