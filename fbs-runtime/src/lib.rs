use std::future::Future;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::cell::Cell;
use std::slice;
use std::task::{Context, Poll};
use std::marker::PhantomData;
use thiserror::Error;

use fbs_library::open_mode::OpenMode;
use fbs_library::socket::*;
use fbs_executor::*;
use fbs_reactor::*;

mod ops;
mod linked_ops;

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
    REACTOR.with(|r| {
        r.borrow_mut().process_ops().expect("io_uring error")
    })
}

pub trait AsyncOpResult : Unpin {
    type Output: 'static;

    fn get_result(cqe: IoUringCQE, params: ReactorOpParameters) -> Self::Output;
}

pub struct AsyncOp<T: AsyncOpResult> (IOUringReq, Rc<Cell<Option<T::Output>>>, PhantomData<T>);

impl<T: AsyncOpResult> AsyncOp<T> {
    fn new(op: IOUringOp) -> Self {
        let req = IOUringReq {
            completion: None,
            op,
        };

        Self(req, Rc::new(Cell::new(None)), PhantomData)
    }

    pub fn schedule(mut self, handler: impl Fn(T::Output) + 'static) {
        self.0.completion = Some(Box::new(move |cqe, params| {
            handler(T::get_result(cqe, params));
        }));

        REACTOR.with(|r| {
            r.borrow_mut().schedule_linked2(slice::from_mut(&mut self.0))
        });
    }
}

impl<T: AsyncOpResult> Future for AsyncOp<T> {
    type Output = T::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.0.op {
            IOUringOp::InProgress(rop) => {
                match rop.result_code() {
                    Some(_) => Poll::Ready(self.1.take().unwrap()),
                    None => Poll::Pending,
                }
            },
            _ => {
                let waker = cx.waker().clone();
                let result = self.1.clone();

                self.0.completion = Some(Box::new(move |cqe, params| {
                    result.set(Some(T::get_result(cqe, params)));
                    waker.wake_by_ref();
                }));

                REACTOR.with(|r| {
                    r.borrow_mut().schedule_linked2(slice::from_mut(&mut self.0))
                });

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
    use std::os::fd::{OwnedFd, FromRawFd, AsFd};

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
    fn local_openat2_and_close_test() {
        let result = async_run(async {
            let mut options = OpenMode::new();
            options.create(true, 0o777);

            let result = async_open("/tmp/testowy-uring.txt", &options).await;
            assert!(result.is_ok());

            let result = async_close(result.unwrap()).await;
            assert!(result.is_ok());

            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_close_test() {
        let result = async_run(async {
            let testfd = unsafe { OwnedFd::from_raw_fd(123) };
            let result = async_close(testfd).await;
            assert!(result.is_err());
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_close_test2() {
        let result = async_run(async {
            let testfd = unsafe { OwnedFd::from_raw_fd(0) };
            let result = async_close(testfd).await;
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
        let result = async_run(async {
            let testfd = unsafe { OwnedFd::from_raw_fd(12) };
            let data = async_read(&testfd, vec![]);
            let data = data.await;

            assert!(data.is_err());
            assert_eq!(data.err().unwrap().0, libc::EBADF);
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_linked_ops_test() {
        let result = async_run(async {
            let mut ops = AsyncLinkedOps::new();
            let testfd = unsafe { OwnedFd::from_raw_fd(12) };

            let r1 = ops.add(async_read(&testfd, vec![]));
            let r2 = ops.add(async_close(testfd));

            let succeeded = ops.await;

            assert_eq!(succeeded, false);
            assert_eq!(r1.value(), Err((libc::EBADF, vec![])));
            assert_eq!(r2.value(), Err(libc::ECANCELED));

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
            async_close(result.unwrap()).schedule(move |_| {
                called.set(true);
            });

            1
        });

        assert_eq!(called_orig.get(), true);

        // ensure it actually executed
        assert_eq!(result, 1);
    }
}
