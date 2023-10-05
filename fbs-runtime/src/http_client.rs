use std::ffi::CString;
use std::marker::PhantomPinned;
use std::{rc::Rc, cell::RefCell};
use std::pin::Pin;
use std::time::Duration;

use super::async_spawn;
use super::async_utils::{async_channel_create, AsyncChannelRx, AsyncChannelTx};
use super::ops::{async_sleep_with_result, async_sleep_update, async_cancel, async_poll, async_poll_update};

use fbs_executor::TaskHandle;
use fbs_library::poll::PollMask;

use libcurl_sys::*;

const_cstr! {
    HTTP_METHOD_DELETE = "DELETE";
}

#[derive(Debug, Clone, Copy)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    method: HttpMethod,
    url: String,
}

impl HttpRequest {
    pub fn new() -> Self {
        Self { method: HttpMethod::Get, url: String::new() }
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    ptr: Rc<RefCell<Pin<Box<HttpResponseData>>>>,
}

impl HttpResponse {
    fn new() -> Result<Self, ()> {
        let result = Self {
            ptr: Rc::new(RefCell::new(Box::pin(HttpResponseData::new()?))),
        };

        result.ptr.borrow_mut().as_mut().init();
        Ok(result)
    }

    fn setup(&self, request: &HttpRequest) {
        self.ptr.borrow_mut().as_mut().setup(request)
    }

    fn easy_handle(&self) -> *mut CURL {
        self.ptr.borrow().handle
    }
}

#[derive(Debug)]
struct UploadBuffer {
    data: Vec<u8>,
    offset: usize,
}

#[derive(Debug)]
struct HttpResponseData {
    handle: *mut CURL,
    data_to_send: UploadBuffer,
    data_received: Vec<u8>,
    curl_error: [u8; CURL_ERROR_SIZE as usize],
    url_cstring: CString,
}

impl HttpResponseData {
    pub fn new() -> Result<Self, ()> {
        unsafe {
            let handle = curl_easy_init();
            if handle.is_null() {
                return Err(());
            }

            Ok(Self {
                handle,
                data_to_send: UploadBuffer { data: vec![], offset: 0 },
                data_received: vec![],
                curl_error: [0; CURL_ERROR_SIZE as usize],
                url_cstring: CString::default(),
            })
        }
    }

    fn init(mut self: Pin<&mut Self>) {
        unsafe {
            curl_easy_setopt(self.handle, CURLOPT_READFUNCTION, &read_proxy);
            curl_easy_setopt(self.handle, CURLOPT_READDATA, &mut self.data_to_send);
            curl_easy_setopt(self.handle, CURLOPT_WRITEFUNCTION, &write_proxy);
            curl_easy_setopt(self.handle, CURLOPT_WRITEDATA, &mut self.data_received);
            curl_easy_setopt(self.handle, CURLOPT_NOPROGRESS, 1 as libc::c_long);
            curl_easy_setopt(self.handle, CURLOPT_VERBOSE, 0 as libc::c_long);
            curl_easy_setopt(self.handle, CURLOPT_UPLOAD, 0 as libc::c_long);
            curl_easy_setopt(self.handle, CURLOPT_CUSTOMREQUEST, std::ptr::null::<libc::c_char>());
            curl_easy_setopt(self.handle, CURLOPT_ERRORBUFFER, self.curl_error.as_mut_ptr());
        }
    }

    fn setup(mut self: Pin<&mut Self>, request: &HttpRequest) {
        unsafe {
            match request.method {
                HttpMethod::Get => curl_easy_setopt(self.handle, CURLOPT_HTTPGET, 1 as libc::c_long),
                HttpMethod::Post => curl_easy_setopt(self.handle, CURLOPT_HTTPPOST, 1 as libc::c_long),
                HttpMethod::Put => curl_easy_setopt(self.handle, CURLOPT_UPLOAD, 1 as libc::c_long),
                HttpMethod::Delete => curl_easy_setopt(self.handle, CURLOPT_CUSTOMREQUEST, HTTP_METHOD_DELETE.as_ptr()),
            };

            self.url_cstring = CString::new(request.url.clone()).unwrap();
            curl_easy_setopt(self.handle, CURLOPT_URL, self.url_cstring.as_ptr());
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct SocketData
{
    fd: i32,
    armed: PollMask,
    wanted: PollMask,
    change_in_flight: bool,
    poll_op: Option<(u64, usize)>,
}

impl SocketData {
    fn fd(&self) -> i32 {
        self.fd
    }

    fn set_fd(&mut self, fd: i32) {
        self.fd = fd;
    }

    fn need_update(&self) -> bool {
        self.armed == self.wanted
    }

    fn wanted(&self) -> PollMask {
        self.wanted
    }

    fn set_wanted(&mut self, wanted: PollMask) {
        self.wanted = wanted;
    }

    fn set_armed(&mut self, armed: PollMask) {
        self.armed = armed;
    }

    fn change_in_flight(&self) -> bool {
        self.change_in_flight
    }

    fn set_change_in_flight(&mut self, value: bool) {
        self.change_in_flight = value;
    }

    fn take_poll_op(&mut self) -> Option<(u64, usize)> {
        self.poll_op.take()
    }

    fn get_poll_op(&self) -> Option<(u64, usize)> {
        self.poll_op
    }

    fn set_poll_op(&mut self, token: (u64, usize)) {
        self.poll_op = Some(token);
    }

    fn clear_poll_op(&mut self) {
        self.poll_op = None;
    }
}

#[derive(Debug)]
enum IOEvent {
    TimerFired,
    FdReady(i32, bool, bool),
}

#[derive(Debug)]
struct PollerData {
    timer_epoch: u64,
    timer_op: Option<(u64, usize)>,
    io_events_tx: AsyncChannelTx<IOEvent>,
    io_events_rx: AsyncChannelRx<IOEvent>,
}

impl Default for PollerData {
    fn default() -> Self {
        let (rx, tx) = async_channel_create();
        Self { timer_epoch: 0, timer_op: None, io_events_rx: rx, io_events_tx: tx }
    }
}

#[derive(Debug, Clone)]
struct PollerDataPtr {
    ptr: Rc<RefCell<PollerData>>,
}

impl Default for PollerDataPtr {
    fn default() -> Self {
        Self { ptr: Rc::new(RefCell::new(PollerData::default())) }
    }
}

impl PollerDataPtr {
    fn get_new_epoch(&self) -> u64 {
        self.ptr.borrow_mut().timer_epoch += 1;
        self.ptr.borrow().timer_epoch
    }

    fn get_epoch(&self) -> u64 {
        self.ptr.borrow().timer_epoch
    }

    fn dec_epoch(&self) {
        self.ptr.borrow_mut().timer_epoch -= 1;
    }

    fn take_current_op(&self) -> Option<(u64, usize)> {
        self.ptr.borrow_mut().timer_op.take()
    }

    fn get_current_op(&self) -> Option<(u64, usize)> {
        self.ptr.borrow_mut().timer_op
    }

    fn set_current_op(&self, token: (u64, usize)) {
        self.ptr.borrow_mut().timer_op = Some(token);
    }

    fn clear_current_op(&self) {
        self.ptr.borrow_mut().timer_op = None;
    }

    fn push_event(&self, event: IOEvent) {
        self.ptr.borrow_mut().io_events_tx.send(event);
    }
}

struct HttpClientData {
    multi_handle: *mut CURLM,
    responses: Vec<HttpResponse>,
    poller: PollerDataPtr,
    event_processor: TaskHandle<()>,
    _pin: PhantomPinned,
}

impl HttpClientData {
    fn new() -> Result<Self, ()> {
        let curl = unsafe { curl_multi_init() };
        if curl.is_null() {
            return Err(());
        }

        Ok(Self {
            multi_handle: curl,
            responses: Vec::new(),
            poller: PollerDataPtr::default(),
            event_processor: TaskHandle::default(),
            _pin: PhantomPinned,
        })
    }

    fn init(mut self: Pin<&mut Self>) {
        unsafe {
            let this = self.as_mut().get_unchecked_mut() as *mut HttpClientData as *mut libc::c_void;

            curl_multi_setopt(self.multi_handle, CURLMOPT_SOCKETFUNCTION, socket_callback as *mut libc::c_void);
            curl_multi_setopt(self.multi_handle, CURLMOPT_SOCKETDATA, this);
            curl_multi_setopt(self.multi_handle, CURLMOPT_TIMERFUNCTION, timer_callback as *mut libc::c_void);
            curl_multi_setopt(self.multi_handle, CURLMOPT_TIMERDATA, this);

            let poller = self.poller.clone();
            let multi_handle = self.multi_handle;
            self.as_mut().get_unchecked_mut().event_processor = async_spawn(async move {
                loop {
                    let event = poller.ptr.borrow_mut().io_events_rx.receive().await;
                    match event {
                        IOEvent::FdReady(fd, read, write) => {
                            let mask = match (read, write) {
                                (true, false) => CURL_CSELECT_IN,
                                (false, true) => CURL_CSELECT_OUT,
                                (true, true) => CURL_CSELECT_IN | CURL_CSELECT_IN,
                                (_, _) => panic!("IO event false/false"),
                            };

                            let mut running: i32 = 0;
                            let error = curl_multi_socket_action(multi_handle, fd as u32, mask as i32, &mut running);
                            assert_eq!(error, 0);
                        },
                        IOEvent::TimerFired => {
                            let mut running: i32 = 0;
                            let error = curl_multi_socket_action(multi_handle, CURL_SOCKET_TIMEOUT as u32, 0, &mut running);
                            assert_eq!(error, 0);
                        },
                    }

                    loop {
                        let mut msg_in_queue: i32 = 0;
                        let maybe_msg = curl_multi_info_read(multi_handle, &mut msg_in_queue);
                        let msg = match maybe_msg.is_null() {
                            true => break,
                            false => &*maybe_msg,
                        };

                        if msg.msg == CURLMSG_DONE {
                            let easy = msg.easy_handle;
                        }
                    }
                }
            });
        }
    }

    pub fn execute(mut self: Pin<&mut Self>, request: HttpRequest) -> Result<HttpResponse, ()> {
        let response = HttpResponse::new()?;
        response.setup(&request);

        unsafe {
            self.as_mut().get_unchecked_mut().responses.push(response.clone());
        }

        self.as_mut().attach(&response);
        self.as_mut().perform();

        Ok(response)
    }

    fn attach(self: Pin<&mut Self>, request: &HttpResponse) {
        unsafe {
            curl_multi_add_handle(self.multi_handle, request.easy_handle());
        }
    }

    fn detach(self: Pin<&mut Self>, request: &HttpResponse) {
        unsafe {
            curl_multi_remove_handle(self.multi_handle, request.ptr.borrow().handle);
        }
    }

    fn perform(self: Pin<&mut Self>) -> libc::c_int {
        unsafe {
            let mut still_running: libc::c_int = 0;
            curl_multi_perform(self.multi_handle, &mut still_running);
            still_running
        }
    }

}

impl Drop for HttpClientData {
    fn drop(&mut self) {
        unsafe {
            self.event_processor.cancel_by_ref();

            self.responses.iter_mut().for_each(|e| {
                curl_multi_remove_handle(self.multi_handle, e.easy_handle());
            });

            curl_multi_cleanup(self.multi_handle);
        }
    }
}

pub struct HttpClient {
    ptr: Pin<Box<HttpClientData>>,
}

impl HttpClient {
    pub fn new() -> Result<Self, ()>  {
        let mut ptr = Box::pin(HttpClientData::new()?);
        ptr.as_mut().init();

        Ok(Self { ptr })
    }

    pub fn execute(&mut self, request: HttpRequest) -> Result<HttpResponse, ()> {
        self.ptr.as_mut().execute(request)
    }

    fn attach(&mut self, request: &HttpResponse) {
        self.ptr.as_mut().attach(request)
    }

    fn detach(&mut self, request: &HttpResponse) {
        self.ptr.as_mut().detach(request)
    }
}

unsafe extern "C" fn socket_callback(_curl: *mut CURL, sockfd: curl_socket_t, what: libc::c_int, userp: *mut libc::c_void, sockp: *mut libc::c_void) -> libc::c_int {
    println!("socket callback {} {}", sockfd, what);
    let client = &mut *(userp as *mut HttpClientData);

    let socket = match sockp.is_null() {
        true => {
            let mut socket = Box::new(SocketData::default());
            socket.set_fd(sockfd as i32);

            curl_multi_assign(client.multi_handle, sockfd, socket.as_mut() as *mut SocketData as *mut libc::c_void);
            Box::leak(socket)
        },
        false => {
            &mut *(sockp as *mut SocketData)
        },
    };

    let mask = match what as u32 {
        CURL_POLL_REMOVE    => {
            poll_cleanup(Box::from_raw(socket));
            return 0;
        },
        CURL_POLL_IN        => PollMask::default().read(true),
        CURL_POLL_OUT       => PollMask::default().write(true),
        CURL_POLL_INOUT     => PollMask::default().read(true).write(true),
        _                   => PollMask::default(),
    };

    poll_socket(client.poller.clone(), socket, mask);
    0
}

unsafe extern "C" fn timer_callback(_: *mut CURLM, timeout_ms: libc::c_long, sockp: *mut libc::c_void) -> libc::c_int {
    println!("timer callback {}", timeout_ms);
    let client = &mut *(sockp as *mut HttpClientData);

    let seconds = timeout_ms * 1_000_000 / 1_000_000_000;
    let nanoseconds = timeout_ms * 1_000_000 % 1_000_000_000;

    schedule_timeout(client.poller.clone(), seconds, nanoseconds);
    0
}

extern "C" fn read_proxy(ptr: *mut libc::c_void, size: libc::size_t, nmemb: libc::size_t, userdata: *mut libc::c_void) -> libc::size_t {
    unsafe {
        let upload = &mut *(userdata as *mut UploadBuffer);
        let bytes_requested = size * nmemb;
        let bytes_to_copy = std::cmp::min(bytes_requested, upload.data.len() - upload.offset);

        std::ptr::copy_nonoverlapping(upload.data.as_ptr(), ptr as *mut u8, bytes_to_copy);
        upload.offset += bytes_to_copy;

        bytes_to_copy
    }
}

extern "C" fn write_proxy(ptr: *mut libc::c_char, size: libc::size_t, nmemb: libc::size_t, userdata: *mut libc::c_void) -> libc::size_t {
    unsafe {
        let data = std::slice::from_raw_parts(ptr as *const u8, size * nmemb);
        let buffer = &mut *(userdata as *mut Vec<u8>);
        buffer.extend_from_slice(data);
    }

    size * nmemb
}

unsafe fn poll_cleanup(mut socket: Box<SocketData>) {
    if let Some(token) = socket.take_poll_op() {
        async_cancel(token).schedule(move |_|{});
    }
}

unsafe fn poll_socket(poller: PollerDataPtr, socket: &mut SocketData, wanted: PollMask) {
    let socket_ptr = socket as *mut SocketData;
    if !socket.need_update() {
        return;
    }

    // Op is in flight, save desired state for later
    if socket.change_in_flight() {
        socket.set_wanted(wanted);
        return;
    }

    // Poll removal
    if wanted.empty() {
        if let Some(token) = socket.take_poll_op() {
            socket.set_change_in_flight(true);

            async_cancel(token).schedule(move |_| {
                let socket = &mut *socket_ptr;
                socket.set_armed(PollMask::default());
                socket.clear_poll_op();
                socket.set_change_in_flight(false);
            });
        }

        return;
    }

    match socket.get_poll_op() {
        None => {
            // Poll add
            socket.set_armed(wanted);
            let poller_ptr = poller.clone();
            let token = async_poll(&socket.fd(), wanted).schedule(move |result| {
                let socket = &mut *socket_ptr;
                match result {
                    Ok(mask) => poller_ptr.push_event(IOEvent::FdReady(socket.fd(), (mask & libc::POLLIN as i32) != 0, (mask & libc::POLLOUT as i32) != 0)),
                    Err(error) if error.cancelled() => (),
                    Err(error) => panic!("Poll operation for fd {} returned {}", socket.fd(), error),
                };

                socket.set_armed(PollMask::default());
                socket.clear_poll_op();

                poll_socket(poller.clone(), socket, socket.wanted());
            });

            socket.set_poll_op(token);
        },
        Some(token) => {
            // Poll update
            socket.set_change_in_flight(true);
            socket.set_wanted(wanted);

            async_poll_update(token, wanted).schedule(move |result| {
                let socket = &mut *socket_ptr;
                socket.set_change_in_flight(false);
                match result {
                    Ok(_) => {
                        socket.set_armed(wanted);
                        poll_socket(poller.clone(), socket, socket.wanted());
                    },
                    Err(error) if error.errno() == libc::ENOENT => {
                        poll_socket(poller.clone(), socket, socket.wanted());
                    },
                    Err(error) if error.cancelled() => (),
                    Err(error) if error.errno() == libc::EALREADY => (),
                    Err(error) => panic!("Unexpected error for poll_update op: {}", error),
                }
            });
        }
    }
}

fn schedule_timeout(poller: PollerDataPtr, seconds: i64, nanoseconds: i64) {
    let epoch = poller.get_new_epoch();

    // timer removal
    if seconds < 0 {
        if let Some(op_token) = poller.take_current_op() {
            async_cancel(op_token).schedule(|_|{});
            return;
        }
    }

    // no timeout - call now
    if seconds == 0 && nanoseconds == 0 {
        poller.push_event(IOEvent::TimerFired);
        return;
    }

    // real timer setup
    match poller.get_current_op() {
        None => {
            let poller_ptr = poller.clone();
            let token = async_sleep_with_result(Duration::new(seconds as u64, nanoseconds as u32)).schedule(move |result| {
                poller_ptr.clear_current_op();
                if result.is_err() {
                    return;
                }

                if poller_ptr.get_epoch() > epoch {
                    return;
                }

                poller_ptr.push_event(IOEvent::TimerFired);
            });

            poller.set_current_op(token);
        },
        Some(token) => {
            poller.dec_epoch();

            async_sleep_update(token, Duration::new(seconds as u64, nanoseconds as u32)).schedule(move|result| {
                if result.is_ok() {
                    return;
                }

                // We were too late, reschedule timeout
                schedule_timeout(poller.clone(), seconds, nanoseconds);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_client_create() {
        let client = HttpClient::new();
        drop(client);
    }

    #[test]
    fn http_client_request() {
        let mut client = HttpClient::new().unwrap();
        let mut request = HttpRequest::new();
        request.url = String::from("http://www.onet.pl");

        let response = client.execute(request).unwrap();
    }
}
