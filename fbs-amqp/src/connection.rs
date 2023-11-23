use std::cell::{Cell, RefCell, Ref};
use std::cmp::min;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use fbs_library::socket::{Socket, SocketDomain, SocketType, SocketFlags};
use fbs_library::system_error::SystemError;
use fbs_library::indexed_list::IndexedList;
use fbs_runtime::async_utils::{AsyncSignal, AsyncChannelRx, AsyncChannelTx, async_channel_create};
use fbs_runtime::{async_connect, async_write, async_read_into, async_spawn};
use fbs_runtime::resolver::{resolve_address, ResolveAddressError};
use fbs_executor::TaskHandle;

use super::{AmqpDeleteExchangeFlags, AmqpExchangeFlags, AmqpQueueFlags, AmqpDeleteQueueFlags, AmqpConsumeFlags, AmqpPublishFlags, AmqpNackFlags};
use super::defines::AMQP_CLASS_BASIC;
use super::frame::{AmqpProtocolHeader, AmqpFrame, AmqpFrameError, AmqpFramePayload, AmqpMethod, AmqpBasicProperties, AmqpMessage};
use super::frame_reader::AmqpFrameReader;
use super::frame_writer::FrameWriter;

use thiserror::Error;

const FRAME_EXTRA_SIZE: u32 = 8;  // size of frame header and footer

pub type AmqpConsumer = Box<dyn Fn(u64, bool, String, String, AmqpMessage)>;
pub type AmqpConfirmAckCallback = Box<dyn Fn(u64, bool)>;
pub type AmqpConfirmNackCallback = Box<dyn Fn(u64, AmqpNackFlags)>;

#[derive(Error, Debug, Clone)]
pub enum AmqpConnectionError {
    #[error("AMQP address incorrect")]
    AddressIncorrect(#[from] ResolveAddressError),
    #[error("Connect error")]
    ConnectError(SystemError),
    #[error("Write error")]
    WriteError(SystemError),
    #[error("Read error")]
    ReadError(SystemError),
    #[error("Connection closed")]
    ConnectionClosed,
    #[error("Invalid type frame")]
    FrameTypeUnknown(u8),
    #[error("Invalid frame end")]
    FrameEndInvalid,
    #[error("Frame error: {0}")]
    FrameError(#[from] AmqpFrameError),
    #[error("Connection closed by server - {1}")]
    ConnectionClosedByServer(u16, String, u16, u16),
    #[error("Protocol error")]
    ProtocolError(&'static str),
    #[error("Channel closed by server - {1}")]
    ChannelClosedByServer(u16, String, u16, u16),
    #[error("Invalid parameters")]
    InvalidParameters,
}

pub struct AmqpConnection {
    ptr: Rc<AmqpConnectionInternal>,
    address: String,
}

pub struct AmqpChannel {
    ptr: Rc<AmqpChannelInternals>,
}

impl AmqpChannel {
    fn new(connection: Rc<AmqpConnectionInternal>) -> Self {
        Self { ptr: Rc::new(AmqpChannelInternals::new(connection)) }
    }

    pub fn publisher(&self) -> AmqpChannelPublisher {
        AmqpChannelPublisher { 
            ptr: self.ptr.clone(),
        }
    }

    pub fn set_on_return(&mut self, callback: Option<Box<dyn Fn(i16, String, String, String, AmqpMessage)>>) {
        *self.ptr.on_return.borrow_mut() = callback;
    }

    pub async fn close(self) -> Result<(), AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::ChannelClose(0, "shutdown".to_string(), 0, 0)),
        };

        self.ptr.connection.writer_queue.send(Some(frame));

        self.ptr.wait_list.channel_close_ok.set(true);
        self.ptr.rx.receive().await?;

        self.ptr.connection.clear_channel(self.ptr.number.get());

        Ok(())
    }

    pub async fn flow(&mut self, active: bool) -> Result<(), AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::ChannelFlow(active)),
        };

        self.ptr.connection.writer_queue.send(Some(frame));

        self.ptr.wait_list.channel_flow_ok.set(true);
        self.ptr.rx.receive().await?;

        Ok(())
    }

    pub async fn declare_exchange(&mut self, name: String, exchange_type: String, flags: AmqpExchangeFlags) -> Result<(), AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::ExchangeDeclare(name, exchange_type, flags.into(), HashMap::new())),
        };

        self.ptr.connection.writer_queue.send(Some(frame));

        if !flags.has_no_wait() {
            self.ptr.wait_list.exchange_declare_ok.set(true);
            self.ptr.rx.receive().await?;
        }

        Ok(())
    }

    pub async fn delete_exchange(&mut self, name: String, flags: AmqpDeleteExchangeFlags) -> Result<(), AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::ExchangeDelete(name, flags.into())),
        };

        self.ptr.connection.writer_queue.send(Some(frame));

        if !flags.has_no_wait() {
            self.ptr.wait_list.exchange_delete_ok.set(true);
            self.ptr.rx.receive().await?;
        }

        Ok(())
    }

    pub async fn declare_queue(&mut self, name: String, flags: AmqpQueueFlags) -> Result<(String, i32, i32), AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::QueueDeclare(name, flags.into(), HashMap::new())),
        };

        self.ptr.connection.writer_queue.send(Some(frame));

        if !flags.has_no_wait() {
            self.ptr.wait_list.queue_declare_ok.set(true);
            let frame = self.ptr.rx.receive().await?;
            match frame.payload {
                AmqpFramePayload::Method(AmqpMethod::QueueDeclareOk(name, messages, consumers)) => Ok((name, messages, consumers)),
                _ => Err(AmqpConnectionError::ProtocolError("queue.declare-ok frame expected")),
            }
        } else {
            Ok((String::new(), 0, 0))
        }
    }

    pub async fn bind_queue(&mut self, name: String, exchange: String, routing_key: String, no_wait: bool) -> Result<(), AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::QueueBind(name, exchange, routing_key, no_wait as u8, HashMap::new())),
        };

        self.ptr.connection.writer_queue.send(Some(frame));

        if !no_wait {
            self.ptr.wait_list.queue_bind_ok.set(true);
            self.ptr.rx.receive().await?;
        }

        Ok(())
    }

    pub async fn unbind_queue(&mut self, name: String, exchange: String, routing_key: String) -> Result<(), AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::QueueUnbind(name, exchange, routing_key, HashMap::new())),
        };

        self.ptr.connection.writer_queue.send(Some(frame));
        self.ptr.wait_list.queue_unbind_ok.set(true);
        self.ptr.rx.receive().await?;

        Ok(())
    }

    pub async fn purge_queue(&mut self, name: String, no_wait: bool) -> Result<i32, AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::QueuePurge(name, no_wait as u8)),
        };

        self.ptr.connection.writer_queue.send(Some(frame));

        if !no_wait {
            self.ptr.wait_list.queue_purge_ok.set(true);
            let frame = self.ptr.rx.receive().await?;
            match frame.payload {
                AmqpFramePayload::Method(AmqpMethod::QueuePurgeOk(messages)) => Ok(messages),
                _ => Err(AmqpConnectionError::ProtocolError("queue.purge-ok frame expected")),
            }
        } else {
            Ok(0)
        }
    }

    pub async fn delete_queue(&mut self, name: String, flags: AmqpDeleteQueueFlags) -> Result<i32, AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::QueueDelete(name, flags.into())),
        };

        self.ptr.connection.writer_queue.send(Some(frame));

        if !flags.has_no_wait() {
            self.ptr.wait_list.queue_delete_ok.set(true);
            let frame = self.ptr.rx.receive().await?;
            match frame.payload {
                AmqpFramePayload::Method(AmqpMethod::QueueDeleteOk(messages)) => Ok(messages),
                _ => Err(AmqpConnectionError::ProtocolError("queue.delete-ok frame expected")),
            }
        } else {
            Ok(0)
        }
    }

    pub async fn qos(&mut self, prefetch_size: i32, prefetch_count: i16, global: bool) -> Result<(), AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::BasicQos(prefetch_size, prefetch_count, global)),
        };

        self.ptr.connection.writer_queue.send(Some(frame));
        self.ptr.wait_list.basic_qos_ok.set(true);
        self.ptr.rx.receive().await?;

        Ok(())
    }

    pub async fn recover(&mut self, requeue: bool) -> Result<(), AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::BasicRecover(requeue)),
        };

        self.ptr.connection.writer_queue.send(Some(frame));
        self.ptr.wait_list.basic_recover_ok.set(true);
        self.ptr.rx.receive().await?;

        Ok(())
    }

    pub async fn get(&mut self, queue: String, no_ack: bool) -> Result<Option<(u64, bool, String, String, u32, AmqpMessage)>, AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::BasicGet(queue, no_ack)),
        };

        self.ptr.connection.writer_queue.send(Some(frame));
        self.ptr.wait_list.basic_get.set(true);

        let frame = self.ptr.rx.receive().await?;
        match frame.payload {
            AmqpFramePayload::Method(AmqpMethod::BasicGetEmpty()) => Ok(None),
            AmqpFramePayload::Method(AmqpMethod::BasicGetOk(delivery_tag, redelivered, exchange, routing_key, messages)) => {
                Ok(Some((delivery_tag, redelivered, exchange, routing_key, messages, self.ptr.message_rx.receive().await?)))
            },
            _ => Err(AmqpConnectionError::ProtocolError("basic.consume-ok frame expected")),
        }
    }

    pub async fn confirm_select(&mut self, callbacks: (AmqpConfirmAckCallback, AmqpConfirmNackCallback), no_wait: bool) -> Result<(), AmqpConnectionError> {
        self.ptr.is_channel_valid()?;
        *self.ptr.confirm_callbacks.borrow_mut() = Some(callbacks);

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::ConfirmSelect(no_wait)),
        };

        self.ptr.connection.writer_queue.send(Some(frame));

        if !no_wait {
            self.ptr.wait_list.confirm_select_ok.set(true);
            let frame = self.ptr.rx.receive().await?;
            match frame.payload {
                AmqpFramePayload::Method(AmqpMethod::ConfirmSelectOk()) => {
                    Ok(())
                },
                _ => Err(AmqpConnectionError::ProtocolError("confirm.select-ok frame expected")),
            }
        } else {
            Ok(())
        }
    }

    pub async fn consume(&mut self, queue: String, tag: String, callback: AmqpConsumer, flags: AmqpConsumeFlags) -> Result<String, AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        // With no-wait with empty tag makes no sense, as with no reply it's not possible to know the consumer tag
        if tag.is_empty() && flags.has_no_wait() {
            return Err(AmqpConnectionError::InvalidParameters);
        }

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::BasicConsume(queue, tag.clone(), flags.into(), HashMap::new())),
        };
        
        self.ptr.connection.writer_queue.send(Some(frame));

        if !flags.has_no_wait() {
            self.ptr.wait_list.basic_consume_ok.set(true);
            self.ptr.install_consumer.set(Some(callback));
            let frame = self.ptr.rx.receive().await?;
            match frame.payload {
                AmqpFramePayload::Method(AmqpMethod::BasicConsumeOk(tag)) => {
                    Ok(tag)
                },
                _ => Err(AmqpConnectionError::ProtocolError("basic.consume-ok frame expected")),
            }
        } else {
            self.ptr.consumers.borrow_mut().insert(tag, callback);
            Ok(String::new())
        }
    }

    pub async fn cancel(&mut self, tag: String, no_wait: bool) -> Result<String, AmqpConnectionError> {
        self.ptr.is_channel_valid()?;

        if no_wait {
            self.ptr.consumers.borrow_mut().remove(&tag);
        }

        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::BasicCancel(tag, no_wait as u8)),
        };

        self.ptr.connection.writer_queue.send(Some(frame));

        if !no_wait {
            self.ptr.wait_list.basic_cancel_ok.set(true);
            let frame = self.ptr.rx.receive().await?;
            match frame.payload {
                AmqpFramePayload::Method(AmqpMethod::BasicCancelOk(tag)) => {
                    self.ptr.consumers.borrow_mut().remove(&tag);
                    Ok(tag)
                },
                _ => Err(AmqpConnectionError::ProtocolError("basic.cancel-ok frame expected")),
            }
        } else {
            Ok(String::new())
        }
    }

    pub fn publish(&self, exchange: String, routing_key: String, properties: AmqpBasicProperties, flags: AmqpPublishFlags, content: &[u8]) -> Result<(), AmqpConnectionError> {
        self.ptr.publish(exchange, routing_key, properties, flags, content)
    }

    pub fn ack(&self, delivery_tag: u64, multiple: bool) {
        self.ptr.ack(delivery_tag, multiple)
    }

    pub fn reject(&self, delivery_tag: u64, requeue: bool) {
        self.ptr.reject(delivery_tag, requeue)
    }

    pub fn nack(&self, delivery_tag: u64, flags: AmqpNackFlags) {
        self.ptr.nack(delivery_tag, flags)
    }    
}

#[derive(Clone)]
pub struct AmqpChannelPublisher {
    ptr: Rc<AmqpChannelInternals>,
}

impl AmqpChannelPublisher {
    pub fn ack(&self, delivery_tag: u64, multiple: bool) {
        self.ptr.ack(delivery_tag, multiple)
    }

    pub fn reject(&self, delivery_tag: u64, requeue: bool) {
        self.ptr.reject(delivery_tag, requeue)
    }

    pub fn nack(&self, delivery_tag: u64, flags: AmqpNackFlags) {
        self.ptr.nack(delivery_tag, flags)
    }

    pub fn publish(&self, exchange: String, routing_key: String, properties: AmqpBasicProperties, flags: AmqpPublishFlags, content: &[u8]) -> Result<(), AmqpConnectionError> {
        self.ptr.publish(exchange, routing_key, properties, flags, content)
    }
}

struct AmqpChannelInternals {
    connection: Rc<AmqpConnectionInternal>,
    rx: AsyncChannelRx<Result<AmqpFrame, AmqpConnectionError>>,
    tx: AsyncChannelTx<Result<AmqpFrame, AmqpConnectionError>>,
    message_rx: AsyncChannelRx<Result<AmqpMessage, AmqpConnectionError>>,
    message_tx: AsyncChannelTx<Result<AmqpMessage, AmqpConnectionError>>,
    wait_list: FrameWaiter,
    number: Cell<usize>,
    active: Cell<bool>,
    last_error: RefCell<Option<AmqpConnectionError>>,
    on_return: RefCell<Option<Box<dyn Fn(i16, String, String, String, AmqpMessage)>>>,
    message_in_flight: RefCell<AmqpMessageBuilder>,
    consumers: RefCell<HashMap<String, AmqpConsumer>>,
    install_consumer: Cell<Option<AmqpConsumer>>,
    confirm_callbacks: RefCell<Option<(AmqpConfirmAckCallback, AmqpConfirmNackCallback)>>,
}

#[derive(Debug, Default)]
struct FrameWaiter {
    channel_open_ok: Cell<bool>,
    channel_close_ok: Cell<bool>,
    channel_flow_ok: Cell<bool>,
    exchange_declare_ok: Cell<bool>,
    exchange_delete_ok: Cell<bool>,
    queue_declare_ok: Cell<bool>,
    queue_bind_ok: Cell<bool>,
    queue_unbind_ok: Cell<bool>,
    queue_purge_ok: Cell<bool>,
    queue_delete_ok: Cell<bool>,
    basic_qos_ok: Cell<bool>,
    basic_consume_ok: Cell<bool>,
    basic_cancel_ok: Cell<bool>,
    basic_get: Cell<bool>,
    basic_recover_ok: Cell<bool>,
    confirm_select_ok: Cell<bool>,
}

impl AmqpChannelInternals {
    fn new(connection: Rc<AmqpConnectionInternal>) -> Self {
        let (rx, tx) = async_channel_create();
        let (message_rx, message_tx) = async_channel_create();
        
        Self {
            connection,
            wait_list: FrameWaiter::default(),
            number: Cell::new(0),
            active: Cell::new(true),
            rx,
            tx,
            message_rx,
            message_tx,
            last_error: RefCell::new(None),
            on_return: RefCell::new(None),
            message_in_flight: RefCell::new(AmqpMessageBuilder::default()),
            consumers: RefCell::new(HashMap::new()),
            install_consumer: Cell::new(None),
            confirm_callbacks: RefCell::new(None),
        }
    }

    fn publish(&self, exchange: String, routing_key: String, properties: AmqpBasicProperties, flags: AmqpPublishFlags, mut content: &[u8]) -> Result<(), AmqpConnectionError> {
        self.is_channel_valid()?;

        let frame = AmqpFrame {
            channel: self.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::BasicPublish(exchange, routing_key, flags.into())),
        };

        self.connection.writer_queue.send(Some(frame));

        let frame = AmqpFrame {
            channel: self.number.get() as u16,
            payload: AmqpFramePayload::Header(AMQP_CLASS_BASIC, content.len() as u64, properties),
        };

        self.connection.writer_queue.send(Some(frame));

        let mut total_bytes_to_send = content.len();
        while total_bytes_to_send > 0 {
            let bytes_in_frame = min(total_bytes_to_send, self.connection.max_frame_size.get() as usize);

            let frame = AmqpFrame {
                channel: self.number.get() as u16,
                payload: AmqpFramePayload::Content(content[..bytes_in_frame].to_vec()),
            };

            self.connection.writer_queue.send(Some(frame));
            content = &content[bytes_in_frame..];
            total_bytes_to_send -= bytes_in_frame;
        }

        Ok(())
    }

    fn ack(&self, delivery_tag: u64, multiple: bool) {
        let frame = AmqpFrame {
            channel: self.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::BasicAck(delivery_tag, multiple)),
        };

        self.connection.writer_queue.send(Some(frame));
    }

    fn reject(&self, delivery_tag: u64, requeue: bool) {
        let frame = AmqpFrame {
            channel: self.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::BasicReject(delivery_tag, requeue)),
        };

        self.connection.writer_queue.send(Some(frame));
    }

    fn nack(&self, delivery_tag: u64, flags: AmqpNackFlags) {
        let frame = AmqpFrame {
            channel: self.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::BasicNack(delivery_tag, flags.into())),
        };

        self.connection.writer_queue.send(Some(frame));
    }    

    fn is_channel_valid(&self) -> Result<(), AmqpConnectionError> {
        let last_error = self.last_error.borrow();
        match *last_error {
            None => self.connection.is_connection_valid(),
            Some(ref error) => Err(error.clone()),
        }
    }

    fn handle_frame(&self, frame: AmqpFrame) -> Result<(), AmqpConnectionError> {
        match frame.payload {
            AmqpFramePayload::Header(_, size, properties) => {
                self.message_in_flight.borrow_mut().prepare_from_header(size, properties)?;
                Ok(())
            },
            AmqpFramePayload::Content(data) => {
                self.message_in_flight.borrow_mut().append_data(&data)?;
                let frame = self.message_in_flight.borrow_mut().build_if_completed()?;
                match frame {
                    None | Some((MessageDeliveryMode::None, _))=> (),
                    Some((MessageDeliveryMode::Return(code, reason, class, method), message)) => {
                        match &*self.on_return.borrow_mut() {
                            None => (),
                            Some(callback) => {
                                callback(code, reason, class, method, message);
                            },
                        }
                    },
                    Some((MessageDeliveryMode::Deliver(consumer_tag, delivery_tag, redelivered, exchange, routing_key), message)) => {
                        let consumers = self.consumers.borrow();
                        let consumer = consumers.get(&consumer_tag);
                        
                        match consumer {
                            None => eprintln!("Received message with consumer tag {}, but no consumer installed", consumer_tag),
                            Some(callback) => {
                                callback(delivery_tag, redelivered, exchange, routing_key, message);
                            },
                        }
                    },
                    Some((MessageDeliveryMode::Get, message)) => {
                        self.message_tx.send(Ok(message));
                    },
                };

                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::ChannelClose(code, reason, class, method)) => {
                let error = AmqpConnectionError::ChannelClosedByServer(code, reason, class, method);
                *self.last_error.borrow_mut() = Some(error.clone());
                self.tx.send(Err(error.clone()));
                Err(error)
            },
            AmqpFramePayload::Method(AmqpMethod::BasicReturn(code, reason, exchange, routing_key)) => {
                self.message_in_flight.borrow_mut().prepare_mode(MessageDeliveryMode::Return(code, reason, exchange, routing_key))?;
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::BasicDeliver(consumer_tag, delivery_tag, redelivered, exchange, routing_key)) => {
                self.message_in_flight.borrow_mut().prepare_mode(MessageDeliveryMode::Deliver(consumer_tag, delivery_tag, redelivered, exchange, routing_key))?;
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::ChannelCloseOk()) if self.wait_list.channel_close_ok.get() => {
                self.wait_list.channel_close_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::ChannelOpenOk()) if self.wait_list.channel_open_ok.get() => {
                self.wait_list.channel_open_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::ChannelFlow(active)) => {
                self.active.set(active);

                let frame = AmqpFrame {
                    channel: self.number.get() as u16,
                    payload: AmqpFramePayload::Method(AmqpMethod::ChannelFlowOk(active)),
                };

                self.connection.writer_queue.send(Some(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::ChannelFlowOk(_)) if self.wait_list.channel_flow_ok.get() => {
                self.wait_list.channel_flow_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::ExchangeDeclareOk()) if self.wait_list.exchange_declare_ok.get() => {
                self.wait_list.exchange_declare_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::ExchangeDeleteOk()) if self.wait_list.exchange_delete_ok.get() => {
                self.wait_list.exchange_delete_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::QueueDeclareOk(_, _, _)) if self.wait_list.queue_declare_ok.get() => {
                self.wait_list.queue_declare_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::QueueBindOk()) if self.wait_list.queue_bind_ok.get() => {
                self.wait_list.queue_bind_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::QueueUnbindOk()) if self.wait_list.queue_unbind_ok.get() => {
                self.wait_list.queue_unbind_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::QueuePurgeOk(_)) if self.wait_list.queue_purge_ok.get() => {
                self.wait_list.queue_purge_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::QueueDeleteOk(_)) if self.wait_list.queue_delete_ok.get() => {
                self.wait_list.queue_delete_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::BasicQosOk()) if self.wait_list.basic_qos_ok.get() => {
                self.wait_list.basic_qos_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::BasicRecoverOk()) if self.wait_list.basic_recover_ok.get() => {
                self.wait_list.basic_recover_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },            
            AmqpFramePayload::Method(AmqpMethod::BasicConsumeOk(ref tag)) if self.wait_list.basic_consume_ok.get() => {
                self.wait_list.basic_consume_ok.set(false);
                match self.install_consumer.take() {
                    None => panic!("Received basic.consume-ok with no consumer callback pending"),
                    Some(callback) => {
                        self.consumers.borrow_mut().insert(tag.clone(), callback);
                    }
                };

                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::BasicCancelOk(_)) if self.wait_list.basic_cancel_ok.get() => {
                self.wait_list.basic_cancel_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::BasicGetOk(_, _, _, _, _)) if self.wait_list.basic_get.get() => {
                self.message_in_flight.borrow_mut().prepare_mode(MessageDeliveryMode::Get)?;
                self.wait_list.basic_get.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::BasicGetEmpty()) if self.wait_list.basic_get.get() => {
                self.wait_list.basic_get.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::ConfirmSelectOk()) if self.wait_list.confirm_select_ok.get() => {
                self.wait_list.confirm_select_ok.set(false);
                self.tx.send(Ok(frame));
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::BasicAck(delivery_tag, multiple)) => {
                self.on_ack(delivery_tag, multiple);
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::BasicNack(delivery_tag, flags)) => {
                self.on_nack(delivery_tag, flags.into());
                Ok(())
            },
            _ => Ok(()),
        }
    }

    fn on_ack(&self, delivery_tag: u64, multiple: bool) {
        match &*self.confirm_callbacks.borrow() {
            None => eprintln!("Received basic.on-ack without confirm callbacks"),
            Some((on_ack, _)) => {
                on_ack(delivery_tag, multiple);
            },
        }
    }

    fn on_nack(&self, delivery_tag: u64, flags: AmqpNackFlags) {
        match &*self.confirm_callbacks.borrow() {
            None => eprintln!("Received basic.on-ack without confirm callbacks"),
            Some((_, on_nack)) => {
                on_nack(delivery_tag, flags);
            },
        }
    }
}

impl AmqpConnection {
    pub fn new(address: String) -> Self {
        Self { ptr: Rc::new(AmqpConnectionInternal::new()), address }
    }

    pub async fn connect(&mut self, username: &str, password: &str) -> Result<(), AmqpConnectionError> {
        let result = self.ptr.connect(&self.address, username, password, self.ptr.clone()).await;

        result
    }

    pub async fn channel_open(&mut self) -> Result<AmqpChannel, AmqpConnectionError> {
        self.ptr.is_connection_valid()?;

        let channel = AmqpChannel::new(self.ptr.clone());
        let index = self.ptr.set_channel(&channel);
        channel.ptr.number.set(index);

        let frame = AmqpFrame {
            channel: index as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::ChannelOpen()),
        };

        self.ptr.writer_queue.send(Some(frame));
        channel.ptr.wait_list.channel_open_ok.set(true);
        channel.ptr.rx.receive().await?;

        Ok(channel)
    }

    pub async fn close(self) {
        if self.ptr.is_connection_valid().is_err() {
            return;
        }

        let frame = AmqpFrame {
            channel: 0,
            payload: AmqpFramePayload::Method(AmqpMethod::ConnectionClose(0, "shutdown".to_string(), 0, 0)),
        };

        self.ptr.writer_queue.send(Some(frame));
        self.ptr.signal.wait().await;
    }
}

impl Drop for AmqpConnection {
    fn drop(&mut self) {
        self.ptr.mark_connection_closed(AmqpConnectionError::ConnectionClosed, false);
    }
}

struct AmqpConnectionReader {
    fd: Rc<Socket>,
    read_buffer: Vec<u8>,
    read_offset: usize,
}

impl AmqpConnectionReader {
    fn new(fd: Rc<Socket>) -> Self {
        Self { fd, read_buffer: Vec::with_capacity(4096), read_offset: 0 }
    }

    fn change_capacity(&mut self, size: usize) {
        assert!(self.read_buffer.capacity() <= size);
        self.read_buffer.reserve(size - self.read_buffer.capacity());
    }

    async fn fill_buffer(&mut self) -> Result<usize, AmqpConnectionError> {
        if self.read_offset < self.read_buffer.len() {
            return Ok(self.read_buffer.len() - self.read_offset);
        }

        self.read_offset = 0;
        let result = async_read_into(&self.fd, std::mem::take(&mut self.read_buffer), None).await;
        match result {
            Err((error, _)) => return Err(AmqpConnectionError::ReadError(error)),
            Ok(buffer) => self.read_buffer = buffer,
        }

        if self.read_buffer.is_empty() {
            return Err(AmqpConnectionError::ConnectionClosed);
        }

        Ok(self.read_buffer.len())
    }

    async fn read_bytes(&mut self, mut target: &mut [u8]) -> Result<(), AmqpConnectionError> {
        let mut target_size = target.len();

        while target_size > 0 {
            let bytes_available = self.fill_buffer().await?;

            let to_copy = min(target_size, bytes_available);
            target.clone_from_slice(&self.read_buffer[self.read_offset .. self.read_offset + to_copy]);
            self.read_offset += to_copy;
            target_size -= to_copy;

            target = &mut target[to_copy..];
        }

        Ok(())
    }

    async fn read_u8(&mut self) -> Result<u8, AmqpConnectionError> {
        let mut bytes: [u8; 1] = [0; 1];
        self.read_bytes(&mut bytes).await?;

        Ok(u8::from_be_bytes(bytes))
    }

    async fn read_u16(&mut self) -> Result<u16, AmqpConnectionError> {
        let mut bytes: [u8; 2] = [0; 2];
        self.read_bytes(&mut bytes).await?;

        Ok(u16::from_be_bytes(bytes))
    }

    async fn read_u32(&mut self) -> Result<u32, AmqpConnectionError> {
        let mut bytes: [u8; 4] = [0; 4];
        self.read_bytes(&mut bytes).await?;

        Ok(u32::from_be_bytes(bytes))
    }

    async fn read_frame(&mut self) -> Result<AmqpFrame, AmqpConnectionError> {
        let frame_type = self.read_u8().await?;
        let channel = self.read_u16().await?;
        let payload_size = self.read_u32().await? as usize;

        let mut payload = Vec::with_capacity(payload_size);
        payload.resize(payload_size, b'\x00');

        self.read_bytes(&mut payload).await?;

        let frame_end = self.read_u8().await?;
        if frame_end != b'\xCE' {
            return Err(AmqpConnectionError::FrameEndInvalid);
        }

        let mut reader = AmqpFrameReader::new(&payload);
        Ok(reader.read_frame(frame_type, channel)?)
    }
}

struct AmqpConnectionWriter {
    fd: Rc<Socket>,
    queue: VecDeque<AmqpFrame>,
}

impl AmqpConnectionWriter {
    fn new(fd: Rc<Socket>) -> Self {
        Self { fd, queue: VecDeque::new() }
    }

    fn enqueue_frame(&mut self, frame: AmqpFrame) {
        self.queue.push_back(frame);
    }

    async fn flush_all(&mut self) -> Result<(), AmqpConnectionError> {
        loop {
            let frame = self.queue.pop_front();
            match frame {
                None => return Ok(()),
                Some(frame) => self.write_frame(&frame).await?,
            }
        }
    }

    async fn write_frame(&mut self, frame: &AmqpFrame) -> Result<(), AmqpConnectionError> {
        let data = FrameWriter::write_frame(frame);
        let result = async_write(&self.fd, data, None).await;

        match result {
            Ok(_) => (),
            Err((error, _)) => return Err(AmqpConnectionError::WriteError(error)),
        }

        dbg!(&frame);

        Ok(())
    }
}

pub struct AmqpConnectionInternal {
    fd: Rc<Socket>,
    channels: RefCell<IndexedList<Rc<AmqpChannelInternals>>>,
    writer_queue: AsyncChannelTx<Option<AmqpFrame>>,
    read_handler: Cell<TaskHandle<()>>,
    write_handler: Cell<TaskHandle<()>>,
    signal: AsyncSignal,
    max_channels: Cell<u16>,
    max_frame_size: Cell<u32>,
    heartbeat: Cell<u16>,
    last_error: RefCell<Option<AmqpConnectionError>>,
}

impl AmqpConnectionInternal {
    fn new() -> Self {
        let (_, tx) = async_channel_create();
        AmqpConnectionInternal {
            fd: Rc::new(Socket::new(SocketDomain::Inet, SocketType::Stream, SocketFlags::new().close_on_exec(true).flags())),
            channels: RefCell::new(IndexedList::default()),
            writer_queue: tx,
            read_handler: Cell::new(TaskHandle::default()),
            write_handler: Cell::new(TaskHandle::default()),
            signal: AsyncSignal::new(),
            max_channels: Cell::new(100),
            max_frame_size: Cell::new(4096),
            heartbeat: Cell::new(0),
            last_error: RefCell::new(None),
        }
    }

    fn is_connection_valid(&self) -> Result<(), AmqpConnectionError> {
        let last_error = self.last_error.borrow();
        match *last_error {
            None => Ok(()),
            Some(ref error) => Err(error.clone()),
        }
    }

    fn set_channel(&self, channel: &AmqpChannel) -> usize {
        self.channels.borrow_mut().insert(channel.ptr.clone()) + 1
    }

    fn clear_channel(&self, index: usize) {
        self.channels.borrow_mut().remove(index - 1);
    }

    fn handle_channel_frame(&self, frame: AmqpFrame) -> Result<(), AmqpConnectionError> {
        let index = frame.channel as usize;
        let mut channels = self.channels.borrow_mut();

        let channel = channels.get_mut(index - 1);
        let mut close_channel = false;
        let result = match channel {
            None => Ok(()),
            Some(channel) => {
                let result = channel.handle_frame(frame);
                match result {
                    Ok(_) => result,
                    Err(AmqpConnectionError::ChannelClosedByServer(_, _, _, _)) => {
                        close_channel = true;
                        Ok(())
                    },
                    Err(_) => {
                        close_channel = true;
                        result
                    },
                }
            }
        };

        drop(channels);
        if close_channel {
            self.clear_channel(index);
        }

        result
    }

    fn handle_connection_frame(&self, frame: AmqpFrame) -> Result<(), AmqpConnectionError> {
        match frame.payload {
            AmqpFramePayload::Method(AmqpMethod::ConnectionClose(code, reason, class, method)) => {
                self.mark_connection_closed(AmqpConnectionError::ConnectionClosedByServer(code, reason, class, method), false);
                self.signal.signal();
            },
            AmqpFramePayload::Method(AmqpMethod::ConnectionCloseOk()) => {
                self.mark_connection_closed(AmqpConnectionError::ConnectionClosed, false);
                self.signal.signal();
            },
            _ => (),
        }

        Ok(())
    }

    fn mark_connection_closed(&self, error: AmqpConnectionError, send_close_frame: bool) {
        if self.last_error.borrow().is_none() {
            *self.last_error.borrow_mut() = Some(error.clone());
            if send_close_frame {
                let close_frame = AmqpFrame {
                    channel: 0,
                    payload: AmqpFramePayload::Method(AmqpMethod::ConnectionClose(400, "protocol-error".to_string(), 0, 0)),
                };

                self.writer_queue.clear();
                self.writer_queue.send(Some(close_frame));
            }

            self.writer_queue.send(None);

            let channels = self.channels.borrow();
            channels.iter().for_each(|channel| {
                match channel {
                    None => (),
                    Some(channel) => channel.tx.send(Err(error.clone())),
                }
            });
        }
    }

    async fn connect(&self, address: &str, username: &str, password: &str, self_ptr: Rc<AmqpConnectionInternal>) -> Result<(), AmqpConnectionError> {
        let address = resolve_address(address, Some(5672)).await?;
        let connected = async_connect(&self.fd, address).await;
        match connected {
            Ok(_) => (),
            Err(error) => return Err(AmqpConnectionError::ConnectError(error)),
        };

        let written = async_write(&self.fd, AmqpProtocolHeader::new().into(), None).await;
        match written {
            Ok(_) => (),
            Err((error, _)) => return Err(AmqpConnectionError::WriteError(error)),
        }

        let mut reader = AmqpConnectionReader::new(self.fd.clone());
        let mut writer = AmqpConnectionWriter::new(self.fd.clone());

        let _ = reader.read_frame().await?;

        let mut sasl = String::new();
        sasl.push('\x00');
        sasl.push_str(username);
        sasl.push('\x00');
        sasl.push_str(password);

        let response = AmqpFrame {
            channel: 0,
            payload: AmqpFramePayload::Method(AmqpMethod::ConnectionStartOk(HashMap::new(), "PLAIN".to_string(), sasl, String::new())),
        };

        writer.enqueue_frame(response);
        writer.flush_all().await?;

        let frame = reader.read_frame().await?;
        match &frame.payload {
            AmqpFramePayload::Method(AmqpMethod::ConnectionTune(channels, frame, heartbeat)) => {
                if *frame > 0 {
                    reader.change_capacity((*frame) as usize);

                    // no real server is going to send value so small, but we want to
                    // prevent an underflow
                    if *frame > FRAME_EXTRA_SIZE {
                        self.max_frame_size.set((*frame) - FRAME_EXTRA_SIZE);
                    }
                }

                self.max_channels.set(*channels);
                self.heartbeat.set(*heartbeat);
            },
            _ => (),
        }

        dbg!(&frame);
        let response = AmqpFrame {
            channel: 0,
            payload: AmqpFramePayload::Method(AmqpMethod::ConnectionTuneOk(self.max_channels.get(), self.max_frame_size.get(), self.heartbeat.get())),
        };

        writer.enqueue_frame(response);
        writer.flush_all().await?;

        let response = AmqpFrame {
            channel: 0,
            payload: AmqpFramePayload::Method(AmqpMethod::ConnectionOpen("/".to_string())),
        };

        writer.enqueue_frame(response);
        writer.flush_all().await?;

        let _ = reader.read_frame().await?;

        self.start_io_handler(writer, self.writer_queue.rx(), reader, self_ptr);
        Ok(())
    }

    fn start_io_handler(&self, mut writer: AmqpConnectionWriter, writer_channel: AsyncChannelRx<Option<AmqpFrame>>, mut reader: AmqpConnectionReader, connection: Rc<AmqpConnectionInternal>) {

        self.read_handler.set(async_spawn(async move {
            while connection.last_error.borrow().is_none() {
                let frame = reader.read_frame().await;
                match frame {
                    Ok(frame) => {
                        dbg!(&frame);
                        let handle_result = if frame.channel > 0 {
                            connection.handle_channel_frame(frame)
                        } else {
                            connection.handle_connection_frame(frame)
                        };

                        match handle_result {
                            Ok(_) => (),
                            Err(error) => {
                                connection.mark_connection_closed(error, true);
                                break;
                            },
                        }
                    },
                    Err(error) => {
                        eprintln!("Connection closed unexpectedly: {}", error);
                        connection.mark_connection_closed(error, false);
                        break;
                    },
                }
            }

            // It is possible that we're waiting for connection.close-ok, so signaling
            // is needed to avoid deadlock
            connection.signal.signal();
        }));

        self.write_handler.set(async_spawn(async move {
            loop {
                // TODO: enqueue more frames at once before sending
                let frame = writer_channel.receive().await;

                match frame {
                    Some(frame) => {
                        writer.enqueue_frame(frame);
                        let result = writer.flush_all().await;

                        // on write error shutdown socket, this should cause read_handler to return error
                        // and mark connection closed
                        if result.is_err() {
                            eprintln!("Connection write error");
                            let _ = writer.fd.shutdown(true, true);
                            break;
                        }
                    },
                    None => {
                        let _ = writer.fd.shutdown(true, true);
                        break;
                    }
                }
            }
        }));
    }
}

#[derive(Debug, Default, Clone)]
enum MessageDeliveryMode {
    #[default] None,
    Return(i16, String, String, String),
    Deliver(String, u64, bool, String, String),
    Get,
}

#[derive(Debug, Default, Clone)]
struct AmqpMessageBuilder {
    mode: MessageDeliveryMode,
    size: usize,
    properties: AmqpBasicProperties,
    content: Vec<u8>,
}

impl AmqpMessageBuilder {
    pub(crate) fn build_if_completed(&mut self) -> Result<Option<(MessageDeliveryMode, AmqpMessage)>, AmqpConnectionError> {
        if !self.is_prepared() {
            return Ok(None);
        }

        let result = if self.is_complete() {
            Ok(Some((std::mem::take(&mut self.mode), AmqpMessage { properties: std::mem::take(&mut self.properties), content: std::mem::take(&mut self.content) })))
        } else {
            eprintln!("Discarding incomplete frame");
            Ok(None)
        };

        self.mode = MessageDeliveryMode::None;
        self.size = 0;

        result
    }

    fn prepare_mode(&mut self, mode: MessageDeliveryMode) -> Result<(), AmqpConnectionError> {
        self.mode = mode;
        Ok(())
    }

    fn prepare_from_header(&mut self, size: u64, properties: AmqpBasicProperties) -> Result<(), AmqpConnectionError> {
        self.properties = properties;
        self.size = size as usize;
        self.content = Vec::with_capacity(size as usize);
        Ok(())
    }

    fn append_data(&mut self, data: &[u8]) -> Result<(), AmqpConnectionError> {
        if !self.is_prepared() {
            return Err(AmqpConnectionError::ProtocolError("Content frame received without header first"));
        }

        self.content.extend_from_slice(data);
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        match self.mode {
            MessageDeliveryMode::None => false,
            _ => true,
        }
    }

    fn is_complete(&self) -> bool {
        self.content.len() == self.size
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use fbs_runtime::{async_run, async_sleep};

    #[test]
    fn bad_connect_test() {
        async_run(async {
            let mut amqp = AmqpConnection::new("asd".to_string());
            let result = amqp.connect("guest", "guest").await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn good_connect_test() {
        async_run(async {
            let mut amqp = AmqpConnection::new("localhost".to_string());
            let result = amqp.connect("guest", "guest").await;
            dbg!(result);

            let mut channel = amqp.channel_open().await.unwrap();
            println!("Got channel opened!");

            let flow = channel.flow(true).await;
            println!("After flow!");
            dbg!(flow);

            let _ = channel.declare_exchange("test-exchange".to_string(), "direct".to_string(), AmqpExchangeFlags::new()).await;
            println!("After declare exchange!");

            let _ = channel.declare_queue("test-queue".to_string(), AmqpQueueFlags::new().durable(true)).await;
            println!("After declare queue!");

            let _ = channel.bind_queue("test-queue".to_string(), "test-exchange".to_string(), "test-key".to_string(), false).await;
            println!("After queue bind!");

            let _ = channel.purge_queue("test-queue".to_string(), false).await;
            println!("After queue purge!");

            let _ = channel.qos(0, 1, false).await;
            println!("After basic qos!");

            let tag = channel.consume("test-queue".to_string(), String::new(), Box::new(|_, _, _, _, _| { }), AmqpConsumeFlags::new()).await.unwrap();
            println!("After basic consume!");

            let _ = channel.cancel(tag, false).await;
            println!("After basic cancel!");

            let _ = channel.unbind_queue("test-queue".to_string(), "test-exchange".to_string(), "test-key".to_string()).await;
            println!("After queue unbind!");

            let _ = channel.delete_queue("test-queue".to_string(), AmqpDeleteQueueFlags::new()).await;
            println!("After queue delete!");

            let _ = channel.delete_exchange("test-exchange".to_string(), AmqpDeleteExchangeFlags::new().if_unused(true)).await;
            println!("After delete exchange!");

            let r2 = channel.close().await;
            dbg!(r2);
            println!("Got channel closed!");

            amqp.close().await;
            println!("Got connection closed!");
        });
    }

    #[test]
    fn publish_test() {
        async_run(async {
            let mut amqp = AmqpConnection::new("localhost".to_string());
            let _ = amqp.connect("guest", "guest").await;

            let mut channel = amqp.channel_open().await.unwrap();
            println!("Got channel opened!");

            let _ = channel.flow(true).await;
            println!("After flow!");

            let _ = channel.declare_exchange("test-exchange".to_string(), "direct".to_string(), AmqpExchangeFlags::new()).await;
            println!("After declare exchange!");

            let _ = channel.declare_queue("test-queue".to_string(), AmqpQueueFlags::new().durable(true)).await;
            println!("After declare queue!");

            // let _ = channel.purge_queue("test-queue".to_string(), false).await;
            // println!("After queue purge!");

            let mut properties = AmqpBasicProperties::default();
            properties.content_type = Some("text/plain".to_string());
            properties.correlation_id = Some("correlation_id test".to_string());
            properties.app_id = Some("app_id test".to_string());
            properties.timestamp = Some(234524);
            properties.cluster_id = Some("cluster_id test".to_string());
            properties.message_id = Some("message id test".to_string());
            properties.priority = Some(2);

            channel.set_on_return(Some(Box::new(|code, reason, exchange, routing_key, message| {
                println!("Got return ({}, {}, {}, {}) - {:?}", code, reason, exchange, routing_key, message);
            })));

            let consume = Box::new(|tag, redelivered, exchange, routing_key, message| {
                println!("Got message ({}, {}, {}, {}) - {:?}", tag, redelivered, exchange, routing_key, message);
            });

            let tag = channel.consume("test-queue".to_string(), String::new(), consume, AmqpConsumeFlags::new()).await.unwrap();
            println!("After basic consume!");

            let _ = channel.publish("".to_string(), "test-queue".to_string(), properties, AmqpPublishFlags::new().mandatory(true), "test-data".as_bytes());

            channel.recover(true).await;
            async_sleep(Duration::new(2, 0)).await;

            let r2 = channel.close().await;
            dbg!(r2);
            println!("Got channel closed!");

            amqp.close().await;
            println!("Got connection closed!");
        });
    }

    #[test]
    fn no_sense_test() {
        async_run(async {
            let mut amqp = AmqpConnection::new("localhost".to_string());
            let _ = amqp.connect("guest", "guest").await;

            let mut channel = amqp.channel_open().await.unwrap();
            println!("Got channel opened!");

            let _ = channel.declare_queue("test-queue-empty".to_string(), AmqpQueueFlags::new().durable(true)).await;
            println!("After declare queue!");

            let envelope = channel.get("test-queue".to_string(), false).await.unwrap();
            println!("After basic consume!");

            match envelope {
                None => (),
                Some((tag, _, _, _, _, _)) => {
                    channel.nack(tag, AmqpNackFlags::new());
                },
            }

            dbg!(envelope);

            async_sleep(Duration::new(2, 0)).await;

            let r2 = channel.close().await;
            dbg!(r2);
            println!("Got channel closed!");

            amqp.close().await;
            println!("Got connection closed!");
        });
    }    

    #[test]
    fn confirm_test() {
        async_run(async {
            let mut amqp = AmqpConnection::new("localhost".to_string());
            let _ = amqp.connect("guest", "guest").await;

            let mut channel = amqp.channel_open().await.unwrap();
            println!("Got channel opened!");

            let on_ack = Box::new(|tag, multiple| {
                println!("Got ack");
            });

            let on_nack = Box::new(|tag, flags| {
                println!("Got nack");
            });

            channel.confirm_select((on_ack, on_nack), false).await;

            let _ = channel.publish("".to_string(), "test-queue".to_string(), AmqpBasicProperties::default(), AmqpPublishFlags::new().mandatory(true), "test-data".as_bytes());

            async_sleep(Duration::new(2, 0)).await;

            let r2 = channel.close().await;
            dbg!(r2);
            println!("Got channel closed!");

            amqp.close().await;
            println!("Got connection closed!");
        });
    }    

}
