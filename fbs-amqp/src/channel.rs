use std::rc::Rc;
use std::cmp::min;
use std::cell::{RefCell, Cell};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};

use super::*;
use super::defines::*;
use super::connection::AmqpConnectionInternal;
use super::frame::{AmqpFrame, AmqpFramePayload, AmqpMethod};

use fbs_runtime::async_utils::{AsyncChannelRx, AsyncChannelTx, async_channel_create};

pub struct AmqpChannel {
    pub(super) ptr: Rc<AmqpChannelInternals>,
}

impl AmqpChannel {
    pub(super) fn new(connection: Rc<AmqpConnectionInternal>) -> Self {
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

pub(super) struct AmqpChannelInternals {
    connection: Rc<AmqpConnectionInternal>,
    pub rx: AsyncChannelRx<Result<AmqpFrame, AmqpConnectionError>>,
    pub tx: AsyncChannelTx<Result<AmqpFrame, AmqpConnectionError>>,
    message_rx: AsyncChannelRx<Result<AmqpMessage, AmqpConnectionError>>,
    message_tx: AsyncChannelTx<Result<AmqpMessage, AmqpConnectionError>>,
    pub wait_list: FrameWaiter,
    pub number: Cell<usize>,
    active: Cell<bool>,
    last_error: RefCell<Option<AmqpConnectionError>>,
    on_return: RefCell<Option<Box<dyn Fn(i16, String, String, String, AmqpMessage)>>>,
    message_in_flight: RefCell<AmqpMessageBuilder>,
    consumers: RefCell<HashMap<String, AmqpConsumer>>,
    install_consumer: Cell<Option<AmqpConsumer>>,
    confirm_callbacks: RefCell<Option<(AmqpConfirmAckCallback, AmqpConfirmNackCallback)>>,
}

impl Debug for AmqpChannelInternals {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AmqpConnectionInternal")
        .field("wait_list", &self.wait_list)
        .field("number", &self.number)
        .field("active", &self.active)
        .finish()
    }
}

#[derive(Debug, Default)]
pub(super) struct FrameWaiter {
    pub channel_open_ok: Cell<bool>,
    pub channel_close_ok: Cell<bool>,
    pub channel_flow_ok: Cell<bool>,
    pub exchange_declare_ok: Cell<bool>,
    pub exchange_delete_ok: Cell<bool>,
    pub queue_declare_ok: Cell<bool>,
    pub queue_bind_ok: Cell<bool>,
    pub queue_unbind_ok: Cell<bool>,
    pub queue_purge_ok: Cell<bool>,
    pub queue_delete_ok: Cell<bool>,
    pub basic_qos_ok: Cell<bool>,
    pub basic_consume_ok: Cell<bool>,
    pub basic_cancel_ok: Cell<bool>,
    pub basic_get: Cell<bool>,
    pub basic_recover_ok: Cell<bool>,
    pub confirm_select_ok: Cell<bool>,
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

            let mut data_buffer = self.connection.buffers.get_buffer();
            data_buffer.extend_from_slice(&content[..bytes_in_frame]);

            let frame = AmqpFrame {
                channel: self.number.get() as u16,
                payload: AmqpFramePayload::Content(data_buffer),
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

    pub fn handle_frame(&self, frame: AmqpFrame) -> Result<(), AmqpConnectionError> {
        match frame.payload {
            AmqpFramePayload::Header(_, size, properties) => {
                self.message_in_flight.borrow_mut().prepare_from_header(size, properties)?;
                Ok(())
            },
            AmqpFramePayload::Content(data) => {
                self.message_in_flight.borrow_mut().append_data(&data)?;
                self.connection.buffers.put_buffer(data);

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
    fn build_if_completed(&mut self) -> Result<Option<(MessageDeliveryMode, AmqpMessage)>, AmqpConnectionError> {
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
