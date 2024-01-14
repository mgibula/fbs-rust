use std::marker::PhantomData;

use fbs_runtime::*;
use fbs_runtime::async_utils::*;
use fbs_runtime::async_run;
use fbs_executor::TaskHandle;
use fbs_library::sigset::*;
use fbs_library::signalfd::*;

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

        let mut notifier = state.create_notifier();
        notifier.send_system_event(SystemEvent::ApplicationInit);

        async_run(async move {
            state.signal_coro = async_spawn(async move {
                let mut mask = SignalSet::empty();
                mask.add(Signal::SIGINT);
                mask.add(Signal::SIGQUIT);
                mask.add(Signal::SIGHUP);
                mask.add(Signal::SIGCHLD);

                set_process_signal_mask(SignalMask::Block, mask).unwrap();

                let sigfd = SignalFd::new(mask, SignalFdFlags::new().close_on_exec(true).flags()).unwrap();

                loop {
                    let received = async_read_struct::<SignalFdInfo>(&sigfd, None).await;
                    match received {
                        Err(error) => panic!("Got error while reading from signalfd {}", error),
                        Ok(info) => {
                            notifier.send_system_event(SystemEvent::ApplicationSignal(info.signal()));
                        }
                    }
                }
            });

            async_spawn(async move {
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

                state.signal_coro.cancel();
            });
        });

        Ok(())
    }
}

#[derive(Debug)]
pub enum SystemEvent {
    ApplicationInit,
    ApplicationQuit,
    ApplicationSignal(Signal),
}

struct ApplicationStateNotifier {
    internal_queue_tx: AsyncChannelTx<SystemEvent>,
    notifier: AsyncSignal,
}

impl ApplicationStateNotifier {
    fn send_system_event(&mut self, event: SystemEvent) {
        self.internal_queue_tx.send(event);
        self.notifier.signal();
    }
}

pub struct ApplicationState<T> {
    internal_queue_rx: AsyncChannelRx<SystemEvent>,
    internal_queue_tx: AsyncChannelTx<SystemEvent>,
    app_queue_rx: AsyncChannelRx<T>,
    app_queue_tx: AsyncChannelTx<T>,
    has_event: AsyncSignal,
    signal_coro: TaskHandle<()>,
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
            signal_coro: TaskHandle::default(),
        }
    }

    fn create_notifier(&mut self) -> ApplicationStateNotifier {
        ApplicationStateNotifier {
            internal_queue_tx: self.internal_queue_tx.clone(),
            notifier: self.has_event.clone(),
        }
    }

    fn handle_system_event(&mut self, event: SystemEvent) -> bool {
        eprintln!("System event - {:?}", event);
        match event {
            SystemEvent::ApplicationQuit => return false,
            SystemEvent::ApplicationSignal(Signal::SIGQUIT) => return false,
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