use std::future::Future;
use std::cell::RefCell;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;

use fbs_executor::*;
use fbs_reactor::*;

mod ops;
pub use ops::*;

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

pub struct AsyncOp (ReactorOp, Option<OpDescriptorPtr>);

impl Future for AsyncOp {
    type Output = i32;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.1 {
            // already scheduled
            Some(op) => {
                match op.completed() {
                    false => Poll::Pending,
                    true => Poll::Ready(op.get_cqe().result)
                }
            },
            // not yet scheduled
            None => {
                let scheduled = REACTOR.with(|r| {
                    r.borrow_mut().schedule(&mut self.0, cx.waker().clone())
                });

                match scheduled {
                    Err(error) => panic!("Error while scheduling async op: {}", error),
                    Ok(op) => {
                        self.1 = Some(op);
                        Poll::Pending
                    }
                }
            }
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
            assert_eq!(result, 0);

            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_openat2_test() {
        let result = async_run(async {
            let result = async_open("/tmp/testowy-uring.txt").await;
            assert!(result >= 0);
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_close_test() {
        let result = async_run(async {
            let result = async_close(123).await;
            assert_eq!(result, -libc::EBADF);
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }

    #[test]
    fn local_close_test2() {
        let result = async_run(async {
            let result = async_close(0).await;
            assert_eq!(result, 0);
            1
        });

        // ensure it actually executed
        assert_eq!(result, 1);
    }
}
