use super::AsyncOpResult;
use super::AsyncOp;
use super::IOUringReq;
use super::IOUringOp;
use super::IoUringCQE;

use std::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;
use std::cell::Cell;
use std::rc::Rc;

use super::REACTOR;

pub struct AsyncLinkedOps {
    ops: Vec<IOUringReq>,
    last_result: Rc<Cell<Option<IoUringCQE>>>,
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
        AsyncLinkedOps { ops: vec![], last_result: Rc::new(Cell::new(None)) }
    }

    pub fn add<T: AsyncOpResult>(&mut self, op: AsyncOp<T>) -> DelayedResult<T::Output> {
        let AsyncOp::<T>(mut op_req, result_ptr) = op;

        // unsafe needed to move out member variables out
        let result = DelayedResult::new(result_ptr.clone());

        op_req.completion = Some(Box::new(move |cqe, params| {
            result_ptr.set(Some(T::get_result(cqe, params)));
        }));

        self.ops.push(op_req);
        result
    }
}

impl Future for AsyncLinkedOps {
    type Output = bool;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let last_result = self.last_result.clone();

        // return immediately if there are no ops
        let last_op = match self.ops.last_mut() {
            None                         => return Poll::Ready(true),
            Some(op)    => op,
        };

        match &last_op.op {
            IOUringOp::InProgress() => {
                match last_result.get() {
                    Some(cqe)   => { return Poll::Ready(cqe.result >= 0) },
                    None                    => { return Poll::Pending },
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

            last_result.set(Some(cqe));
            waker.wake_by_ref();
        }));

        REACTOR.with(|r| {
            r.borrow_mut().schedule_linked2(&mut self.ops)
        });

        Poll::Pending
    }
}