use std::future::Future;
use std::task::{Context, Poll, Waker};
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::cell::{RefCell, Cell};
use std::collections::LinkedList;

use crate::misc::channel::{ChannelTx, ChannelRx};
use crate::misc::indexed_list::IndexedList;
use crate::waker::task_into_waker;

thread_local! {
    // static EXECUTOR: ExecutorRuntime = ExecutorRuntime::default();
}

pub enum ExecutorCmd {
    Schedule(Rc<RefCell<dyn Task>>),
}

pub struct Executor {
    ready: LinkedList<Rc<RefCell<dyn Task>>>,
    waiting: IndexedList<Rc<RefCell<dyn Task>>>,
    channel: ChannelRx<ExecutorCmd>,
}

impl Executor {
    pub fn spawn<T>(&mut self, future: impl Future<Output = T> + 'static) -> TaskHandle<impl Future> {
        let task = Rc::new(RefCell::new(TaskData::new(future, self.channel.tx())));
        task.borrow_mut().own_ptr = Rc::downgrade(&task);

        self.ready.push_back(task.clone());
        TaskHandle {
            task: task.clone(),
        }
    }

    pub fn run_once(&mut self) -> bool {
        loop {
            let cmd = self.channel.receive();
            match cmd {
                None => break,
                Some(ExecutorCmd::Schedule(task)) => self.ready.push_back(task),
            };
        };

        let task = self.ready.pop_front();
        match task {
            None => return false,
            Some(task) => {
                let waker = task.borrow().get_waker();
                let mut context = Context::from_waker(&waker);

                let poll_result = task.borrow_mut().poll(&mut context);

                match poll_result {
                    Poll::Pending => {
                        let index = self.waiting.allocate();
                        // task.borrow_mut()
                    },
                    Poll::Ready(()) => { },
                };
            }
        };

        true
    }
}

pub struct TaskHandle<T: Future> {
    task: Rc<RefCell<TaskData<T>>>,
}

impl<T: Future + 'static> Future for TaskHandle<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let completed = self.task.borrow().completed();
        match completed {
            true => Poll::Ready(self.task.borrow_mut().get_result().unwrap()),
            false => Poll::Pending,
        }
    }
}

impl<T: Future+ 'static> TaskHandle<T> {
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
    fn get_waker(&self) -> Waker;
}

pub enum TaskStorage<T: Future> {
    InProgress(T),
    Result(T::Output),
    Finished,
}

pub struct TaskData<T: Future> {
    pub channel: ChannelTx<ExecutorCmd>,
    storage: TaskStorage<T>,
    own_ptr: Weak<RefCell<TaskData<T>>>,
}

impl<T: Future> TaskData<T> {
    pub fn new(future: T, channel: ChannelTx<ExecutorCmd>) -> Self {
        TaskData {
            storage: TaskStorage::InProgress(future),
            channel ,
            own_ptr: Weak::new(),
        }
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

impl<T: Future + 'static> Task for TaskData<T> {
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

    fn get_waker(&self) -> Waker {
        task_into_waker(Rc::into_raw(self.own_ptr.upgrade().unwrap()))
    }
}
