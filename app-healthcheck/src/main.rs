use std::time::Duration;

use fbs_executor::*;
use fbs_application::*;
use fbs_amqp::*;
use fbs_runtime::*;

#[derive(Debug)]
enum AppEvent {

}

#[derive(Debug)]
enum AppError {

}

struct HealthcheckApp {
    amqp: AmqpResource,
}

struct AmqpResource {
    proc: TaskHandle<Result<AmqpConnection, AmqpConnectionError>>,
    connection: Option<AmqpConnection>,
    channel: Option<AmqpChannel>,
    notifier: ApplicationStateNotifier,
}

impl ApplicationResource for AmqpResource {
    fn ping(&mut self) -> bool {
        // connection alive
        if self.is_amqp_connection_alive() {
            eprintln!("connection alive");
            return true;
        }

        // connection in progress
        if self.is_amqp_connecting() {
            eprintln!("connection still connecting");
            return false;
        }

        let maybe_result = self.proc.result();
        match maybe_result {
            None => {
                eprintln!("Starting connection");
                self.start_connection()
            },
            Some(Ok(connection)) => {
                eprintln!("Connection established");
                self.connection = Some(connection);
                return true;
            },
            Some(Err(error)) => {
                eprintln!("Error while connecting to AMQP: {}. Reconnecting", error);
                self.start_connection();
            },
        }

        false
    }
}

impl AmqpResource {
    fn start_connection(&mut self) {
        eprintln!("Establishing AMQP connection");
        let notifier = self.notifier.clone();
        self.proc = async_spawn(async move {
            let mut params = AmqpConnectionParams::default();
            params.address = "localhost".to_string();
            params.username = "guest".to_string();
            params.password = "guest".to_string();
            params.vhost = "/".to_string();
            params.heartbeat = 5;

            let notifier2 = notifier.clone();
            params.on_error = Some(Box::new(move |err| {
                eprintln!("AMQP connection error: {}", err);
                notifier2.send_system_event(SystemEvent::ResourceStateChanged);
            }));

            let result = AmqpConnection::connect(params).await;
            if result.is_err() {
                async_sleep(Duration::new(2, 0)).await;
            }

            notifier.send_system_event(SystemEvent::ResourceStateChanged);

            result
        });
    }

    fn is_amqp_connection_alive(&self) -> bool {
        match &self.connection {
            None => false,
            Some(connection) => connection.is_alive(),
        }
    }

    fn is_amqp_connecting(&self) -> bool {
        !self.proc.is_completed()
    }
}

impl ApplicationLogic for HealthcheckApp {
    type Error = AppError;
    type Event = AppEvent;

    fn create(notifier: ApplicationStateNotifier) -> Result<Self, Self::Error> {
        let app = Self {
            amqp: AmqpResource {
                proc: TaskHandle::default(),
                connection: None,
                channel: None,
                notifier,
            },
        };

        Ok(app)
    }

    fn handle_system_event(&mut self, event: SystemEvent) {
        eprintln!("App::handle_system_event - {:?}", event);
    }

    async fn handle_app_event(&mut self, _notifier: ApplicationStateNotifier, event: Self::Event) -> EventProcessing {
        eprintln!("App::handle_app_event - {:?}", event);
        EventProcessing::Completed
    }

    fn get_resources(&mut self) -> Vec<&mut dyn ApplicationResource> {
        return vec![
            &mut self.amqp
        ];
    }
}

fn main() {
    let mut app = Application::<HealthcheckApp>::create().unwrap();
    let _ = app.run();
}
