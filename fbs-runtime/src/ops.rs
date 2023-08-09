use super::AsyncOp;
use super::ReactorOp;
use std::path::Path;

pub fn async_nop() -> AsyncOp {
    let mut op = ReactorOp::new();
    op.prepare_nop();

    AsyncOp(op, None)
}

pub fn async_close(fd: i32) -> AsyncOp {
    let mut op = ReactorOp::new();
    op.prepare_close(fd);

    AsyncOp(op, None)
}

pub fn async_open<P: AsRef<Path>>(path: P) -> AsyncOp {
    let mut op = ReactorOp::new();
    op.prepare_openat2(path.as_ref().as_os_str(), libc::O_CREAT | libc::O_RDWR, 0);

    AsyncOp(op, None)
}

