use std::collections::LinkedList;
use std::cell::RefCell;
use std::rc::Rc;
use std::task::{Context, Poll};

use super::IndexedList;
use super::ExecutorCmd;
use super::Executor;
use super::ExecutorFrontend;
use super::Task;
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

    pub fn has_pending_tasks(&self) -> bool {
        !self.ready.is_empty()
    }

    fn process_queue(&mut self) {
        loop {
            let cmd = self.channel.receive();
            match cmd {
                None => break,
                Some(ExecutorCmd::Schedule(task)) => {
                    let current_wait_index = task.borrow_mut().set_wait_index(None);
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
