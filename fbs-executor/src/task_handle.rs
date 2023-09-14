use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::TaskHandle;

impl<T> Future for TaskHandle<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let maybe_value = self.result.take();
        match maybe_value {
            None => {
                self.task.borrow_mut().waiters.push(cx.waker().clone());
                return Poll::Pending
            },
            Some(value) => Poll::Ready(value),
        }
    }
}

impl<T> TaskHandle<T> {
    pub fn is_completed(&self) -> bool {
        self.task.borrow().completed
    }

    pub fn result(self) -> Option<T> {
        self.result.take()
    }
}
