use std::marker::PhantomData;
use std::cell::Cell;
use std::rc::Rc;

use fbs_library::update_cell;
use fbs_runtime::*;
use fbs_runtime::async_utils::*;
use fbs_runtime::async_run;
use fbs_executor::TaskHandle;
use fbs_library::sigset::*;
use fbs_library::signalfd::*;

pub trait ApplicationResource {
    fn ping(&mut self) -> bool;
}

pub enum EventProcessing {
    Completed,
    Retry,
}

pub trait ApplicationLogic : Sized + 'static {
    type Event;
    type Error;

    fn create(notifier: ApplicationStateNotifier) -> Result<Self, Self::Error>;

    fn handle_system_event(&mut self, event: SystemEvent);

    async fn handle_app_event(&mut self, notifier: ApplicationStateNotifier, event: Self::Event) -> EventProcessing;

    fn get_resources(&mut self) -> Vec<&mut dyn ApplicationResource>;
}

pub struct Application<T: ApplicationLogic> {
    _marker: PhantomData<T>,
}

impl<T: ApplicationLogic> Application<T> {
    pub fn create() -> Result<Self, T::Error> {
        Ok(Self { _marker: PhantomData })
    }

    pub fn run(&mut self) -> Result<(), T::Error> {
        let state = Rc::new(ApplicationState::<T::Event>::new());
        let state_int = state.clone();

        let mut app = Rc::new(T::create(state.create_notifier())?);

        let notifier = state.create_notifier();
        notifier.send_system_event(SystemEvent::ApplicationInit);

        async_run(async move {
            state.signal_proc.set(async_spawn(async move {
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
            }));

            state.main_proc.set(async_spawn(async move {
                loop {
                    state_int.has_event.wait().await;

                    let mut resources = app.get_resources();
                    resources.iter_mut().for_each(|r| {
                        eprintln!("ping");
                        r.ping();
                    });

                    drop(resources);

                    if !state_int.internal_queue_rx.is_empty() {
                        let event = state_int.internal_queue_rx.receive().await;
                        let running = state_int.handle_system_event(event);
                        if !running {
                            break;
                        }

                        continue;
                    }

                    if !state_int.app_queue_rx.is_empty() {
                        let event = state_int.app_queue_rx.receive().await;

                        let state = state_int.clone();
                        async_spawn(async move {
                            let a = app.handle_app_event(state.create_notifier(), event).await;

                        });

                        continue;
                    }
                }

                update_cell(&state_int.signal_proc, |signal| { signal.cancel(); TaskHandle::default() });
            }));
        });

        Ok(())
    }
}

#[derive(Debug)]
pub enum SystemEvent {
    ApplicationInit,
    ApplicationQuit,
    ApplicationSignal(Signal),
    ResourceStateChanged,
}

#[derive(Clone)]
pub struct ApplicationStateNotifier {
    internal_queue_tx: AsyncChannelTx<SystemEvent>,
    notifier: AsyncSignal,
}

impl ApplicationStateNotifier {
    pub fn send_system_event(&self, event: SystemEvent) {
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
    signal_proc: Cell<TaskHandle<()>>,
    main_proc: Cell<TaskHandle<()>>,
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
            signal_proc: Cell::new(TaskHandle::default()),
            main_proc: Cell::new(TaskHandle::default()),
        }
    }

    fn create_notifier(&self) -> ApplicationStateNotifier {
        ApplicationStateNotifier {
            internal_queue_tx: self.internal_queue_tx.clone(),
            notifier: self.has_event.clone(),
        }
    }

    fn handle_system_event(&self, event: SystemEvent) -> bool {
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