use std::pin::Pin;
use std::future::Future;
use std::task::{Context, Waker, Poll};

use std::collections::VecDeque;
use std::rc::Rc;
use std::cell::RefCell;

pub struct AsyncChannelRx<T> {
    backend: Rc<AsyncChannelBackend<T>>,
}

impl<T> Clone for AsyncChannelRx<T> {
    fn clone(&self) -> Self {
        AsyncChannelRx { backend: self.backend.clone() }
    }
}

pub struct AsyncChannelTx<T> {
    backend: Rc<AsyncChannelBackend<T>>,
}

impl<T> Clone for AsyncChannelTx<T> {
    fn clone(&self) -> Self {
        AsyncChannelTx { backend: self.backend.clone() }
    }
}

struct AsyncChannelBackend<T> {
    messages: RefCell<VecDeque<T>>,
    wakers: RefCell<Vec<Waker>>,
}

pub struct AsyncChannelValue<T> {
    channel: Rc<AsyncChannelBackend<T>>,
}

impl<T> Future for AsyncChannelValue<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.channel.receive() {
            None => {
                self.channel.add_waiter(cx.waker().clone());
                Poll::Pending
            },
            Some(value) => Poll::Ready(value)
        }
    }
}

impl<T> AsyncChannelRx<T> {
    pub fn receive(&mut self) -> AsyncChannelValue<T> {
        AsyncChannelValue { channel: self.backend.clone() }
    }

    pub fn is_empty(&self) -> bool {
        self.backend.is_empty()
    }

    pub fn tx(&self) -> AsyncChannelTx<T> {
        AsyncChannelTx {
            backend: self.backend.clone(),
        }
    }
}

impl<T> AsyncChannelTx<T> {
    pub fn send(&self, value : T) {
        self.backend.send(value)
    }
}

impl<T> AsyncChannelBackend<T> {
    pub fn send(&self, value : T) {
        self.messages.borrow_mut().push_back(value);
        self.wake_one();
    }

    pub fn is_empty(&self) -> bool {
        self.messages.borrow_mut().is_empty()
    }

    pub fn receive(&self) -> Option<T> {
        self.messages.borrow_mut().pop_front()
    }

    fn add_waiter(&self, waker: Waker) {
        self.wakers.borrow_mut().push(waker);
    }

    fn wake_one(&self) {
        let waiter = self.wakers.borrow_mut().pop();
        if let Some(waker) = waiter {
            waker.wake();
        }
    }
}

pub fn async_channel_create<T>() -> (AsyncChannelRx<T>, AsyncChannelTx<T>) {
    let backend = Rc::new(AsyncChannelBackend { messages: RefCell::new(VecDeque::new()), wakers: RefCell::new(Vec::new()) });

    (
        AsyncChannelRx{
            backend: backend.clone(),
        },
        AsyncChannelTx{
            backend: backend.clone(),
        }
    )
}

#[cfg(test)]
mod test {
    use crate::{async_run, async_spawn};
    use super::*;

    #[test]
    fn async_channel_test() {
        async_run(async {
            let (mut rx1, tx1) = async_channel_create::<i32>();
            let (mut rx2, tx2) = async_channel_create::<i32>();

            async_spawn(async move {
                let mut value = rx1.receive().await;
                value += 1;
                tx2.send(value);
            });

            let result = async_spawn(async move {
                tx1.send(1);
                rx2.receive().await
            });

            assert_eq!(result.await, 2);
        });
    }
}