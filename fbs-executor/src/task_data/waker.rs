use std::rc::Rc;
use std::task::{Waker, RawWaker, RawWakerVTable};

use super::TaskData;
use super::ExecutorCmd;

fn create_vtable() -> &'static RawWakerVTable {
    &RawWakerVTable::new(
        |s| waker_clone(s as *const TaskData),
        |s| waker_wake(s as *const TaskData),
        |s| waker_wake_by_ref(s as *const TaskData),
        |s| waker_drop(s as *const TaskData),
    )
}

fn waker_clone(s: *const TaskData) -> RawWaker {
    let waker_rc = unsafe { Rc::from_raw(s) };
    std::mem::forget(waker_rc.clone());

    RawWaker::new(Rc::into_raw(waker_rc) as *const (), create_vtable())
}

fn waker_wake(s: *const TaskData) {
    let waker_rc = unsafe { Rc::from_raw(s) };
    waker_rc.channel.send(ExecutorCmd::Schedule(waker_rc.clone()));
}

fn waker_wake_by_ref(s: *const TaskData) {
    let waker_rc = unsafe { Rc::from_raw(s) };
    std::mem::forget(waker_rc.clone());

    waker_rc.channel.send(ExecutorCmd::Schedule(waker_rc.clone()));
}

fn waker_drop(s: *const TaskData) {
    let rc = unsafe { Rc::from_raw(s) };
    drop(rc);
}

pub fn task_into_waker(s: *const TaskData) -> Waker {
    let raw_waker = RawWaker::new(s as *const (), create_vtable());
    unsafe { Waker::from_raw(raw_waker) }
}