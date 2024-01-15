use std::cell::{Cell, RefCell};
use std::cmp::min;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::fmt::{Debug, Formatter};
use std::time::Duration;

use fbs_library::socket::{Socket, SocketDomain, SocketType, SocketFlags};
use fbs_library::indexed_list::IndexedList;
use fbs_runtime::async_utils::{AsyncSignal, AsyncChannelRx, AsyncChannelTx, async_channel_create};
use fbs_runtime::{async_connect, async_write, async_read_into, async_spawn, async_sleep};
use fbs_resolver::resolve_address;
use fbs_executor::TaskHandle;

use super::{AmqpConnectionError, AmqpChannel};
use super::channel::AmqpChannelInternals;
use super::frame::{AmqpProtocolHeader, AmqpFrame, AmqpFramePayload, AmqpMethod};
use super::frame_reader::AmqpFrameReader;
use super::frame_writer::FrameWriter;

const FRAME_EXTRA_SIZE: u32 = 8;  // size of frame header and footer

#[derive(Debug, Default, Clone)]
pub struct AmqpConnectionParams {
    pub address: String,
    pub username: String,
    pub password: String,
    pub vhost: String,
    pub heartbeat: u16,
}

#[derive(Debug)]
pub struct AmqpConnection {
    ptr: Rc<AmqpConnectionInternal>,
}

impl AmqpConnection {
    pub async fn connect(params: &AmqpConnectionParams) -> Result<AmqpConnection, AmqpConnectionError> {
        let result: AmqpConnection = AmqpConnection { ptr: Rc::new(AmqpConnectionInternal::new()) };
        result.ptr.connect(params, result.ptr.clone()).await?;

        Ok(result)
    }

    pub fn is_alive(&self) -> bool {
        self.ptr.is_connection_valid().is_ok()
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

    pub fn get_buffer_stats(&self) -> (u64, u64, u64) {
        self.ptr.buffers.get_stats()
    }

    pub fn set_buffers_capacity(&mut self, capacity: usize) {
        self.ptr.buffers.change_capacity(capacity)
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
    frame_buffer: Vec<u8>,
    pub buffers: Rc<BufferManager>,
}

impl AmqpConnectionReader {
    fn new(fd: Rc<Socket>, buffers: Rc<BufferManager>) -> Self {
        Self { fd, read_buffer: Vec::with_capacity(4096), read_offset: 0, frame_buffer: Vec::with_capacity(4096), buffers }
    }

    fn change_frame_size(&mut self, size: usize) {
        assert!(self.read_buffer.capacity() <= size);
        self.read_buffer.reserve(size - self.read_buffer.capacity());
        self.frame_buffer.reserve(size - self.frame_buffer.capacity());
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

        let mut frame_buffer = std::mem::take(&mut self.frame_buffer);
        reserve_buffer_size(&mut frame_buffer, payload_size);

        self.read_bytes(&mut frame_buffer).await?;

        let frame_end = self.read_u8().await?;
        if frame_end != b'\xCE' {
            return Err(AmqpConnectionError::FrameEndInvalid);
        }

        let mut reader = AmqpFrameReader::new(&frame_buffer);
        let result = reader.read_frame(&self.buffers, frame_type, channel);
        self.frame_buffer = frame_buffer;

        match result {
            Ok(frame) => Ok(frame),
            Err(error) => Err(AmqpConnectionError::FrameError(error))
        }
    }
}

fn reserve_buffer_size(buffer: &mut Vec<u8>, size: usize) {
    if buffer.capacity() < size {
        buffer.reserve(size - buffer.capacity());
    }

    buffer.resize(size, 0);
}

pub(super) struct BufferManager {
    size: Cell<usize>,
    max_capacity: Cell<usize>,
    buffers: RefCell<VecDeque<Vec<u8>>>,
    allocations: Cell<u64>,
    deallocations: Cell<u64>,
    hits: Cell<u64>,
}

impl Debug for BufferManager {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AmqpConnectionInternal")
        .field("size", &self.size)
        .field("max_capacity", &self.max_capacity)
        .field("buffers", &self.buffers.borrow().len())
        .finish()
    }
}

impl BufferManager {
    fn new(size: usize, max_capacity: usize) -> Self {
        BufferManager {
            size: Cell::new(size),
            max_capacity: Cell::new(max_capacity),
            buffers: RefCell::new(VecDeque::new()),
            allocations: Cell::new(0),
            deallocations: Cell::new(0),
            hits: Cell::new(0),
        }
    }

    fn get_stats(&self) -> (u64, u64, u64) {
        (self.allocations.get(), self.deallocations.get(), self.hits.get())
    }

    fn change_capacity(&self, capacity: usize) {
        if self.max_capacity.get() > capacity && self.buffers.borrow().len() > capacity {
            self.buffers.borrow_mut().resize(capacity, Vec::new())
        }

        self.max_capacity.set(capacity);
    }

    fn change_frame_size(&self, size: usize) {
        if self.size.get() == size {
            return;
        }

        self.size.set(size);
        self.buffers.borrow_mut().iter_mut().for_each(|buffer| {
            if buffer.capacity() < size {
                buffer.reserve(size - buffer.capacity());
            }
        });
    }

    pub(super) fn get_buffer(&self) -> Vec<u8> {
        match self.buffers.borrow_mut().pop_back() {
            Some(buffer) => {
                self.hits.set(self.hits.get() + 1);
                buffer
            },
            None => {
                self.allocations.set(self.allocations.get() + 1);
                Vec::with_capacity(self.size.get())
            }
        }
    }

    pub(super) fn put_buffer(&self, mut buffer: Vec<u8>) {
        if self.buffers.borrow().len() >= self.max_capacity.get() {
            self.deallocations.set(self.deallocations.get() + 1);
            return;
        }

        buffer.resize(0, 0);
        self.buffers.borrow_mut().push_back(buffer)
    }
}

struct AmqpConnectionWriter {
    fd: Rc<Socket>,
    queue: VecDeque<AmqpFrame>,
    buffers: Rc<BufferManager>,
}

impl AmqpConnectionWriter {
    fn new(fd: Rc<Socket>, buffers: Rc<BufferManager>) -> Self {
        Self { fd, queue: VecDeque::new(), buffers }
    }

    fn change_frame_size(&mut self, size: usize) {
        self.buffers.change_frame_size(size)
    }

    fn enqueue_frame(&mut self, frame: AmqpFrame) {
        self.queue.push_back(frame);
    }

    async fn flush_all(&mut self) -> Result<(), AmqpConnectionError> {
        loop {
            let frame = self.queue.pop_front();
            match frame {
                None => return Ok(()),
                Some(frame) => self.write_frame(frame).await?,
            }
        }
    }

    async fn write_frame(&mut self, frame: AmqpFrame) -> Result<(), AmqpConnectionError> {
        let data = FrameWriter::write_frame(frame, self.buffers.as_ref());
        let result = async_write(&self.fd, data, None).await;

        match result {
            Ok(buffer) => self.buffers.put_buffer(buffer),
            Err((error, _)) => return Err(AmqpConnectionError::WriteError(error)),
        }

        Ok(())
    }
}

pub(super) struct AmqpConnectionInternal {
    pub writer_queue: AsyncChannelTx<Option<AmqpFrame>>,
    pub max_frame_size: Cell<u32>,
    fd: Rc<Socket>,
    channels: RefCell<IndexedList<Rc<AmqpChannelInternals>>>,
    read_handler: Cell<TaskHandle<()>>,
    write_handler: Cell<TaskHandle<()>>,
    heartbeat_handler: Cell<TaskHandle<()>>,
    signal: AsyncSignal,
    max_channels: Cell<u16>,
    heartbeat: Cell<u16>,
    last_error: RefCell<Option<AmqpConnectionError>>,
    pub buffers: Rc<BufferManager>,
}

impl Debug for AmqpConnectionInternal {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AmqpConnectionInternal")
        .field("max_frame_size", &self.max_frame_size.get())
        .field("max_channels", &self.max_channels.get())
        .field("heartbeat", &self.heartbeat.get())
        .field("last_error", &self.last_error.borrow())
        .field("fd", &self.fd)
        .field("channels", &self.channels.borrow())
        .field("writer_queue", &self.writer_queue)
        .finish()
    }
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
            heartbeat_handler: Cell::new(TaskHandle::default()),
            signal: AsyncSignal::new(),
            max_channels: Cell::new(100),
            max_frame_size: Cell::new(4096),
            heartbeat: Cell::new(0),
            last_error: RefCell::new(None),
            buffers: Rc::new(BufferManager::new(4096, 10)),
        }
    }

    pub fn is_connection_valid(&self) -> Result<(), AmqpConnectionError> {
        let last_error = self.last_error.borrow();
        match *last_error {
            None => Ok(()),
            Some(ref error) => Err(error.clone()),
        }
    }

    fn set_channel(&self, channel: &AmqpChannel) -> usize {
        self.channels.borrow_mut().insert(channel.ptr.clone()) + 1
    }

    pub fn clear_channel(&self, index: usize) {
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
                Ok(())
            },
            AmqpFramePayload::Method(AmqpMethod::ConnectionCloseOk()) => {
                self.mark_connection_closed(AmqpConnectionError::ConnectionClosed, false);
                self.signal.signal();
                Ok(())
            },
            AmqpFramePayload::Heartbeat() => Ok(()),
            _ => Err(AmqpConnectionError::ProtocolError("Unexpected connection frame")),
        }
    }

    fn mark_connection_closed(&self, error: AmqpConnectionError, send_close_frame: bool) {
        self.heartbeat_handler.take().cancel();
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

    async fn connect(&self, params: &AmqpConnectionParams, self_ptr: Rc<AmqpConnectionInternal>) -> Result<(), AmqpConnectionError> {
        let address = resolve_address(&params.address, Some(5672)).await?;
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

        let mut reader = AmqpConnectionReader::new(self.fd.clone(), self.buffers.clone());
        let mut writer = AmqpConnectionWriter::new(self.fd.clone(), self.buffers.clone());

        let _ = reader.read_frame().await?;

        let mut sasl = String::new();
        sasl.push('\x00');
        sasl.push_str(&params.username);
        sasl.push('\x00');
        sasl.push_str(&params.password);

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
                    reader.change_frame_size((*frame) as usize);
                    writer.change_frame_size((*frame) as usize);

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

        if params.heartbeat > 0 {
            self.heartbeat.set(params.heartbeat);
        }

        let response = AmqpFrame {
            channel: 0,
            payload: AmqpFramePayload::Method(AmqpMethod::ConnectionTuneOk(self.max_channels.get(), self.max_frame_size.get(), self.heartbeat.get())),
        };

        writer.enqueue_frame(response);
        writer.flush_all().await?;

        let response = AmqpFrame {
            channel: 0,
            payload: AmqpFramePayload::Method(AmqpMethod::ConnectionOpen(params.vhost.clone())),
        };

        writer.enqueue_frame(response);
        writer.flush_all().await?;

        let _ = reader.read_frame().await?;

        self.start_io_handler(writer, self.writer_queue.rx(), reader, self_ptr);
        Ok(())
    }

    fn start_io_handler(&self, mut writer: AmqpConnectionWriter, writer_channel: AsyncChannelRx<Option<AmqpFrame>>, mut reader: AmqpConnectionReader, connection: Rc<AmqpConnectionInternal>) {

        let heartbeat = self.heartbeat.get();
        let heartbeat_writer = writer_channel.tx();

        self.heartbeat_handler.set(async_spawn(async move {
            let interval = Duration::new(heartbeat as u64, 0);

            loop {
                let frame = AmqpFrame {
                    channel: 0,
                    payload: AmqpFramePayload::Heartbeat(),
                };

                heartbeat_writer.send(Some(frame));
                async_sleep(interval).await;
            }
        }));

        self.read_handler.set(async_spawn(async move {
            while connection.last_error.borrow().is_none() {
                let frame = reader.read_frame().await;
                match frame {
                    Ok(frame) => {
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
