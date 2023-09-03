use std::os::fd::{OwnedFd, FromRawFd, IntoRawFd};
use std::os::unix::prelude::OsStrExt;
use std::path::Path;
use std::ffi::CString;

use super::AsyncOp;
use super::IOUringOp;
use super::OpenMode;
use super::SocketDomain;
use super::SocketType;
use super::SocketFlags;
use super::AsyncOpResult;
use super::IoUringCQE;
use super::ReactorOpParameters;

pub struct ResultErrno {
}

impl AsyncOpResult for ResultErrno {
    type Output = Result<i32, i32>;

    fn get_result(cqe: IoUringCQE, _params: ReactorOpParameters) -> Self::Output {
        let result: Result<i32, i32> = if cqe.result >= 0 {
            Ok(cqe.result)
        } else {
            Err(-cqe.result)
        };

        result
    }
}

pub struct ResultDescriptor {
}

impl AsyncOpResult for ResultDescriptor {
    type Output = Result<OwnedFd, i32>;

    fn get_result(cqe: IoUringCQE, _params: ReactorOpParameters) -> Self::Output {
        let result: Result<OwnedFd, i32> = if cqe.result >= 0 {
            Ok(unsafe { OwnedFd::from_raw_fd(cqe.result) } )
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

pub type AsyncNop = AsyncOp::<ResultErrno>;
pub type AsyncClose = AsyncOp::<ResultErrno>;
pub type AsyncOpen = AsyncOp::<ResultDescriptor>;
pub type AsyncSocket = AsyncOp::<ResultErrno>;
pub type AsyncRead = AsyncOp::<ResultBuffer>;
pub type AsyncWrite = AsyncOp::<ResultBuffer>;
pub type AsyncAccept = AsyncOp::<ResultDescriptor>;

pub fn async_nop() -> AsyncNop {
    AsyncOp::new(IOUringOp::Nop())
}

pub fn async_close_raw_fd(fd: i32) -> AsyncClose {
    AsyncOp::new(IOUringOp::Close(fd))
}

pub fn async_close(fd: OwnedFd) -> AsyncClose {
    AsyncOp::new(IOUringOp::Close(fd.into_raw_fd()))
}

pub fn async_open<P: AsRef<Path>>(path: P, options: &OpenMode) -> AsyncOpen {
    let path = CString::new(path.as_ref().as_os_str().as_bytes()).expect("Null character in filename");
    AsyncOp::new(IOUringOp::Open(path, options.flags(), options.mode()))
}

pub fn async_socket(domain: SocketDomain, socket_type: SocketType, options: i32) -> AsyncSocket {
    AsyncOp::new(IOUringOp::Socket(domain as i32, socket_type as i32 | options, 0))
}

pub fn async_read(fd: i32, buffer: Vec<u8>) -> AsyncRead {
    AsyncOp::new(IOUringOp::Read(fd, buffer, None))
}

pub fn async_write(fd: i32, buffer: Vec<u8>) -> AsyncWrite {
    AsyncOp::new(IOUringOp::Write(fd, buffer, None))
}

pub fn async_accept(fd: i32, flags: i32) -> AsyncAccept {
    AsyncOp::new(IOUringOp::Accept(fd, flags))
}
