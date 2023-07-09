use std::pin::Pin;
use std::future::Future;
use std::rc::{Weak, Rc};
use std::task::{Context, Poll, Waker};

use super::Task;
use super::TaskData;
use super::ExecutorCmd;
use super::ChannelTx;

mod waker;

impl<T> TaskData<T> {
    pub fn new(future: Pin<Box<dyn Future<Output = T>>>, channel: ChannelTx<ExecutorCmd>) -> Self {
        TaskData {
            future,
            channel ,
            own_ptr: Weak::new(),
            wait_index: None,
            result: None,
            completed: false,
            waiters: vec![],
        }
    }
}

impl<T: 'static> Task for TaskData<T> {
    fn can_execute(&self) -> bool {
        !self.completed
    }

    fn poll(&mut self, ctx: &mut Context) -> Poll<()> {
        if self.completed {
            panic!("Pooling completed coroutine");
        }

        match self.future.as_mut().poll(ctx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(value) => {
                self.result = Some(value);
                self.completed = true;
                return Poll::Ready(());
            }
        };
    }

    fn get_waker(&self) -> Waker {
        waker::task_into_waker(Rc::into_raw(self.own_ptr.upgrade().unwrap()))
    }

    fn set_wait_index(&mut self, index: Option<usize>) -> Option<usize> {
        if let Some(index) = index {
            self.wait_index.replace(index)
        } else {
            self.wait_index.take()
        }
    }

    fn add_waiter(&mut self, waker: Waker) {
        self.waiters.push(waker)
    }

    fn wake_waiters(&mut self) {
        let waiters = std::mem::take(&mut self.waiters);
        waiters.into_iter().for_each(|w| w.wake_by_ref());
    }
}