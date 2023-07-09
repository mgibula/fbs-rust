use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::TaskHandle;
use super::Task;

impl<T: 'static> Future for TaskHandle<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let maybe_value = self.task.borrow_mut().result.take();

        match maybe_value {
            None => {
                self.task.borrow_mut().add_waiter(cx.waker().clone());
                return Poll::Pending
            },
            Some(value) => Poll::Ready(value),
        }
    }
}

impl<T: 'static> TaskHandle<T> {
    pub fn is_completed(&self) -> bool {
        !self.task.borrow().can_execute()
    }

    pub fn result(self) -> Option<T> {
        self.task.borrow_mut().result.take()
    }
}
