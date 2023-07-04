use std::collections::VecDeque;
use std::rc::Rc;
use std::cell::RefCell;

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
}

impl<T> ChannelTx<T> {
    pub fn send(&self, value : T) {
        self.backend.send(value)
    }
}

impl<T> ChannelBackend<T> {
    pub fn send(&self, value : T) {
        self.messages.borrow_mut().push_back(value);
    }

    pub fn is_empty(&self) -> bool {
        self.messages.borrow_mut().is_empty()
    }

    pub fn receive(&self) -> Option<T> {
        self.messages.borrow_mut().pop_front()
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