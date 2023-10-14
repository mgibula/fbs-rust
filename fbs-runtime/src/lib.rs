#[macro_use] extern crate const_cstr;

use std::future::Future;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::cell::Cell;
use std::slice;
use std::task::{Context, Poll};
use std::time::Duration;
use thiserror::Error;

use fbs_library::open_mode::OpenMode;
use fbs_library::socket::*;
use fbs_executor::*;
use fbs_reactor::*;

mod ops;
mod linked_ops;

pub mod async_utils;
pub mod http_client;
pub mod resolver;

pub use ops::*;
pub use linked_ops::*;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("reactor error")]
    ReactorError(#[from] ReactorError),
}

thread_local! {
    static EXECUTOR: RefCell<Executor> = RefCell::new(Executor::new());
    static FRONTEND: ExecutorFrontend = EXECUTOR.with(|e| {
        e.borrow().get_frontend()
    });
    static REACTOR: RefCell<Reactor> = RefCell::new(Reactor::new().expect("Error creating io_uring reactor"));
    static COMPLETIONS: RefCell<Vec<Box<dyn FnOnce()>>> = RefCell::new(Vec::new());
}

pub fn async_spawn<T: 'static>(future: impl Future<Output = T> + 'static) -> TaskHandle<T>  {
    FRONTEND.with(|e| {
        e.spawn(future)
    })
}

pub fn async_yield() -> Yield {
    FRONTEND.with(|e| {
        e.yield_execution()
    })
}

pub fn async_op_supported(opcode: u32) -> bool {
    REACTOR.with(|r| {
        r.borrow().is_supported(opcode)
    })
}

pub fn async_run<T: 'static>(future: impl Future<Output = T> + 'static) -> T {
    let handle = async_spawn(future);

    loop {
        local_executor_run_all();
        let made_progress = local_reactor_process_ops();
        if !made_progress {
            break;
        }
    }

    handle.result().unwrap()
}

fn local_executor_run_all() {
    EXECUTOR.with(|e| {
        let mut e = e.borrow_mut();
        while e.has_ready_tasks() {
            e.run_all();
        }
    });
}

fn local_reactor_process_ops() -> bool {
    let processed = REACTOR.with(|r| {
        r.borrow_mut().process_ops().expect("io_uring error")
    });

    let completions = COMPLETIONS.with(|c| std::mem::take(&mut *c.borrow_mut()));
    completions.into_iter().for_each(|f| f());

    processed
}

pub trait AsyncOpResult : Unpin {
    type Output: 'static;

    fn get_result(cqe: IoUringCQE, params: ReactorOpParameters) -> Self::Output;
}

pub enum AsyncValue<T> {
    InProgress,
    Stored(T),
    Completed,
}

impl<T> AsyncValue<T> {
    pub fn as_option(self) -> Option<T> {
        match self {
            AsyncValue::Stored(value) => Some(value),
            _ => None,
        }
    }
}

// iouring request, result, auto-cancel flag, submit-immediately
pub struct AsyncOp<T: AsyncOpResult> (IOUringReq, Rc<Cell<AsyncValue<T::Output>>>, bool, bool);

impl<T: AsyncOpResult> Drop for AsyncOp<T> {
    fn drop(&mut self) {
        // check if auto cancel is desired
        if !self.2 {
            return;
        }

        // short-circuit to check if op has already been completed
        match self.1.replace(AsyncValue::Completed) {
            AsyncValue::InProgress => (),
            _ => return,
        }

        match self.0.op {
            IOUringOp::InProgress(cancel) => {
                REACTOR.with(|r| {
                    r.borrow_mut().cancel_op(slice::from_ref(&(cancel.0, cancel.1)));
                });
            },
            _ => ()
        }

    }
}

impl<T: AsyncOpResult> AsyncOp<T> {
    fn new(op: IOUringOp) -> Self {
        let req = IOUringReq {
            op,
            completion: None,
            timeout: None,
        };

        Self(req, Rc::new(Cell::new(AsyncValue::InProgress)), false, false)
    }

    pub fn schedule(mut self, handler: impl FnOnce(T::Output) + 'static) -> (u64, usize) {

        self.0.completion = Some(Box::new(move |cqe, params| {
            COMPLETIONS.with(|c| {
                c.borrow_mut().push(Box::new(move || handler(T::get_result(cqe, params))));
            });
        }));

        let immediately = self.3;
        REACTOR.with(|r| {
            r.borrow_mut().schedule_linked2(slice::from_mut(&mut &mut self.0));

            if immediately {
                r.borrow_mut().submit().expect("io_uring error");
            }
        });

        match &self.0.op {
            &IOUringOp::InProgress(cancel) => cancel,
            _ => panic!("io_uring schedling failed"),
        }
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.0.timeout = Some(timeout);
        self
    }

    pub fn clear_timeout(mut self) -> Self {
        self.0.timeout = None;
        self
    }

    pub fn submit_immediately(mut self, value: bool) -> Self {
        self.3 = value;
        self
    }
}

impl<T: AsyncOpResult> Future for AsyncOp<T> {
    type Output = T::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.0.op {
            IOUringOp::InProgress(_) => {
                match self.1.replace(AsyncValue::InProgress) {
                    AsyncValue::InProgress => Poll::Pending,
                    AsyncValue::Stored(value) => { self.1.set(AsyncValue::Completed); Poll::Ready(value) },
                    AsyncValue::Completed => panic!("Pooling completed op"),
                }
            },
            _ => {
                let waker = cx.waker().clone();
                let result = self.1.clone();

                self.0.completion = Some(Box::new(move |cqe, params| {
                    result.set(AsyncValue::Stored(T::get_result(cqe, params)));
                    waker.wake_by_ref();
                }));

                REACTOR.with(|r| {
                    r.borrow_mut().schedule_linked2(slice::from_mut(&mut &mut self.0))
                });

                self.2 = true;
                Poll::Pending
            },
        }
    }
}

#[cfg(test)]
fn async_run_once() -> bool {
    EXECUTOR.with(|e| {
        e.borrow_mut().run_once()
    })
}

#[cfg(test)]
fn async_run_all() {
    EXECUTOR.with(|e| {
        e.borrow_mut().run_all();
    })
}

#[cfg(test)]
mod tests {
    use std::os::fd::{OwnedFd, FromRawFd};

    use fbs_library::poll::PollMask;

    use super::*;

    #[test]
    fn local_executor_test() {
        let handle1 =  async_spawn(async {
            return 123;
        });

        let executed = async_run_once();
        assert_eq!(executed, true);
        assert_eq!(handle1.is_completed(), true);
        assert_eq!(handle1.result(), Some(123));
    }

    #[test]
    fn local_yield_test() {
        let handle1 = async_spawn(async {
            async_yield().await
        });

        let handle2 = async_spawn(async {
            async_yield().await
        });

        assert_eq!(handle1.is_completed(), false);
        assert_eq!(handle2.is_completed(), false);

        async_run_once();
        assert_eq!(handle1.is_completed(), false);
        assert_eq!(handle2.is_completed(), false);

        async_run_all();
        assert_eq!(handle1.is_completed(), true);
        assert_eq!(handle2.is_completed(), true);
    }

    #[test]
    fn local_nop_test() {
        let result = async_run(async {
            let result = async_nop().await;
            assert_eq!(result, Ok(0));

            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_openat2_test() {
        let result = async_run(async {
            let result = async_open("/tmp/testowy-uring.txt", OpenMode::new().create(true, 0o777)).await;
            assert!(result.is_ok());
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_openat2_and_write_test() {
        #[repr(C, packed)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        struct TestStruct(u8, u8, u8, u8);

        let result = async_run(async {
            let result = async_open("/tmp/testowy-uring.txt", OpenMode::new().create(true, 0o777)).await;
            assert!(result.is_ok());

            let content: Vec<u8> = vec![116, 101, 115, 116];
            let fd = &result.unwrap();

            let result = async_write(fd, content, None).await;
            assert!(result.is_ok());
            assert!(result.unwrap().len() == 4);

            let content: Vec<u8> = Vec::with_capacity(10);
            let result = async_read_into(fd, content, Some(0)).await;
            assert!(result.is_ok());
            let read_content = result.unwrap();
            assert_eq!(read_content.len(), 4);
            assert_eq!(read_content.capacity(), 10);
            assert_eq!(read_content, vec![116, 101, 115, 116]);

            let result = async_read_struct::<TestStruct>(fd, Some(0)).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), TestStruct { 0: 116, 1: 101, 2: 115, 3: 116});
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }
    #[test]
    fn local_openat2_and_close_test() {
        let result = async_run(async {
            let mut options = OpenMode::new();
            options.create(true, 0o777);

            let result = async_open("/tmp/testowy-uring.txt", &options).await;
            assert!(result.is_ok());

            let result = async_close_with_result(result.unwrap()).await;
            assert!(result.is_ok());

            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_close_test() {
        let result = async_run(async {
            let result = async_close_with_result(-1).await;
            assert!(result.is_err());
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_close_test2() {
        let result = async_run(async {
            let testfd = unsafe { OwnedFd::from_raw_fd(libc::dup(0)) };
            let result = async_close_with_result(testfd).await;
            assert!(result.is_ok());
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_socket_test() {
        let result = async_run(async {
            if async_op_supported(IOUringOpType::SOCKET) {
                let op = async_socket(SocketDomain::Inet, SocketType::Stream, SocketFlags::new().flags());
                let sockfd = op.await;
                assert!(sockfd.is_ok());
            }
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_read_test() {
        use fbs_library::system_error::SystemError;

        let result = async_run(async {
            let data = async_read_into(&-1, vec![], None);
            let data = data.await;

            assert!(data.is_err());
            assert_eq!(data.err().unwrap().0, SystemError::new(libc::EBADF));
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_linked_ops_test() {
        use fbs_library::system_error::SystemError;

        let result = async_run(async {
            let mut ops = AsyncLinkedOps::new();

            let r1 = ops.add(async_read_into(&-1, vec![], None));
            let r2 = ops.add(async_close_with_result(-1));

            let succeeded = ops.await;

            assert_eq!(succeeded, false);
            assert_eq!(r1.value(), Err((SystemError::new(libc::EBADF), vec![])));
            assert!(r2.value().is_err_and(|e| e.cancelled()));

            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_schedule_close() {
        let called = Rc::new(Cell::new(false));
        let called_orig = called.clone();

        let result = async_run(async move {
            let mut options = OpenMode::new();
            options.create(true, 0o777);

            let result = async_open("/tmp/testowy-uring.txt", &options).await;
            assert!(result.is_ok());

            let called = called.clone();
            async_close_with_result(result.unwrap()).schedule(move |result| {
                assert!(result.is_ok());
                called.set(true);
            });

            1
        });

        assert_eq!(called_orig.get(), true);

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_schedule_timeout() {
        let called = Rc::new(Cell::new(false));
        let called_orig = called.clone();

        let result = async_run(async move {
            let called = called.clone();
            async_sleep_with_result(std::time::Duration::new(0, 1_000_000)).schedule(move |result| {
                assert!(result.is_ok());
                called.set(true);
            });

            1
        });

        assert_eq!(called_orig.get(), true);

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_schedule_timeout_and_cancel() {
        let called = Rc::new(Cell::new(false));
        let called_orig = called.clone();

        let result = async_run(async move {
            let called = called.clone();
            let token = async_sleep_with_result(std::time::Duration::new(0, 1_000_000)).schedule(move |result| {
                assert!(result.is_err_and(|r| r.cancelled()));
                called.set(true);
            });

            async_cancel(token).await;
            1
        });

        assert_eq!(called_orig.get(), true);

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_timeout_test() {
        use std::time::{Duration, SystemTime};

        let now = SystemTime::now();

        let result = async_run(async {
            let op = async_sleep_with_result(Duration::new(0, 1_000_000));
            let result = op.await;

            assert!(result.is_ok());

            1
        });

        let elapsed = now.elapsed();
        assert!(elapsed.is_ok_and(|e| e.as_nanos() >= 1_000_000));

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_read_timeout_test() {
        let result = async_run(async {
            let testfd = unsafe { OwnedFd::from_raw_fd(libc::dup(0)) };
            let mut buffer = Vec::new();
            buffer.resize(100, 0);

            let data = async_read_into(&testfd, buffer, None).timeout(Duration::new(0, 1_000_000));
            let data = data.await;

            assert!(data.is_err());
            assert!(data.err().unwrap().0.cancelled());
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_read_timeout_test_notimeout() {
        let result = async_run(async {
            let mut buffer = Vec::new();
            buffer.resize(100, 0);

            let data = async_read_into(&-1, buffer, None).timeout(Duration::new(0, 1_000_000));
            let data = data.await;

            assert!(data.is_err());
            assert_eq!(data.err().unwrap().0.errno(), libc::EBADF);
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_cancel() {
        use std::time::{Duration, SystemTime};

        let result = async_run(async {
            let handle1 = async_spawn(async {
                async_sleep(Duration::new(4, 0)).await;
            });

            let now = SystemTime::now();
            async_sleep(Duration::new(0, 1_000_000)).await;

            handle1.cancel();
            let elapsed = now.elapsed();
            assert!(elapsed.is_ok_and(|e| e.as_secs() < 1));

            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_linked_cancel_test() {
        use std::time::{Duration, SystemTime};

        // cancelling first op
        let result = async_run(async {
            let handle1 = async_spawn(async {
                let testfd = unsafe { OwnedFd::from_raw_fd(libc::dup(0)) };
                let mut buffer = Vec::new();
                buffer.resize(100, 0);

                let mut ops = AsyncLinkedOps::new();

                ops.add(async_read_into(&testfd, buffer, None));
                ops.add(async_sleep(Duration::new(5, 0)));

                ops.await;
            });

            let now = SystemTime::now();
            async_sleep(Duration::new(0, 1_000_000)).await;

            handle1.cancel();
            let elapsed = now.elapsed();
            assert!(elapsed.is_ok_and(|e| e.as_secs() < 1));

            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_linked_cancel_test2() {
        use std::time::{Duration, SystemTime};

        // cancelling second op
        let result = async_run(async {
            let handle1 = async_spawn(async {
                let testfd = unsafe { OwnedFd::from_raw_fd(libc::dup(0)) };
                let mut buffer = Vec::new();
                buffer.resize(100, 0);

                let mut ops = AsyncLinkedOps::new();

                ops.add(async_close(testfd));
                ops.add(async_sleep(Duration::new(5, 0)));

                ops.await;
            });

            let now = SystemTime::now();
            async_sleep(Duration::new(0, 1_000_000)).await;

            handle1.cancel();
            let elapsed = now.elapsed();
            assert!(elapsed.is_ok_and(|e| e.as_secs() < 1));

            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_schedule_timeout_update() {
        use std::time::SystemTime;

        let called = Rc::new(Cell::new(false));
        let called_orig = called.clone();

        let now = SystemTime::now();
        let result = async_run(async move {
            let called = called.clone();
            let token = async_sleep_with_result(std::time::Duration::new(5, 0)).schedule(move |result| {
                assert!(result.is_ok());
                called.set(true);
            });

            async_sleep_update(token, std::time::Duration::new(0, 1_000_000)).await;

            1
        });

        let elapsed = now.elapsed();
        assert!(elapsed.is_ok_and(|e| e.as_secs() < 1));

        assert_eq!(called_orig.get(), true);

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_poll_df() {
        use std::time::SystemTime;
        use fbs_library::pipe::*;

        let called = Rc::new(Cell::new(false));
        let called_orig = called.clone();
        let (rx, tx) = pipe(PipeFlags::default()).unwrap();

        let now = SystemTime::now();
        async_run(async move {
            let called = called.clone();
            async_spawn(async move {
                let mut data = vec![];
                data.extend_from_slice(b"test");

                async_poll(&tx, PollMask::default().write(true)).await;
                async_write(&tx, data, None).await;

                1
            });

            async_spawn(async move {
                async_poll(&rx, PollMask::default().read(true)).await;

                let mut buffer = Vec::with_capacity(10);
                let result = async_read_into(&rx, buffer, None).await.unwrap();
                called.set(true);

                assert_eq!(result, b"test");
            });
        });

        assert_eq!(called_orig.get(), true);
    }

}
