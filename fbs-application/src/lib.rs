use std::marker::PhantomData;

use fbs_runtime::async_utils::*;
use fbs_runtime::async_run;

pub trait ApplicationLogic : Sized + 'static {
    type Event;
    type Error;

    fn create() -> Result<Self, Self::Error>;

    fn handle_system_event(&mut self, event: SystemEvent);

    fn handle_app_event(&mut self, state: &mut ApplicationState<Self::Event>, event: Self::Event);
}

pub struct Application<T: ApplicationLogic> {
    _marker: PhantomData<T>,
}

impl<T: ApplicationLogic> Application<T> {
    pub fn create() -> Result<Self, T::Error> {
        Ok(Self { _marker: PhantomData })
    }

    pub fn run(&mut self) -> Result<(), T::Error> {
        let mut app = Box::new(T::create()?);
        let mut state = Box::new(ApplicationState::<T::Event>::new());

        state.send_system_event(SystemEvent::ApplicationInit);

        async_run(async move {
            loop {
                state.has_event.wait().await;

                if !state.internal_queue_rx.is_empty() {
                    let event = state.internal_queue_rx.receive().await;
                    let running = state.handle_system_event(event);
                    if !running {
                        break;
                    }

                    continue;
                }

                if !state.app_queue_rx.is_empty() {
                    let event = state.app_queue_rx.receive().await;
                    app.handle_app_event(state.as_mut(), event);

                    continue;
                }
           }
        });

        Ok(())
    }
}

pub enum SystemEvent {
    ApplicationInit,
    ApplicationQuit,
}

pub struct ApplicationState<T> {
    internal_queue_rx: AsyncChannelRx<SystemEvent>,
    internal_queue_tx: AsyncChannelTx<SystemEvent>,
    app_queue_rx: AsyncChannelRx<T>,
    app_queue_tx: AsyncChannelTx<T>,
    has_event: AsyncSignal,
}

impl<T> ApplicationState<T> {
    fn new() -> Self {
        let (rx, tx) = async_channel_create();
        let (app_rx, app_tx) = async_channel_create();

        ApplicationState {
            internal_queue_rx: rx,
            internal_queue_tx: tx,
            app_queue_tx: app_tx,
            app_queue_rx: app_rx,
            has_event: AsyncSignal::new(),
        }
    }

    fn handle_system_event(&mut self, event: SystemEvent) -> bool {
        match event {
            SystemEvent::ApplicationQuit => return false,
            _ => (),
        }

        true
    }

    fn send_system_event(&mut self, event: SystemEvent) {
        self.internal_queue_tx.send(event);
        self.has_event.signal();
    }

    pub fn send_app_event(&mut self, event: T) {
        self.app_queue_tx.send(event);
        self.has_event.signal();
    }
}