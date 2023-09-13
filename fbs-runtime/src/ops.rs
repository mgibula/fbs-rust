use std::os::fd::{OwnedFd, FromRawFd, IntoRawFd, AsRawFd};
use std::os::unix::prelude::OsStrExt;
use std::path::Path;
use std::ffi::CString;

use super::AsyncOp;
use super::IOUringOp;
use super::OpenMode;
use super::SocketDomain;
use super::SocketType;
use super::AsyncOpResult;
use super::IoUringCQE;
use super::ReactorOpParameters;

use fbs_library::system_error::SystemError;
use fbs_library::socket::Socket;
use fbs_library::socket_address::SocketIpAddress;

pub struct ResultErrno;

trait AsyncResultEx {
    fn cancelled(&self) -> bool;
    fn timed_out(&self) -> bool;
}

impl<T> AsyncResultEx for Result<T, SystemError> {
    fn cancelled(&self) -> bool {
        self.as_ref().is_err_and(|e| e.cancelled())
    }

    fn timed_out(&self) -> bool {
        self.as_ref().is_err_and(|e| e.timed_out())
    }
}

impl<T> AsyncResultEx for Result<T, (SystemError, Vec<u8>)> {
    fn cancelled(&self) -> bool {
        self.as_ref().is_err_and(|e| e.0.cancelled())
    }

    fn timed_out(&self) -> bool {
        self.as_ref().is_err_and(|e| e.0.timed_out())
    }
}

impl AsyncOpResult for ResultErrno {
    type Output = Result<i32, SystemError>;

    fn get_result(cqe: IoUringCQE, _params: ReactorOpParameters) -> Self::Output {
        let result = if cqe.result >= 0 {
            Ok(cqe.result)
        } else {
            Err(SystemError::new(-cqe.result))
        };

        result
    }
}

pub struct ResultDescriptor;

impl AsyncOpResult for ResultDescriptor {
    type Output = Result<OwnedFd, SystemError>;

    fn get_result(cqe: IoUringCQE, _params: ReactorOpParameters) -> Self::Output {
        let result = if cqe.result >= 0 {
            Ok(unsafe { OwnedFd::from_raw_fd(cqe.result) } )
        } else {
            Err(SystemError::new(-cqe.result))
        };

        result
    }
}

pub struct ResultSocket;

impl AsyncOpResult for ResultSocket {
    type Output = Result<Socket, SystemError>;

    fn get_result(cqe: IoUringCQE, _params: ReactorOpParameters) -> Self::Output {
        let result = if cqe.result >= 0 {
            Ok(unsafe { Socket::from_raw_fd(cqe.result) } )
        } else {
            Err(SystemError::new(-cqe.result))
        };

        result
    }
}

pub struct ResultBuffer;

impl AsyncOpResult for ResultBuffer {
    type Output = Result<Vec<u8>, (SystemError, Vec<u8>)>;

    fn get_result(cqe: IoUringCQE, params: ReactorOpParameters) -> Self::Output {
        let mut buffer = params.buffer;
        let result = if cqe.result >= 0 {
            buffer.resize(cqe.result as usize, 0);
            Ok(buffer)
        } else {
            Err((SystemError::new(-cqe.result), buffer))
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
pub type AsyncAccept = AsyncOp::<ResultSocket>;
pub type AsyncConnect = AsyncOp::<ResultErrno>;

pub fn async_nop() -> AsyncNop {
    AsyncOp::new(IOUringOp::Nop())
}

pub fn async_close<T: IntoRawFd>(fd: T) -> AsyncClose {
    AsyncOp::new(IOUringOp::Close(fd.into_raw_fd()))
}

pub fn async_open<P: AsRef<Path>>(path: P, options: &OpenMode) -> AsyncOpen {
    let path = CString::new(path.as_ref().as_os_str().as_bytes()).expect("Null character in filename");
    AsyncOp::new(IOUringOp::Open(path, options.flags(), options.mode()))
}

pub fn async_socket(domain: SocketDomain, socket_type: SocketType, options: i32) -> AsyncSocket {
    AsyncOp::new(IOUringOp::Socket(domain as i32, socket_type as i32 | options, 0))
}

pub fn async_read<T: AsRawFd>(fd: &T, buffer: Vec<u8>) -> AsyncRead {
    AsyncOp::new(IOUringOp::Read(fd.as_raw_fd(), buffer, None))
}

pub fn async_write<T: AsRawFd>(fd: &T, buffer: Vec<u8>) -> AsyncWrite {
    AsyncOp::new(IOUringOp::Write(fd.as_raw_fd(), buffer, None))
}

pub fn async_accept<T: AsRawFd>(fd: &T, flags: i32) -> AsyncAccept {
    AsyncOp::new(IOUringOp::Accept(fd.as_raw_fd(), flags))
}

pub fn async_connect<T: AsRawFd>(fd: &T, address: SocketIpAddress) -> AsyncConnect {
    AsyncOp::new(IOUringOp::Connect(fd.as_raw_fd(), address))
}
