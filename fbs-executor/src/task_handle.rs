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
                task.borrow_mut().waiters.push(cx.waker().clone());
                return Poll::Pending;
            },
            (None, _) => panic!("Polling empty task handle"),
        }
    }
}

impl<T> Default for TaskHandle<T> {
    fn default() -> Self {
        Self { task: None, result: Rc::new(Cell::new(None)) }
    }
}

impl<T> TaskHandle<T> {
    pub fn is_completed(&self) -> bool {
        match &self.task {
            Some(task) => task.borrow().future.is_none(),
            None => true,
        }
    }

    pub fn result(&self) -> Option<T> {
        self.result.take()
    }

    pub fn cancel(self) {
        match self.task {
            Some(task) => {
                let mut task_data = task.borrow_mut();
                task_data.future = None;
                task_data.channel.send(crate::ExecutorCmd::Schedule(task.clone()));
            },
            None => (),
        }
    }

    pub fn cancel_by_ref(&mut self) {
        match &self.task {
            Some(task) => {
                let mut task_data = task.borrow_mut();
                task_data.future = None;
                task_data.channel.send(crate::ExecutorCmd::Schedule(task.clone()));
            },
            None => (),
        }

        self.task = None;
    }
}
