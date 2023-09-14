use std::rc::Rc;
use std::cell::RefCell;
use std::task::{Waker, RawWaker, RawWakerVTable};

use super::TaskData;
use super::ExecutorCmd;

fn create_vtable() -> &'static RawWakerVTable {
    &RawWakerVTable::new(
        |s| waker_clone(s as *const RefCell<TaskData>),
        |s| waker_wake(s as *const RefCell<TaskData>),
        |s| waker_wake_by_ref(s as *const RefCell<TaskData>),
        |s| waker_drop(s as *const RefCell<TaskData>),
    )
}

fn waker_clone(s: *const RefCell<TaskData>) -> RawWaker {
    let waker_rc = unsafe { Rc::from_raw(s) };
    std::mem::forget(waker_rc.clone());

    RawWaker::new(Rc::into_raw(waker_rc) as *const (), create_vtable())
}

fn waker_wake(s: *const RefCell<TaskData>) {
    let waker_rc = unsafe { Rc::from_raw(s) };
    waker_rc.borrow_mut().channel.send(ExecutorCmd::Schedule(waker_rc.clone()));
}

fn waker_wake_by_ref(s: *const RefCell<TaskData>) {
    let waker_rc = unsafe { Rc::from_raw(s) };
    std::mem::forget(waker_rc.clone());

    waker_rc.borrow_mut().channel.send(ExecutorCmd::Schedule(waker_rc.clone()));
}

fn waker_drop(s: *const RefCell<TaskData>) {
    let rc = unsafe { Rc::from_raw(s) };
    drop(rc);
}

pub fn task_into_waker(s: *const RefCell<TaskData>) -> Waker {
    let raw_waker = RawWaker::new(s as *const (), create_vtable());
    unsafe { Waker::from_raw(raw_waker) }
}