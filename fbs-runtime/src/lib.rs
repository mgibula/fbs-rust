use std::future::Future;
use std::cell::RefCell;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;

use fbs_executor::*;
use fbs_reactor::*;


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

#[cfg(test)]
fn async_run_once() -> bool {
    EXECUTOR.with(|e| {
        e.borrow_mut().run_once()
    })
}

pub fn async_run_all() {
    EXECUTOR.with(|e| {
        e.borrow_mut().run_all();
        e.borrow().has_pending_tasks()
    });
}

pub fn async_yield() -> Yield {
    FRONTEND.with(|e| {
        e.yield_execution()
    })
}

pub struct AsyncOp (ReactorOp, Option<OpDescriptorPtr>);

impl Future for AsyncOp {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.1 {
            Some(op) => {
                match op.completed() {
                    false => Poll::Pending,
                    true => Poll::Ready(())
                }
            },
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
}
