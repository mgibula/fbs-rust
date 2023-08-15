use std::marker::PhantomData;
use std::path::Path;

use super::AsyncOp;
use super::ReactorOpPtr;
use super::OpenMode;
use super::SocketDomain;
use super::SocketType;
use super::SocketOptions;
use super::AsyncOpResult;
use super::IoUringCQE;
use super::ReactorOpParameters;

pub struct ResultErrno {
}

impl AsyncOpResult for ResultErrno {
    type Output = Result<i32, i32>;

    fn get_result(cqe: IoUringCQE, _params: ReactorOpParameters) -> Self::Output {
        let result = if cqe.result >= 0 {
            Ok(cqe.result)
        } else {
            Err(-cqe.result)
        };

        result
    }
}

pub struct ResultBuffer {
}

impl AsyncOpResult for ResultBuffer {
    type Output = Result<Vec<u8>, (i32, Vec<u8>)>;

    fn get_result(cqe: IoUringCQE, params: ReactorOpParameters) -> Self::Output {
        let mut buffer = params.buffer;
        let result = if cqe.result >= 0 {
            buffer.resize(cqe.result as usize, 0);
            Ok(buffer)
        } else {
            Err((-cqe.result, buffer))
        };

        result
    }
}

pub fn async_nop() -> AsyncOp::<ResultErrno> {
    let mut op = ReactorOpPtr::new();
    op.prepare_nop();

    AsyncOp(op, PhantomData)
}

pub fn async_close(fd: i32) -> AsyncOp::<ResultErrno> {
    let mut op = ReactorOpPtr::new();
    op.prepare_close(fd);

    AsyncOp(op, PhantomData)
}

pub fn async_open<P: AsRef<Path>>(path: P, options: &OpenMode) -> AsyncOp::<ResultErrno> {
    let mut op = ReactorOpPtr::new();
    op.prepare_openat2(path.as_ref().as_os_str(), options.flags(), options.mode());

    AsyncOp(op, PhantomData)
}

pub fn async_socket(domain: SocketDomain, socket_type: SocketType, options: SocketOptions) -> AsyncOp::<ResultErrno> {
    let mut op = ReactorOpPtr::new();
    op.prepare_socket(domain as i32, socket_type as i32 | options.flags(), 0);

    AsyncOp(op, PhantomData)
}

pub fn async_read(fd: i32, buffer: Vec<u8>) -> AsyncOp::<ResultBuffer> {
    let mut op = ReactorOpPtr::new();
    op.prepare_read(fd, buffer, None);

    AsyncOp(op, PhantomData)
}