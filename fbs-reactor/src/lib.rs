use std::cell::RefCell;
use std::ffi::{CString, OsStr};
use std::rc::Rc;
use std::mem::{self, ManuallyDrop};
use std::task::Waker;
use std::ops::{Drop, DerefMut};
use std::os::unix::prelude::OsStrExt;
use std::pin::Pin;

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
    Buffer,
}

#[repr(C)]
struct ReactorOpExtraParamsOpenAt2 {
    how: libc::open_how,
    path: CString,
}

#[repr(C)]
struct ReactorOpExtraParamsBuffer {
    buffer: Vec<u8>,
}

#[repr(C)]
union ReactorOpExtraParamsUnion {
    openat2: mem::ManuallyDrop<ReactorOpExtraParamsOpenAt2>,
    buffer: mem::ManuallyDrop<ReactorOpExtraParamsBuffer>,
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
                },
                ReactorOpExtraParamsType::Buffer => {
                    ManuallyDrop::drop(&mut self.value.buffer);
                }
            }
        }
    }
}

enum ReactorOpSQE {
    Unscheduled(io_uring_sqe),
    Scheduled(IoUringSQEPtr),
}

pub struct ReactorOpParameters {
    how: libc::open_how,
    path: CString,
    buffer: Vec<u8>,
}

impl ReactorOpParameters {
    pub fn new() -> Self {
        ReactorOpParameters { how: unsafe { mem::zeroed() }, path: CString::default(), buffer: Vec::default() }
    }
}

pub struct ReactorOp2 {
    sqe: ReactorOpSQE,
    parameters: ReactorOpParameters,
    cqe: Option<IoUringCQEPtr>,
    index: usize,
    waker: Option<Waker>,
}

impl ReactorOp2 {
    pub fn new() -> Self {
        ReactorOp2 {
            sqe: ReactorOpSQE::Unscheduled(unsafe { mem::zeroed() }),
            parameters: ReactorOpParameters::new(),
            cqe: None,
            index: 0,
            waker: None
        }
    }
}

pub struct ReactorOpPtr {
    ptr: Rc<RefCell<ReactorOp2>>,
}

impl ReactorOpPtr {
    pub fn new() -> Self {
        ReactorOpPtr { ptr: Rc::new(RefCell::new(ReactorOp2::new())) }
    }

    fn schedule(&self, mut target_sqe: IoUringSQEPtr, index: usize, waker: Waker) {
        let mut op = self.ptr.borrow_mut();
        match op.sqe {
            ReactorOpSQE::Unscheduled(sqe) => {
                target_sqe.copy_from(&sqe);
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
}

pub struct ReactorOp(io_uring_sqe, Option<Pin<Box<ReactorOpExtraParams>>>);

impl ReactorOp {
    pub fn new() -> Self {
        let mut result = ReactorOp {
            0: unsafe { mem::zeroed() },
            1: Some(Box::pin(ReactorOpExtraParams::new())),
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
            let mut extra_params = self.1.as_mut().unwrap();
            extra_params.tag = ReactorOpExtraParamsType::OpenAt2;
            extra_params.value.openat2 = ManuallyDrop::new(ReactorOpExtraParamsOpenAt2 { how: mem::zeroed(), path: CString::new(path.as_bytes()).expect("Null character in filename") });

            io_uring_prep_openat(&mut self.0, libc::AT_FDCWD, extra_params.value.openat2.path.as_ptr(), flags, mode);
        }
    }

    pub fn prepare_socket(&mut self, domain: i32, socket_type: i32, protocol: i32) {
        unsafe {
            io_uring_prep_socket(&mut self.0, domain, socket_type, protocol, 0);
        }
    }

    pub fn prepare_read(&mut self, fd: i32, buffer: Vec<u8>, offset: Option<u64>) {
        unsafe {
            let mut extra_params = self.1.as_mut().unwrap();
            extra_params.tag = ReactorOpExtraParamsType::Buffer;
            extra_params.value.buffer = ManuallyDrop::new(ReactorOpExtraParamsBuffer { buffer });

            let buffer = &mut extra_params.value.buffer.deref_mut().buffer;
            io_uring_prep_read(&mut self.0, fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len() as u32, offset.unwrap_or(u64::MAX));
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

    pub fn opcode(&self) -> u8 {
        // opcode is u8 field at offset zero
        unsafe {
            *(&self.0 as *const io_uring_sqe as *const u8)
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
    sqe: IoUringSQEPtr,
    cqe: Option<IoUringCQE>,
    index: usize,
    waker: Waker,
    extra_params: ReactorOpExtraParams,
}

pub struct Reactor {
    ring: IoUring,
    ops: Vec<Option<OpDescriptorPtr>>,
    ops2: Vec<Option<ReactorOpPtr>>,
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

        Ok(Reactor { ring: IoUring::new(params)?, ops: vec![], ops2: vec![], ops_free_entries: vec![], in_flight: 0, uncommited: 0 })
    }

    pub fn is_supported(&self, op: &ReactorOp) -> bool {
        self.ring.is_op_supported(op.opcode())
    }

    pub fn schedule2(&mut self, op: ReactorOpPtr, waker: Waker) -> Result<(), ReactorError> {
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
            self.ops2.push(Some(op));
        } else {
            self.ops2[index] = Some(op);
        }

        Ok(())
    }

    pub fn schedule(&mut self, op: &mut ReactorOp, waker: Waker) -> Result<OpDescriptorPtr, ReactorError> {
        if self.ring.sq_space_left() == 0 {
            self.submit();
        }

        let mut desc = self.create_op_descriptor(waker, op.1.take().unwrap())?;
        op.set_data64(desc.get_index() as u64);
        desc.copy_from(op);

        self.in_flight += 1;
        self.uncommited += 1;
        Ok(desc)
    }

    pub fn pending_ops(&self) -> i32 {
        self.in_flight
    }

    fn create_op_descriptor(&mut self, waker: Waker, extra_params: Pin<Box<ReactorOpExtraParams>>) -> Result<OpDescriptorPtr, ReactorError> {
        let index = match self.ops_free_entries.pop() {
            Some(index) => index,
            None => self.ops.len(),
        };

        let result = OpDescriptorPtr {
            ptr: Rc::new(RefCell::new(OpDescriptor { sqe: self.get_sqe()?, cqe: None, index, waker, extra_params: ReactorOpExtraParams::new() }))
        };

        if self.ops.len() == index {
            self.ops.push(Some(result.clone()));
        } else {
            self.ops[index] = Some(result.clone());
        }

        Ok(result)
    }

    fn get_sqe(&mut self) -> Result<IoUringSQEPtr, ReactorError> {
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

