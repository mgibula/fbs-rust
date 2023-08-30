use super::AsyncOpResult;
use super::AsyncOp;
use super::IOUringReq;
use super::IOUringOp;

use std::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;
use std::cell::Cell;
use std::rc::Rc;

use super::REACTOR;

pub struct AsyncLinkedOps {
    ops: Vec<IOUringReq>,
}

pub struct DelayedResult<T> {
    value: Rc<Cell<Option<T>>>,
}

impl<T> Clone for DelayedResult<T> {
    fn clone(&self) -> Self {
        DelayedResult { value: self.value.clone() }
    }
}

impl<T> DelayedResult<T> {
    pub fn new(ptr: Rc<Cell<Option<T>>>) -> Self {
        Self {
            value: ptr
        }
    }

    pub fn value(self) -> T {
        self.value.replace(None).unwrap()
    }
}

impl AsyncLinkedOps {
    pub fn new() -> Self {
        AsyncLinkedOps { ops: vec![] }
    }

    pub fn add<T: AsyncOpResult>(&mut self, mut op: AsyncOp<T>) -> DelayedResult<T::Output> {
        let result_ptr = op.1;
        let result = DelayedResult::new(result_ptr.clone());

        op.0.completion = Some(Box::new(move |cqe, params| {
            result_ptr.set(Some(T::get_result(cqe, params)));
        }));

        self.ops.push(op.0);
        result
    }
}

impl Future for AsyncLinkedOps {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // return immediately if there are no ops
        let last_op = match self.ops.last_mut() {
            None => return Poll::Ready(()),
            Some(op) => op,
        };

        match &last_op.op {
            IOUringOp::InProgress(rop) => {
                match rop.completed() {
                    true => { return Poll::Ready(()) },
                    false => { return Poll::Pending },
                }
            },
            _ => { /* handled below */ }
        }

        let prev_cb = last_op.completion.take();
        let waker = cx.waker().clone();
        last_op.completion = Some(Box::new(move |cqe, params| {
            if let Some(cb) = &prev_cb {
                cb(cqe, params);
            }

            waker.wake_by_ref();
        }));

        REACTOR.with(|r| {
            r.borrow_mut().schedule_linked2(&mut self.ops)
        });

        Poll::Pending
    }
}