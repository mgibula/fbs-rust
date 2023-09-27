use super::AsyncOpResult;
use super::AsyncOp;
use super::IOUringReq;
use super::IOUringOp;
use super::IoUringCQE;
use super::AsyncValue;

use std::mem::ManuallyDrop;
use std::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;
use std::cell::Cell;
use std::rc::Rc;

use super::REACTOR;

pub struct AsyncLinkedOps {
    ops: Vec<(IOUringReq, Rc<Cell<Option<IoUringCQE>>>)>,
    auto_cancel: bool,
}

pub struct DelayedResult<T> {
    value: Rc<Cell<AsyncValue<T>>>,
}

impl<T> Clone for DelayedResult<T> {
    fn clone(&self) -> Self {
        DelayedResult { value: self.value.clone() }
    }
}

impl<T> DelayedResult<T> {
    pub fn new(ptr: Rc<Cell<AsyncValue<T>>>) -> Self {
        Self {
            value: ptr
        }
    }

    pub fn value(self) -> T {
        self.value.replace(AsyncValue::Completed).as_option().unwrap()
    }
}

impl AsyncLinkedOps {
    pub fn new() -> Self {
        AsyncLinkedOps { ops: vec![], auto_cancel: false }
    }

    pub fn add<T: AsyncOpResult>(&mut self, op: AsyncOp<T>) -> DelayedResult<T::Output> {
        // AsyncOp has a custom Drop trait, so unsafe is needed for destructurization
        let op = ManuallyDrop::new(op);
        let (mut op_req, result_ptr) = unsafe { (std::ptr::read(&op.0), std::ptr::read(&op.1)) };

        let result = DelayedResult::new(result_ptr.clone());
        let result_generic = Rc::new(Cell::new(None));
        let result_generic_inner = result_generic.clone();

        op_req.completion = Some(Box::new(move |cqe, params| {
            result_generic_inner.set(Some(cqe));
            result_ptr.set(AsyncValue::Stored(T::get_result(cqe, params)));
        }));

        self.ops.push((op_req, result_generic));
        result
    }
}

impl Future for AsyncLinkedOps {
    type Output = bool;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // return immediately if there are no ops
        let last_op = match self.ops.last_mut() {
            None                         => return Poll::Ready(true),
            Some(op)    => op,
        };

        match (&last_op.0.op, last_op.1.get()) {
            (IOUringOp::InProgress(_), Some(cqe))   => { return Poll::Ready(cqe.result >= 0) },
            (IOUringOp::InProgress(_), None)                    => { return Poll::Pending },
            (_, _) => (),   /* handled below */
        }

        let prev_cb = last_op.0.completion.take();
        let waker = cx.waker().clone();
        last_op.0.completion = Some(Box::new(move |cqe, params| {
            if let Some(cb) = &prev_cb {
                cb(cqe, params);
            }

            waker.wake_by_ref();
        }));

        self.auto_cancel = true;
        let mut ops = self.ops.iter_mut().map(|e| {
            &mut e.0
        }).collect::<Vec<_>>();

        REACTOR.with(|r| {
            r.borrow_mut().schedule_linked2(&mut ops);
        });

        Poll::Pending
    }
}

impl Drop for AsyncLinkedOps {
    fn drop(&mut self) {
        if !self.auto_cancel {
            return;
        }

        let cancel_tags = self.ops.iter().filter_map(|e| {
            match (&e.0.op, e.1.get()) {
                (IOUringOp::InProgress(cancel), None) => Some(*cancel),
                (_, _) => None,
            }
        }).collect::<Vec<_>>();

        REACTOR.with(|r| {
            r.borrow_mut().cancel_op(&cancel_tags);
        });
    }
}