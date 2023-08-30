use std::os::unix::prelude::OsStrExt;
use std::path::Path;
use std::ffi::CString;

use super::AsyncOp;
use super::IOUringOp;
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
    AsyncOp::new(IOUringOp::Nop())
}

pub fn async_close(fd: i32) -> AsyncOp::<ResultErrno> {
    AsyncOp::new(IOUringOp::Close(fd))
}

pub fn async_open<P: AsRef<Path>>(path: P, options: &OpenMode) -> AsyncOp::<ResultErrno> {
    let path = CString::new(path.as_ref().as_os_str().as_bytes()).expect("Null character in filename");
    AsyncOp::new(IOUringOp::Open(path, options.flags(), options.mode()))
}

pub fn async_socket<'op>(domain: SocketDomain, socket_type: SocketType, options: SocketOptions) -> AsyncOp::<ResultErrno> {
    AsyncOp::new(IOUringOp::Socket(domain as i32, socket_type as i32, options.flags()))
}

pub fn async_read<'op>(fd: i32, buffer: Vec<u8>) -> AsyncOp::<ResultBuffer> {
    AsyncOp::new(IOUringOp::Read(fd, buffer, None))
}

pub fn async_write<'op>(fd: i32, buffer: Vec<u8>) -> AsyncOp::<ResultBuffer> {
    AsyncOp::new(IOUringOp::Write(fd, buffer, None))
}
