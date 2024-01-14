use fbs_application::*;

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
        todo!()
    }

    fn handle_system_event(&mut self, event: SystemEvent) {
        todo!()
    }

    fn handle_app_event(&mut self, state: &mut ApplicationState<Self::Event>, event: Self::Event) {
        todo!()
    }
}

fn main() {
    let mut app = Application::<HealthcheckApp>::create().unwrap();
    let _ = app.run();
}
