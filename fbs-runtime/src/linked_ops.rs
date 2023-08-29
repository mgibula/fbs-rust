use super::ReactorOpPtr;
use super::IOUringOp;
use super::AsyncOpResult;
use super::AsyncOp;

use std::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;
use std::cell::RefCell;
use std::rc::Rc;

use super::REACTOR;

pub struct AsyncLinkedOps<'ops> {
    ops: Vec<IOUringOp<'ops>>,
}

pub struct DelayedResult<T> {
    value: Rc<RefCell<Option<T>>>,
}

impl<T> Clone for DelayedResult<T> {
    fn clone(&self) -> Self {
        DelayedResult { value: self.value.clone() }
    }
}

impl<T> DelayedResult<T> {
    pub fn new() -> Self {
        DelayedResult { value: Rc::new(RefCell::new(None)) }
    }

    pub fn is_completed(&self) -> bool {
        self.value.borrow().is_some()
    }

    pub fn set_value(&self, value: T) {
        *self.value.borrow_mut()  = Some(value);
    }
}

impl AsyncLinkedOps {
    pub fn new() -> Self {
        AsyncLinkedOps { ops: vec![] }
    }

    pub fn add<T: AsyncOpResult + 'static>(&mut self, op: AsyncOp<T>) -> DelayedResult<T::Output> {
        let result = DelayedResult::new();
        let result2 = result.clone();

        // let op = op.0.clone();
        // // op.set_completion(move |cqe, params| {
        // //     result2.set_value(T::get_result(cqe, params));
        // // });

        // self.ops.push(op);
        result
    }
}

impl Future for AsyncLinkedOps {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let last_op = match self.ops.last() {
            None => return Poll::Ready(()),
            Some(op) => op,
        };

        let result = match (last_op.scheduled(), last_op.completed()) {
            (false, _) => {
                let waker = cx.waker().clone();
                last_op.set_completion(move |_cqe, _params| {
                    waker.wake_by_ref();
                });

                REACTOR.with(|r| {
                    r.borrow_mut().schedule_linked(&self.ops)
                }).expect("Error while scheduling multiple ops");

                Poll::Pending
            },
            (true, false) => Poll::Pending,
            (true, true) => Poll::Ready(())
        };

        result
    }
}