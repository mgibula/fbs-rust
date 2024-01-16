use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::rc::Rc;
use std::cell::Cell;

use super::TaskHandle;

impl<T> Future for TaskHandle<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let maybe_value = self.result.take();

        match (&self.task, maybe_value) {
            (Some(_), Some(value)) => Poll::Ready(value),
            (Some(task), None) => {
                task.waiters.borrow_mut().push(cx.waker().clone());
                return Poll::Pending;
            },
            (None, _) => panic!("Polling empty task handle"),
        }
    }
}

impl<T> Default for TaskHandle<T> {
    fn default() -> Self {
        Self { task: None, result: Rc::new(Cell::new(None)), detached: true }
    }
}

impl<T> Drop for TaskHandle<T> {
    fn drop(&mut self) {
        if !self.detached {
            self.cancel_by_ref()
        }
    }
}

impl<T> TaskHandle<T> {
    pub fn is_completed(&self) -> bool {
        match &self.task {
            Some(task) => !task.is_executable.get(),
            None => true,
        }
    }

    pub fn result(&self) -> Option<T> {
        self.result.take()
    }

    pub fn detach(mut self) {
        self.detached = true
    }

    pub fn cancel(self) {
        match &self.task {
            Some(task) => {
                task.is_executable.set(false);
                task.future.set(None);
                task.channel.send(crate::ExecutorCmd::Schedule(task.clone()));
            },
            None => (),
        }
    }

    pub fn cancel_by_ref(&mut self) {
        match &self.task {
            Some(task) => {
                task.is_executable.set(false);
                task.future.set(None);
                task.channel.send(crate::ExecutorCmd::Schedule(task.clone()));
            },
            None => (),
        }

        self.task = None;
    }
}
