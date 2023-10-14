use std::collections::VecDeque;
use std::rc::Rc;
use std::cell::RefCell;
use std::fmt::{Debug, Formatter};

pub struct ChannelRx<T> {
    backend: Rc<ChannelBackend<T>>,
}

impl<T> Clone for ChannelRx<T> {
    fn clone(&self) -> Self {
        ChannelRx { backend: self.backend.clone() }
    }
}

pub struct ChannelTx<T> {
    backend: Rc<ChannelBackend<T>>,
}

impl<T> Clone for ChannelTx<T> {
    fn clone(&self) -> Self {
        ChannelTx { backend: self.backend.clone() }
    }
}

struct ChannelBackend<T> {
    messages: RefCell<VecDeque<T>>,
}

impl<T> Debug for ChannelBackend<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("messages", &self.messages.borrow().len())
            .finish()
    }
}

impl<T> Debug for ChannelTx<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelTx")
            .field("messages", &self.backend.messages.borrow().len())
            .finish()
    }
}

impl<T> Debug for ChannelRx<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelRx")
            .field("messages", &self.backend.messages.borrow().len())
            .finish()
    }
}

impl<T> ChannelRx<T> {
    pub fn receive(&mut self) -> Option<T> {
        self.backend.receive()
    }

    pub fn is_empty(&self) -> bool {
        self.backend.is_empty()
    }

    pub fn tx(&self) -> ChannelTx<T> {
        ChannelTx {
            backend: self.backend.clone(),
        }
    }

    pub fn len(&self) -> usize {
        self.backend.len()
    }
}

impl<T> ChannelTx<T> {
    pub fn send(&self, value: T) {
        self.backend.send(value)
    }
}

impl<T> ChannelBackend<T> {
    pub fn send(&self, value: T) {
        self.messages.borrow_mut().push_back(value);
    }

    pub fn is_empty(&self) -> bool {
        self.messages.borrow_mut().is_empty()
    }

    pub fn receive(&self) -> Option<T> {
        self.messages.borrow_mut().pop_front()
    }

    pub fn len(&self) -> usize {
        self.messages.borrow().len()
    }
}

pub fn channel_create<T>() -> (ChannelRx<T>, ChannelTx<T>) {
    let backend = Rc::new(ChannelBackend { messages: RefCell::new(VecDeque::new()) });

    (
        ChannelRx{
            backend: backend.clone(),
        },
        ChannelTx{
            backend: backend.clone(),
        }
    )
}