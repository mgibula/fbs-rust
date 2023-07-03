use std::future::Future;
use std::task::{Context, Poll};
use std::pin::Pin;
use std::rc::Rc;
use std::cell::{RefCell, Cell};
use std::collections::LinkedList;

thread_local! {
    // static EXECUTOR: ExecutorRuntime = ExecutorRuntime::default();
}

#[derive(Default)]
pub struct Executor {
    ready: LinkedList<Rc<RefCell<dyn Task>>>,
}

impl Executor {
    pub fn spawn<T>(&mut self, future: impl Future<Output = T> + 'static) -> TaskHandle<impl Future> {
        let task = Rc::new(RefCell::new(TaskData::new(future)));

        self.ready.push_back(task.clone());
        TaskHandle { task }
    }
}

struct Waker {

}

pub struct TaskHandle<T: Future> {
    task: Rc<RefCell<TaskData<T>>>,
}

impl<T: Future> Future for TaskHandle<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let completed = self.task.borrow().completed();
        match completed {
            true => Poll::Ready(self.task.borrow_mut().get_result().unwrap()),
            false => Poll::Pending,
        }
    }
}

impl<T: Future> TaskHandle<T> {
    pub fn is_completed(&self) -> bool {
        self.task.borrow().completed()
    }

    pub fn result(&mut self) -> Option<T::Output> {
        if !self.is_completed() {
            return None;
        }

        self.task.borrow_mut().get_result()
    }
}

pub trait Task {
    fn completed(&self) -> bool;
    fn poll(&mut self, ctx: &mut Context) -> Poll<()>;
}

pub enum TaskStorage<T: Future> {
    InProgress(T),
    Result(T::Output),
    Finished,
}

pub struct TaskData<T: Future> {
    storage: TaskStorage<T>,
}

impl<T: Future> TaskData<T> {
    pub fn new(future: T) -> Self {
        TaskData { storage: TaskStorage::InProgress(future) }
    }

    pub fn get_result(&mut self) -> Option<T::Output> {
        match self.storage {
            TaskStorage::Finished => None,
            TaskStorage::InProgress(_) => None,
            TaskStorage::Result(_) => {
                let result = std::mem::replace(&mut self.storage, TaskStorage::Finished);
                if let TaskStorage::Result(value) = result {
                    return Some(value);
                }

                unreachable!()
            }
        }
    }
}

impl<T: Future> Task for TaskData<T> {
    fn completed(&self) -> bool {
        match self.storage {
            TaskStorage::InProgress(_) => false,
            TaskStorage::Result(_) => true,
            TaskStorage::Finished => true,
        }
    }

    fn poll(&mut self, ctx: &mut Context) -> Poll<()> {
        if let TaskStorage::InProgress(future) = &mut self.storage {
            let mut future = unsafe { Pin::new_unchecked(future) };
            let result = future.as_mut().poll(ctx);
            match result {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(value) => self.storage = TaskStorage::Result(value),
            }
        }

        if let TaskStorage::Result(_) = self.storage {
            return Poll::Ready(())
        }

        panic!("Pooling completed coroutine");
    }
}

pub fn test_async(future: impl Future) {
    println!("Size of future is {}", std::mem::size_of_val(&future));
}