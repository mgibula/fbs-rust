use std::future::Future;
use std::cell::RefCell;
use std::rc::Rc;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::ChannelTx;
use super::ExecutorCmd;
use super::TaskData;
use super::TaskHandle;
use super::ExecutorFrontend;

impl ExecutorFrontend {
    pub fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> TaskHandle<T> {
        let future = Box::pin(future);
        let task = Rc::new(RefCell::new(TaskData::new(future, self.channel.clone())));
        task.borrow_mut().own_ptr = Rc::downgrade(&task);

        self.channel.send(ExecutorCmd::Schedule(task.clone()));
        TaskHandle {
            task: task.clone(),
        }
    }

    pub fn yield_execution(&self) -> Yield {
        Yield {
            channel: self.channel.clone(),
            yielded: false,
        }
    }
}

pub struct Yield {
    channel: ChannelTx<ExecutorCmd>,
    yielded: bool,
}

impl Future for Yield {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.yielded {
            false => {
                self.yielded = true;
                self.channel.send(ExecutorCmd::Wake(cx.waker().clone()));
                Poll::Pending
            },
            true => {
                Poll::Ready(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Executor;

    #[test]
    fn basic_async_test() {
        let mut executor = Executor::new();
        let frontend = executor.get_frontend();

        let handle = frontend.spawn(async {
            return 111;
        });

        assert_eq!(handle.is_completed(), false);
        executor.run_once();
        assert_eq!(handle.is_completed(), true);

        let result = handle.result();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), 111);
    }

    #[test]
    fn basic_await_test() {
        let mut executor = Executor::new();
        let frontend = executor.get_frontend();

        let handle1 = frontend.spawn(async {
            return 123;
        });

        let handle2 = frontend.spawn(async move {
            return 1 + handle1.await;
        });

        executor.run_all();

        assert_eq!(handle2.is_completed(), true);
        assert_eq!(handle2.result(), Some(124));
    }
}
