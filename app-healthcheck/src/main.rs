use fbs_application::*;

#[derive(Debug)]
enum AppEvent {

}

#[derive(Debug)]
enum AppError {

}

struct HealthcheckApp {
}


impl ApplicationLogic for HealthcheckApp {
    type Error = AppError;
    type Event = AppEvent;

    fn create() -> Result<Self, Self::Error> {
        Ok(Self { })
    }

    fn handle_system_event(&mut self, event: SystemEvent) {
        eprintln!("App::handle_system_event - {:?}", event);
    }

    fn handle_app_event(&mut self, _state: &mut ApplicationState<Self::Event>, event: Self::Event) {
        eprintln!("App::handle_app_event - {:?}", event);
    }
}

fn main() {
    let mut app = Application::<HealthcheckApp>::create().unwrap();
    let _ = app.run();
}
