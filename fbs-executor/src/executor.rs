use std::collections::LinkedList;
use std::cell::RefCell;
use std::rc::Rc;
use std::task::{Context, Poll};

use super::TaskData;
use super::IndexedList;
use super::ExecutorCmd;
use super::Executor;
use super::ExecutorFrontend;
use super::channel_create;

impl Executor {
    pub fn new() -> Self {
        let (rx, _) = channel_create();

        Executor {
            ready: LinkedList::default(),
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
                    let current_wait_index = task.borrow_mut().wait_index.take();
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

    fn process_task(&mut self, task: Rc<RefCell<TaskData>>) {
        if task.borrow().completed {
            return;
        }

        let waker = super::task_data::task_into_waker(Rc::into_raw(task.clone()));
        let mut context = Context::from_waker(&waker);

        let mut task_data = task.borrow_mut();
        match task_data.future.as_mut().poll(&mut context) {
            Poll::Pending => {
                let index = self.waiting.allocate();
                task_data.wait_index = Some(index);

                drop(task_data);
                self.waiting.insert_at(index, task);
            },
            Poll::Ready(()) => {
                task_data.completed = true;
                task_data.waiters.iter().for_each(|w| w.wake_by_ref());
                task_data.waiters.clear();
            },
        }
    }

}
