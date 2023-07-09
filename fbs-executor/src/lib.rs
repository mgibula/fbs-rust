use std::task::{Context, Poll, Waker};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::pin::Pin;
use std::collections::LinkedList;
use std::future::Future;
use misc::channel::{ChannelTx, ChannelRx, channel_create};
use misc::indexed_list::IndexedList;

mod misc;
mod task_data;
mod executor_frontend;
mod executor;
mod task_handle;

pub use executor_frontend::Yield;

pub struct TaskHandle<T> {
    task: Rc<RefCell<TaskData<T>>>,
}

enum ExecutorCmd {
    Schedule(Rc<RefCell<dyn Task>>),
    Wake(Waker),
}

pub struct Executor {
    ready: LinkedList<Rc<RefCell<dyn Task>>>,
    waiting: IndexedList<Rc<RefCell<dyn Task>>>,
    channel: ChannelRx<ExecutorCmd>,
}

pub struct ExecutorFrontend {
    channel: ChannelTx<ExecutorCmd>,
}

trait Task {
    fn can_execute(&self) -> bool;
    fn poll(&mut self, ctx: &mut Context) -> Poll<()>;
    fn get_waker(&self) -> Waker;
    fn set_wait_index(&mut self, index: Option<usize>) -> Option<usize>;
    fn add_waiter(&mut self, waker: Waker);
    fn wake_waiters(&mut self);
}

struct TaskData<T> {
    pub channel: ChannelTx<ExecutorCmd>,        // for requeuing
    future: Pin<Box<dyn Future<Output = T>>>,   // future to execute
    own_ptr: Weak<RefCell<TaskData<T>>>,        // used by waker to get typed pointer to ourselfs
    wait_index: Option<usize>,                  // index on waitlist when not running
    result: Option<T>,                          // where to store result
    completed: bool,                            // are we completed?
    waiters: Vec<Waker>,
}
