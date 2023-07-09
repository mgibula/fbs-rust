use std::rc::Rc;
use std::cell::RefCell;
use std::task::{Waker, RawWaker, RawWakerVTable};

use super::TaskData;
use super::ExecutorCmd;

fn create_vtable<T: 'static>() -> &'static RawWakerVTable {
    unsafe {
        &RawWakerVTable::new(
            |s| waker_clone(&*(s as *const RefCell<TaskData<T>>)),
            |s| waker_wake(&*(s as *const RefCell<TaskData<T>>)),
            |s| waker_wake_by_ref(&*(s as *const RefCell<TaskData<T>>)),
            |s| waker_drop(&*(s as *const RefCell<TaskData<T>>)),
        )
    }
}

fn waker_clone<T: 'static>(s: &RefCell<TaskData<T>>) -> RawWaker {
    let waker_rc = unsafe { Rc::from_raw(s) };
    std::mem::forget(waker_rc.clone());

    RawWaker::new(Rc::into_raw(waker_rc) as *const (), create_vtable::<T>())
}

fn waker_wake<T: 'static>(s: &RefCell<TaskData<T>>) {
    let waker_rc = unsafe { Rc::from_raw(s) };
    waker_rc.borrow_mut().channel.send(ExecutorCmd::Schedule(waker_rc.clone()));
}

fn waker_wake_by_ref<T: 'static>(s: &RefCell<TaskData<T>>) {
    let waker_rc = unsafe { Rc::from_raw(s) };
    std::mem::forget(waker_rc.clone());

    waker_rc.borrow_mut().channel.send(ExecutorCmd::Schedule(waker_rc.clone()));
}

fn waker_drop<T: 'static>(s: &RefCell<TaskData<T>>) {
    let rc = unsafe { Rc::from_raw(s) };
    drop(rc);
}

pub(super) fn task_into_waker<T: 'static>(s: *const RefCell<TaskData<T>>) -> Waker {
    let raw_waker = RawWaker::new(s as *const (), create_vtable::<T>());
    unsafe { Waker::from_raw(raw_waker) }
}