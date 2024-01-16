use std::collections::VecDeque;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::fmt::{Debug, Formatter};

use super::TaskData;
use super::IndexedList;
use super::ExecutorCmd;
use super::Executor;
use super::ExecutorFrontend;
use super::channel_create;

impl Debug for Executor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Executor")
            .field("ready", &self.ready.len())
            .field("waiting", &self.waiting.size())
            .field("channel", &self.channel.len())
            .finish()
    }
}

impl Executor {
    pub fn new() -> Self {
        let (rx, _) = channel_create();

        Executor {
            ready: VecDeque::with_capacity(10),
            waiting: IndexedList::new(),
            channel: rx,
        }
    }

    pub fn get_frontend(&self) -> ExecutorFrontend {
        ExecutorFrontend {
            channel: self.channel.tx()
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

    pub fn has_ready_tasks(&self) -> bool {
        !self.channel.is_empty() || !self.ready.is_empty()
    }

    fn process_queue(&mut self) {
        loop {
            let cmd = self.channel.receive();
            match cmd {
                None => break,
                Some(ExecutorCmd::Schedule(task)) => {
                    let current_wait_index = task.wait_index.take();
                    if let Some(wait_index) = current_wait_index {
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

    fn process_task(&mut self, task: Rc<TaskData>) {
        match (task.is_executable.get(), task.future.take()) {
            (false, _) => (),
            (true, None) => (),
            (true, Some(mut future)) => {
                let waker = super::task_data::task_into_waker(Rc::into_raw(task.clone()));
                let mut context = Context::from_waker(&waker);

                match future.as_mut().poll(&mut context) {
                    Poll::Pending => {
                        task.future.set(Some(future));

                        let index = self.waiting.allocate();
                        task.wait_index.set(Some(index));
                        self.waiting.insert_at(index, task);
                    },
                    Poll::Ready(()) => {
                        task.is_executable.set(false);
                        task.waiters.borrow().iter().for_each(|w| w.wake_by_ref());
                        task.waiters.borrow_mut().clear();
                    },
                }
            },
        }
    }
}
