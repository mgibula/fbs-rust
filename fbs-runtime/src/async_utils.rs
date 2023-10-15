use std::os::fd::AsRawFd;
use std::pin::Pin;
use std::cell::Cell;
use std::future::Future;
use std::task::{Context, Waker, Poll};
use std::fmt::{Debug, Formatter};

use std::collections::VecDeque;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Arc;

use fbs_library::eventfd::*;
use fbs_library::system_error::SystemError;

use super::{async_read_struct, async_write_struct};

#[derive(Debug)]
pub struct AsyncChannelRx<T> {
    backend: Rc<AsyncChannelBackend<T>>,
}

impl<T> Clone for AsyncChannelRx<T> {
    fn clone(&self) -> Self {
        AsyncChannelRx { backend: self.backend.clone() }
    }
}

#[derive(Debug)]
pub struct AsyncChannelTx<T> {
    backend: Rc<AsyncChannelBackend<T>>,
}

impl<T> Clone for AsyncChannelTx<T> {
    fn clone(&self) -> Self {
        AsyncChannelTx { backend: self.backend.clone() }
    }
}

#[derive(Debug)]
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

struct AsyncSignalBackend {
    fired: Cell<bool>,
    waiters: Cell<Vec<Waker>>,
}

impl Debug for AsyncSignalBackend {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncSignalBackend")
            .field("fired", &self.fired)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct AsyncSignal {
    ptr: Rc<AsyncSignalBackend>,
}

impl AsyncSignal {
    pub fn new() -> Self {
        Self { ptr: Rc::new(AsyncSignalBackend { fired: Cell::new(false), waiters: Cell::new(Vec::new()) }) }
    }

    pub fn signal(&self) {
        self.ptr.fired.set(true);
        self.ptr.waiters.take().into_iter().for_each(|w| w.wake());
    }

    pub fn is_signalled(&self) -> bool {
        self.ptr.fired.get()
    }

    pub async fn wait(&self) {
        self.clone().await;
    }
}

impl Future for AsyncSignal {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.ptr.fired.get() {
            true => Poll::Ready(()),
            false => {
                let mut waiters = self.ptr.waiters.take();
                waiters.push(cx.waker().clone());
                self.ptr.waiters.set(waiters);

                Poll::Pending
            },
        }
    }
}

struct AsyncSignalBackendMT {
    eventfd: EventFd,
}

impl AsyncSignalBackendMT {
    fn new() -> Result<Self, SystemError> {
        Ok(Self { eventfd: EventFd::new(0, EventFdFlags::new().close_on_exec(true))? })
    }
}

pub struct AsyncSignalMT {
    ptr: Arc<AsyncSignalBackendMT>,
}

impl AsyncSignalMT {
    pub fn new() -> Result<Self, SystemError> {
        Ok(Self { ptr: Arc::new(AsyncSignalBackendMT::new()?)})
    }

    pub fn trigger(&self) -> AsyncSignalTriggerMT {
        AsyncSignalTriggerMT { ptr: self.ptr.clone() }
    }

    pub async fn wait(&self) {
        async_read_struct::<u64>(&self.ptr.eventfd.as_raw_fd(), None).await.expect("Error while waiting for event signal");
    }
}

pub struct AsyncSignalTriggerMT {
    ptr: Arc<AsyncSignalBackendMT>,
}

impl AsyncSignalTriggerMT {
    pub fn signal(&self) {
        self.ptr.eventfd.write(1);
    }

    pub async fn async_signal(&self) {
        async_write_struct(&self.ptr.eventfd.as_raw_fd(), 1 as u64, None).await.expect("Error while writing to event signal");
    }
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

    #[test]
    fn async_signal_test() {
        async_run(async {
            let (mut rx1, tx1) = async_channel_create::<i32>();
            let tx2 = tx1.clone();
            let sig1 = AsyncSignal::new();
            let sig1cpy = sig1.clone();

            let sig2 = AsyncSignal::new();
            let sig2cpy = sig2.clone();

            async_spawn(async move {
                assert_eq!(sig1.is_signalled(), false);
                sig1.wait().await;
                assert_eq!(sig1.is_signalled(), true);

                tx1.send(1);
                sig2cpy.signal();
            });

            async_spawn(async move {
                tx2.send(2);

                assert_eq!(sig1cpy.is_signalled(), false);
                sig1cpy.signal();
                assert_eq!(sig1cpy.is_signalled(), true);

                sig2.wait().await;
                tx2.send(3);
            });

            let v1 = rx1.receive().await;
            let v2 = rx1.receive().await;
            let v3 = rx1.receive().await;

            assert_eq!(v1, 2);
            assert_eq!(v2, 1);
            assert_eq!(v3, 3);
        });
    }

    #[test]
    fn async_signal_mt_test() {
        async_run(async {
            let (mut rx1, tx1) = async_channel_create::<i32>();
            let tx2 = tx1.clone();
            let sig1 = AsyncSignalMT::new().unwrap();
            let sig1cpy = sig1.trigger();

            let sig2 = AsyncSignalMT::new().unwrap();
            let sig2cpy = sig2.trigger();

            async_spawn(async move {
                sig1.wait().await;

                tx1.send(1);
                sig2cpy.signal();
            });

            async_spawn(async move {
                tx2.send(2);

                sig1cpy.signal();

                sig2.wait().await;
                tx2.send(3);
            });

            let v1 = rx1.receive().await;
            let v2 = rx1.receive().await;
            let v3 = rx1.receive().await;

            assert_eq!(v1, 2);
            assert_eq!(v2, 1);
            assert_eq!(v3, 3);
        });
    }
}