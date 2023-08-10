use std::marker::PhantomData;
use std::path::Path;

use super::AsyncOp;
use super::ReactorOp;
use super::OpenMode;
use super::SocketDomain;
use super::SocketType;
use super::SocketOptions;
use super::AsyncOpResult;
use super::OpDescriptorPtr;

pub struct ResultErrno {
}

impl AsyncOpResult for ResultErrno {
    type Output = i32;

    fn get_result(op: &mut OpDescriptorPtr) -> Self::Output {
        op.get_cqe().result
    }
}

pub fn async_nop() -> AsyncOp::<ResultErrno> {
    let mut op = ReactorOp::new();
    op.prepare_nop();

    AsyncOp(op, None, PhantomData)
}

pub fn async_close(fd: i32) -> AsyncOp::<ResultErrno> {
    let mut op = ReactorOp::new();
    op.prepare_close(fd);

    AsyncOp(op, None, PhantomData)
}

pub fn async_open<P: AsRef<Path>>(path: P, options: &OpenMode) -> AsyncOp::<ResultErrno> {
    let mut op = ReactorOp::new();
    op.prepare_openat2(path.as_ref().as_os_str(), options.flags(), options.mode());

    AsyncOp(op, None, PhantomData)
}

pub fn async_socket(domain: SocketDomain, socket_type: SocketType, options: SocketOptions) -> AsyncOp::<ResultErrno> {
    let mut op = ReactorOp::new();
    op.prepare_socket(domain as i32, socket_type as i32 | options.flags(), 0);

    AsyncOp(op, None, PhantomData)
}

pub fn async_read(fd: i32, buffer: Vec<u8>) -> AsyncOp::<ResultErrno> {
    unimplemented!()
}