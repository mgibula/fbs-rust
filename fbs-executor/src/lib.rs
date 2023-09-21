use std::task::Waker;
use std::cell::{RefCell, Cell};
use std::rc::Rc;
use std::pin::Pin;
use std::collections::VecDeque;
use std::future::Future;
use misc::channel::{ChannelTx, ChannelRx, channel_create};
use misc::indexed_list::IndexedList;

mod misc;
mod task_data;
mod executor_frontend;
mod executor;
mod task_handle;

pub use executor_frontend::Yield;

enum ExecutorCmd {
    Schedule(Rc<RefCell<TaskData>>),
    Wake(Waker),
}

pub struct Executor {
    ready: VecDeque<Rc<RefCell<TaskData>>>,
    waiting: IndexedList<Rc<RefCell<TaskData>>>,
    channel: ChannelRx<ExecutorCmd>,
}

pub struct ExecutorFrontend {
    channel: ChannelTx<ExecutorCmd>,
}

pub struct TaskData {
    channel: ChannelTx<ExecutorCmd>,
    future: Option<Pin<Box<dyn Future<Output = ()>>>>,
    wait_index: Option<usize>,
    waiters: Vec<Waker>,
}

pub struct TaskHandle<T> {
    task: Rc<RefCell<TaskData>>,
    result: Rc<Cell<Option<T>>>,
}
