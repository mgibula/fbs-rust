use std::future::Future;
use std::ops::DerefMut;
use std::task::{Context, Poll, Waker};
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::cell::{RefCell, Cell};
use std::collections::LinkedList;

use crate::misc::channel::{ChannelTx, ChannelRx, channel_create};
use crate::misc::indexed_list::IndexedList;
use crate::waker::task_into_waker;

thread_local! {
    static EXECUTOR: RefCell<Executor> = RefCell::new(Executor::new());
}

pub fn local_executor<F: FnMut(&mut Executor)>(mut func: F) {
    EXECUTOR.with(|e| {
        func(e.borrow_mut().deref_mut());
    });
}

pub enum ExecutorCmd {
    Schedule(Rc<RefCell<dyn Task>>),
    Wake(Waker),
}

pub struct Executor {
    ready: LinkedList<Rc<RefCell<dyn Task>>>,
    waiting: IndexedList<Rc<RefCell<dyn Task>>>,
    channel: ChannelRx<ExecutorCmd>,
}

impl Executor {
    pub fn new() -> Self {
        let (rx, _) = channel_create();

        Executor {
            ready: LinkedList::default(),
            waiting: IndexedList::new(),
            channel: rx,
        }
    }

    pub fn spawn<T: 'static>(&mut self, future: impl Future<Output = T> + 'static) -> TaskHandle<T> {
        let future = Box::pin(future);
        let result = Rc::new(Cell::new(Option::<T>::None));
        let task = Rc::new(RefCell::new(TaskData::new(future, self.channel.tx())));
        task.borrow_mut().own_ptr = Rc::downgrade(&task);

        self.ready.push_back(task.clone());
        TaskHandle {
            task: task.clone(),
        }
    }

    pub fn run_all(&mut self) {
        while self.run_once() {
        }
    }

    pub fn run_once(&mut self) -> bool {
        self.process_queue();

        let task = self.ready.pop_front();
        match task {
            None => false,
            Some(task) => {
                self.process_task(task);
                true
            }
        }
    }

    fn process_queue(&mut self) {
        loop {
            let cmd = self.channel.receive();
            match cmd {
                None => break,
                Some(ExecutorCmd::Schedule(task)) => {
                    if let Some(wait_index) = task.borrow().get_wait_index() {
                        task.borrow_mut().set_wait_index(None);
                        self.waiting.remove(wait_index);
                    }

                    self.ready.push_back(task);
                },
                Some(ExecutorCmd::Wake(waker)) => {
                    waker.wake();
                }
            };
        }
    }

    fn process_task(&mut self, task: Rc<RefCell<dyn Task>>) {
        if !task.borrow().can_execute() {
            return;
        }

        let waker = task.borrow().get_waker();
        let mut context = Context::from_waker(&waker);

        let poll_result = task.borrow_mut().poll(&mut context);
        match poll_result {
            Poll::Pending => {
                let index = self.waiting.allocate();
                task.borrow_mut().set_wait_index(Some(index));
                self.waiting.insert_at(index, task);
            },
            Poll::Ready(()) => {
                task.borrow_mut().wake_waiters();
            },
        };
    }
}

pub struct TaskHandle<T> {
    task: Rc<RefCell<TaskData<T>>>,
}

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

pub struct Yield {
    channel: ChannelTx<ExecutorCmd>,
    yielded: bool,
}

impl Yield {
    pub fn new(channel: ChannelTx<ExecutorCmd>) -> Self {
        Yield { channel, yielded: false }
    }
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

pub trait Task {
    fn can_execute(&self) -> bool;
    fn poll(&mut self, ctx: &mut Context) -> Poll<()>;
    fn get_waker(&self) -> Waker;
    fn set_wait_index(&mut self, index: Option<usize>);
    fn get_wait_index(&self) -> Option<usize>;
    fn add_waiter(&mut self, waker: Waker);
    fn wake_waiters(&mut self);
}

pub struct TaskData<T> {
    pub channel: ChannelTx<ExecutorCmd>,        // for requeuing
    future: Pin<Box<dyn Future<Output = T>>>,   // future to execute
    own_ptr: Weak<RefCell<TaskData<T>>>,        // used by waker to get typed pointer to ourselfs
    wait_index: Option<usize>,                  // index on waitlist when not running
    result: Option<T>,                // where to store result
    completed: bool,                            // are we completed?
    waiters: Vec<Waker>,
}

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
        task_into_waker(Rc::into_raw(self.own_ptr.upgrade().unwrap()))
    }

    fn set_wait_index(&mut self, index: Option<usize>) {
        self.wait_index = index
    }

    fn get_wait_index(&self) -> Option<usize> {
        self.wait_index
    }

    fn add_waiter(&mut self, waker: Waker) {
        self.waiters.push(waker)
    }

    fn wake_waiters(&mut self) {
        let waiters = std::mem::take(&mut self.waiters);
        waiters.into_iter().for_each(|w| w.wake_by_ref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_async_test() {
        let mut executor = Executor::new();

        let handle = executor.spawn(async {
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

        let handle1 = executor.spawn(async {
            return 123;
        });

        let handle2 = executor.spawn(async move {
            return 1 + handle1.await;
        });

        executor.run_all();

        assert_eq!(handle2.is_completed(), true);
        assert_eq!(handle2.result(), Some(124));
    }

    #[test]
    fn local_executor_test() {
        local_executor(|e| {
            let handle1 = e.spawn(async {
                return 1;
            });
        });
    }


}
