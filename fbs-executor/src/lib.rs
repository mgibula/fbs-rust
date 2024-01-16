use std::task::Waker;
use std::cell::{RefCell, Cell};
use std::rc::Rc;
use std::pin::Pin;
use std::collections::VecDeque;
use std::future::Future;

use fbs_library::channel::{ChannelTx, ChannelRx, channel_create};
use fbs_library::indexed_list::IndexedList;

mod task_data;
mod executor_frontend;
mod executor;
mod task_handle;

pub use executor_frontend::Yield;

enum ExecutorCmd {
    Schedule(Rc<TaskData>),
    Wake(Waker),
}

pub struct Executor {
    ready: VecDeque<Rc<TaskData>>,
    waiting: IndexedList<Rc<TaskData>>,
    channel: ChannelRx<ExecutorCmd>,
}

pub struct ExecutorFrontend {
    channel: ChannelTx<ExecutorCmd>,
}

pub struct TaskData {
    channel: ChannelTx<ExecutorCmd>,
    future: Cell<Option<Pin<Box<dyn Future<Output = ()>>>>>,
    wait_index: Cell<Option<usize>>,
    waiters: RefCell<Vec<Waker>>,
    is_executable: Cell<bool>,
}

pub struct TaskHandle<T> {
    task: Option<Rc<TaskData>>,
    result: Rc<Cell<Option<T>>>,
    detached: bool,
}
