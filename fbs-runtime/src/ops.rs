use std::marker::PhantomData;
use std::os::fd::{OwnedFd, FromRawFd, IntoRawFd, AsRawFd};
use std::os::unix::prelude::OsStrExt;
use std::path::Path;
use std::ffi::CString;
use std::time::Duration;

use super::AsyncOp;
use super::IOUringOp;
use super::OpenMode;
use super::SocketDomain;
use super::SocketType;
use super::AsyncOpResult;
use super::IoUringCQE;
use super::ReactorOpParameters;
use super::Buffer;
use super::MaybeFd;

use fbs_library::system_error::SystemError;
use fbs_library::socket::Socket;
use fbs_library::socket_address::SocketIpAddress;
use fbs_library::poll::PollMask;

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

pub struct ResultSuccess;

impl AsyncOpResult for ResultSuccess {
    type Output = ();

    fn get_result(cqe: IoUringCQE, _params: ReactorOpParameters) -> Self::Output {
        match cqe.result {
            result if result == 0 => (),
            result if result == -libc::ECANCELED => (),
            result => println!("Ignoring CQE result of {}", result),
        }
    }
}

pub struct ResultSuccessSleep;

impl AsyncOpResult for ResultSuccessSleep {
    type Output = ();

    fn get_result(cqe: IoUringCQE, _params: ReactorOpParameters) -> Self::Output {
        match cqe.result {
            result if result == 0 => (),
            result if result == -libc::ETIME => (),
            result if result == -libc::ECANCELED => (),
            result => println!("Ignoring CQE result of {}", result),
        }
    }
}

pub struct ResultErrno;

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

pub struct ResultErrnoTimeout;

impl AsyncOpResult for ResultErrnoTimeout {
    type Output = Result<i32, SystemError>;

    fn get_result(cqe: IoUringCQE, _params: ReactorOpParameters) -> Self::Output {
        match cqe.result {
            i if cqe.result >= 0 => Ok(i),
            _i if cqe.result == -libc::ETIME => Ok(0),
            i => Err(SystemError::new(-i))
        }
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
        let buffer = params.buffer;

        let result = if cqe.result >= 0 {
            let buffer = unsafe { buffer.to_vec(cqe.result as usize) };
            Ok(buffer)
        } else {
            let buffer = unsafe { buffer.to_vec(0) };
            Err((SystemError::new(-cqe.result), buffer))
        };

        result
    }
}

pub struct ResultStruct<T: Copy + Unpin> {
    data: PhantomData<T>,
}

impl<T: Copy + Unpin + 'static> AsyncOpResult for ResultStruct<T> {
    type Output = Result<T, SystemError>;

    fn get_result(cqe: IoUringCQE, params: ReactorOpParameters) -> Self::Output {
        let buffer = params.buffer;

        let result = if cqe.result == std::mem::size_of::<T>() as i32 {
            Ok(unsafe { buffer.to_struct::<T>(cqe.result as usize) })
        } else if cqe.result > 0 {
            Err(SystemError::new(libc::ENOENT))
        } else {
            Err(SystemError::new(-cqe.result))
        };

        result
    }
}

pub type AsyncNop = AsyncOp::<ResultErrno>;
pub type AsyncClose = AsyncOp::<ResultSuccess>;
pub type AsyncCloseWithResult = AsyncOp::<ResultErrno>;
pub type AsyncOpen = AsyncOp::<ResultDescriptor>;
pub type AsyncSocket = AsyncOp::<ResultErrno>;
pub type AsyncReadBytes = AsyncOp::<ResultBuffer>;
pub type AsyncReadStruct<T> = AsyncOp::<ResultStruct<T>>;
pub type AsyncWrite = AsyncOp::<ResultBuffer>;
pub type AsyncAccept = AsyncOp::<ResultSocket>;
pub type AsyncConnect = AsyncOp::<ResultErrno>;
pub type AsyncTimeout = AsyncOp::<ResultSuccessSleep>;
pub type AsyncTimeoutWithResult = AsyncOp::<ResultErrnoTimeout>;
pub type AsyncCancel = AsyncOp::<ResultErrno>;
pub type AsyncPoll = AsyncOp::<ResultErrno>;

pub fn async_nop() -> AsyncNop {
    AsyncOp::new(IOUringOp::Nop())
}

pub fn async_close<T: IntoRawFd>(fd: T) -> AsyncClose {
    AsyncOp::new(IOUringOp::Close(MaybeFd::new(fd)))
}

pub fn async_close_with_result<T: IntoRawFd>(fd: T) -> AsyncCloseWithResult {
    AsyncOp::new(IOUringOp::Close(MaybeFd::new(fd)))
}

pub fn async_open<P: AsRef<Path>>(path: P, options: &OpenMode) -> AsyncOpen {
    let path = CString::new(path.as_ref().as_os_str().as_bytes()).expect("Null character in filename");
    AsyncOp::new(IOUringOp::Open(path, options.flags(), options.mode()))
}

pub fn async_socket(domain: SocketDomain, socket_type: SocketType, options: i32) -> AsyncSocket {
    AsyncOp::new(IOUringOp::Socket(domain as i32, socket_type as i32 | options, 0))
}

pub fn async_read_into<T: AsRawFd>(fd: &T, buffer: Vec<u8>, offset: Option<u64>) -> AsyncReadBytes {
    AsyncOp::new(IOUringOp::Read(fd.as_raw_fd(), Buffer::from_vec(buffer), offset))
}

pub fn async_read_struct<U: Copy + Unpin + 'static>(fd: &impl AsRawFd, offset: Option<u64>) -> AsyncReadStruct<U> {
    AsyncOp::new(IOUringOp::Read(fd.as_raw_fd(), Buffer::new_struct::<U>(), offset))
}

pub fn async_write<T: AsRawFd>(fd: &T, buffer: Vec<u8>, offset: Option<u64>) -> AsyncWrite {
    AsyncOp::new(IOUringOp::Write(fd.as_raw_fd(), Buffer::from_vec(buffer), offset))
}

pub fn async_write_struct<U: Copy + Unpin + 'static>(fd: &impl AsRawFd, value: U, offset: Option<u64>) -> AsyncWrite {
    AsyncOp::new(IOUringOp::Write(fd.as_raw_fd(), Buffer::new_struct_from(value), offset))
}

pub fn async_accept<T: AsRawFd>(fd: &T, flags: i32) -> AsyncAccept {
    AsyncOp::new(IOUringOp::Accept(fd.as_raw_fd(), flags))
}

pub fn async_connect<T: AsRawFd>(fd: &T, address: SocketIpAddress) -> AsyncConnect {
    AsyncOp::new(IOUringOp::Connect(fd.as_raw_fd(), address))
}

pub fn async_sleep(timeout: Duration) -> AsyncTimeout {
    AsyncOp::new(IOUringOp::Sleep(timeout))
}

pub fn async_sleep_with_result(timeout: Duration) -> AsyncTimeoutWithResult {
    AsyncOp::new(IOUringOp::Sleep(timeout))
}

pub fn async_cancel(token: (u64, usize)) -> AsyncCancel {
    AsyncOp::new(IOUringOp::Cancel(token.0, token.1)).submit_immediately(true)
}

pub fn async_sleep_update(token: (u64, usize), timeout: Duration) -> AsyncTimeoutWithResult {
    AsyncOp::new(IOUringOp::SleepUpdate(token, timeout))
}

pub fn async_poll<T: AsRawFd>(fd: &T, mask: PollMask) -> AsyncPoll {
    AsyncOp::new(IOUringOp::Poll(fd.as_raw_fd(), mask))
}

pub fn async_poll_update(token: (u64, usize), mask: PollMask) -> AsyncPoll {
    AsyncOp::new(IOUringOp::PollUpdate(token, mask))
}
