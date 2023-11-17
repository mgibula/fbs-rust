use std::cell::{Cell, RefCell};
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

use super::frame::{AmqpProtocolHeader, AmqpFrame, AmqpFrameError, AmqpFramePayload, AmqpMethod};
use super::frame_reader::AmqpFrameReader;
use super::frame_writer::FrameWriter;

use thiserror::Error;

#[derive(Error, Debug)]
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
    #[error("Frame error")]
    FrameError(#[from] AmqpFrameError)
}

pub struct AmqpConnection {
    ptr: Rc<AmqpConnectionInternal>,
    address: String,
}

#[derive(Clone)]
pub struct AmqpChannel {
    ptr: Rc<AmqpChannelInternals>,
}

impl AmqpChannel {
    fn new(connection: Rc<AmqpConnectionInternal>) -> Self {
        Self { ptr: Rc::new(AmqpChannelInternals::new(connection)) }
    }

    pub async fn close(self) -> Result<(), AmqpConnectionError> {
        let frame = AmqpFrame {
            channel: self.ptr.number.get() as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::ChannelClose(0, "".to_string(), 0, 0)),
        };

        self.ptr.connection.writer_queue.send(Some(frame));

        self.ptr.wait_list.channel_close_ok.set(true);
        self.ptr.signal.wait().await;

        Ok(())
    }
}

struct AmqpChannelInternals {
    connection: Rc<AmqpConnectionInternal>,
    signal: AsyncSignal,
    wait_list: FrameWaiter,
    number: Cell<usize>,
}

#[derive(Debug, Default)]
struct FrameWaiter {
    pub channel_open_ok: Cell<bool>,
    pub channel_close_ok: Cell<bool>,
}

impl AmqpChannelInternals {
    fn new(connection: Rc<AmqpConnectionInternal>) -> Self {
        Self { connection, signal: AsyncSignal::new(), wait_list: FrameWaiter::default(), number: Cell::new(0) }
    }

    fn handle_frame(&self, frame: AmqpFrame) {
        match frame.payload {
            AmqpFramePayload::Method(AmqpMethod::ChannelCloseOk()) if self.wait_list.channel_close_ok.get() => {
                self.wait_list.channel_close_ok.set(false);
                self.signal.signal();
            },
            AmqpFramePayload::Method(AmqpMethod::ChannelOpenOk()) if self.wait_list.channel_open_ok.get() => {
                self.wait_list.channel_open_ok.set(false);
                self.signal.signal();
            },
            _ => (),
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
        let channel = AmqpChannel::new(self.ptr.clone());
        let index = self.ptr.set_channel(&channel);
        channel.ptr.number.set(index);

        let frame = AmqpFrame {
            channel: index as u16,
            payload: AmqpFramePayload::Method(AmqpMethod::ChannelOpen()),
        };

        self.ptr.writer_queue.send(Some(frame));
        channel.ptr.wait_list.channel_open_ok.set(true);
        channel.ptr.signal.wait().await;

        Ok(channel)
    }

    pub async fn close(self) {
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
        self.ptr.mark_connection_closed();
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

        Ok(())
    }
}

pub struct AmqpConnectionInternal {
    fd: Rc<Socket>,
    channels: RefCell<IndexedList<Rc<AmqpChannelInternals>>>,
    writer_queue: AsyncChannelTx<Option<AmqpFrame>>,
    read_handler: Cell<TaskHandle<()>>,
    write_handler: Cell<TaskHandle<()>>,
    connection_closed: Cell<bool>,
    signal: AsyncSignal,
    max_channels: Cell<u16>,
    max_frame_size: Cell<u32>,
    heartbeat: Cell<u16>,
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
            connection_closed: Cell::new(false),
            signal: AsyncSignal::new(),
            max_channels: Cell::new(100),
            max_frame_size: Cell::new(4096),
            heartbeat: Cell::new(0),
        }
    }

    fn set_channel(&self, channel: &AmqpChannel) -> usize {
        self.channels.borrow_mut().insert(channel.ptr.clone()) + 1
    }

    fn clear_channel(&self, index: usize) {
        self.channels.borrow_mut().remove(index + 1);
    }

    fn handle_channel_frame(&self, frame: AmqpFrame) {
        let index = (frame.channel - 1) as usize;
        let mut channels = self.channels.borrow_mut();

        let channel = channels.get_mut(index);
        match channel {
            None => (),
            Some(channel) => {
                channel.handle_frame(frame);
            },
        }
    }

    fn handle_connection_frame(&self, frame: AmqpFrame) {
        match frame.payload {
            AmqpFramePayload::Method(AmqpMethod::ConnectionCloseOk()) => {
                self.mark_connection_closed();
                self.signal.signal();
            },
            _ => (),
        }
    }

    fn mark_connection_closed(&self) {
        self.connection_closed.set(true);
        self.writer_queue.send(None);
        let _ = self.fd.shutdown(true, true);
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

        let frame = reader.read_frame().await?;

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
                reader.change_capacity((*frame) as usize);
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

        let frame = reader.read_frame().await?;

        self.start_io_handler(writer, self.writer_queue.rx(), reader, self_ptr);
        Ok(())
    }

    fn start_io_handler(&self, mut writer: AmqpConnectionWriter, mut writer_channel: AsyncChannelRx<Option<AmqpFrame>>, mut reader: AmqpConnectionReader, connection: Rc<AmqpConnectionInternal>) {

        self.read_handler.set(async_spawn(async move {
            while !connection.connection_closed.get() {
                let frame = reader.read_frame().await;
                match frame {
                    Ok(frame) => {
                        dbg!(&frame);
                        if frame.channel > 0 {
                            connection.handle_channel_frame(frame);
                        } else {
                            connection.handle_connection_frame(frame);
                        }
                    },
                    Err(_) => {
                        eprintln!("Connection closed unexpectedly");
                        connection.mark_connection_closed();

                        // It is possible that we're waiting for connection.close-ok, so signaling
                        // is needed to avoid deadlock
                        connection.signal.signal();
                        break;
                    },
                }
            }
        }));

        self.write_handler.set(async_spawn(async move {
            loop {
                // TODO: enqueue more frames at once before sending
                let frame = writer_channel.receive().await;
                dbg!(&frame);

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
                    None => break,
                }
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fbs_runtime::async_run;

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
            let _ = amqp.connect("guest", "guest").await;

            let channel = amqp.channel_open().await.unwrap();
            println!("Got channel opened!");

            channel.close().await;
            println!("Got channel closed!");

            amqp.close().await;
            println!("Got connection closed!");
        });
    }
}
