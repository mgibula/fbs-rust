use std::ffi::CString;
use std::ffi::CStr;
use std::marker::PhantomPinned;
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use std::pin::Pin;
use std::time::Duration;
use std::collections::HashMap;

use super::async_spawn;
use super::async_utils::{async_channel_create, AsyncChannelRx, AsyncChannelTx, AsyncSignal};
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
    pub method: HttpMethod,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct HttpResponseData {
    http_code: i32,
    headers: HashMap<String, String>,
    response_body: Vec<u8>,
}

impl HttpRequest {
    pub fn new() -> Self {
        Self { method: HttpMethod::Get, url: String::new() }
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    ptr: Rc<RefCell<Pin<Box<HttpResponseInner>>>>,
}

impl HttpResponse {
    fn new() -> Result<Self, ()> {
        let result = Self {
            ptr: Rc::new(RefCell::new(Box::pin(HttpResponseInner::new()?))),
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

    fn complete_request(&self) {
        self.ptr.borrow_mut().as_mut().set_completed();
    }

    fn fail_request(&self) {
        self.ptr.borrow_mut().as_mut().set_completed();
    }

    pub async fn wait_for_completion(self) -> HttpResponseData {
        // clone is to avoid holding borrow across suspension point
        let waiter = self.ptr.borrow().as_ref().get_completion_waiter();
        waiter.await;

        let buffer = std::mem::take(&mut self.ptr.borrow_mut().as_mut().take_data_received());

        let mut result = HttpResponseData { http_code: 0, response_body: buffer, headers: HashMap::new() };
        self.ptr.borrow_mut().as_mut().fill_response_data(&mut result);

        result
    }
}

#[derive(Debug)]
struct UploadBuffer {
    data: Vec<u8>,
    offset: usize,
}

#[derive(Debug)]
struct HttpResponseInner {
    handle: *mut CURL,
    data_to_send: UploadBuffer,
    data_received: Vec<u8>,
    curl_error: [u8; CURL_ERROR_SIZE as usize],
    url_cstring: CString,
    completion: AsyncSignal,
    _pin: PhantomPinned,
}

impl Drop for HttpResponseInner {
    fn drop(&mut self) {
        unsafe {
            curl_easy_cleanup(self.handle);
        }
    }
}

impl HttpResponseInner {
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
                completion: AsyncSignal::new(),
                _pin: PhantomPinned,
            })
        }
    }

    fn init(mut self: Pin<&mut Self>) {
        unsafe {
            curl_easy_setopt(self.handle, CURLOPT_READFUNCTION, read_proxy as *mut libc::c_void);
            curl_easy_setopt(self.handle, CURLOPT_READDATA, &mut self.as_mut().get_unchecked_mut().data_to_send);
            curl_easy_setopt(self.handle, CURLOPT_WRITEFUNCTION, write_proxy as *mut libc::c_void);
            curl_easy_setopt(self.handle, CURLOPT_WRITEDATA, &mut self.as_mut().get_unchecked_mut().data_received);
            curl_easy_setopt(self.handle, CURLOPT_NOPROGRESS, 1 as libc::c_long);
            curl_easy_setopt(self.handle, CURLOPT_VERBOSE, 0 as libc::c_long);
            curl_easy_setopt(self.handle, CURLOPT_UPLOAD, 0 as libc::c_long);
            curl_easy_setopt(self.handle, CURLOPT_CUSTOMREQUEST, std::ptr::null::<libc::c_char>());
            curl_easy_setopt(self.handle, CURLOPT_ERRORBUFFER, self.as_mut().get_unchecked_mut().curl_error.as_mut_ptr());
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

            self.as_mut().get_unchecked_mut().url_cstring = CString::new(request.url.clone()).unwrap();
            curl_easy_setopt(self.handle, CURLOPT_URL, self.url_cstring.as_ptr());
        }
    }

    fn set_completed(self: Pin<&mut Self>) {
        self.completion.signal();
    }

    fn get_completion_waiter(self: Pin<&Self>) -> AsyncSignal {
        self.completion.clone()
    }

    fn take_data_received(mut self: Pin<&mut Self>) -> Vec<u8> {
        unsafe {
            std::mem::take(&mut self.as_mut().get_unchecked_mut().data_received)
        }
    }

    fn fill_response_data(mut self: Pin<&mut Self>, data: &mut HttpResponseData) {
        unsafe {
            let mut code: libc::c_long = 0;
            curl_easy_getinfo(self.handle, CURLINFO_RESPONSE_CODE, &mut code);

            let mut prev_header = std::ptr::null_mut::<curl_header>();
            loop {
                let header = curl_easy_nextheader(self.handle, CURLH_HEADER, -1, prev_header);
                if header.is_null() {
                    break;
                }

                let key = CStr::from_ptr((*header).name).to_str();
                let value = CStr::from_ptr((*header).value).to_str();

                prev_header = header;
                match (key, value) {
                    (Ok(key), Ok(value)) => {
                        data.headers.insert(key.to_owned(), value.to_owned());
                    },
                    (_, _) => {
                        eprintln!("Invalid characters in header name or value, skipping");
                        continue;
                    },
                }
            }

            data.http_code = code as i32;
        }
    }
}

#[derive(Debug, Default, Clone)]
struct SocketData
{
    fd: Cell<i32>,
    armed: Cell<PollMask>,
    wanted: Cell<PollMask>,
    change_in_flight: Cell<bool>,
    poll_op: Cell<Option<(u64, usize)>>,
}

impl SocketData {
    fn fd(&self) -> i32 {
        self.fd.get()
    }

    fn set_fd(&self, fd: i32) {
        self.fd.set(fd);
    }

    fn need_update(&self, wanted: PollMask) -> bool {
        self.armed.get() != wanted
    }

    fn wanted(&self) -> PollMask {
        self.wanted.get()
    }

    fn set_wanted(&self, wanted: PollMask) {
        self.wanted.set(wanted);
    }

    fn set_armed(&self, armed: PollMask) {
        self.armed.set(armed);
    }

    fn change_in_flight(&self) -> bool {
        self.change_in_flight.get()
    }

    fn set_change_in_flight(&self, value: bool) {
        self.change_in_flight.set(value);
    }

    fn take_poll_op(&self) -> Option<(u64, usize)> {
        self.poll_op.take()
    }

    fn get_poll_op(&self) -> Option<(u64, usize)> {
        self.poll_op.get()
    }

    fn set_poll_op(&self, token: (u64, usize)) {
        self.poll_op.set(Some(token));
    }

    fn clear_poll_op(&self) {
        self.poll_op.set(None);
    }
}

#[derive(Debug)]
enum IOEvent {
    TimerFired,
    FdReady(i32, bool, bool),
}

#[derive(Debug)]
struct HttpClientData {
    multi_handle: *mut CURLM,   // owned by HttpPinnedData
    timer_epoch: u64,
    timer_op: Option<(u64, usize)>,
    io_events_tx: AsyncChannelTx<IOEvent>,
    io_events_rx: AsyncChannelRx<IOEvent>,
    responses: Vec<HttpResponse>,
}

impl HttpClientData {
    fn new(multi_handle: *mut CURLM) -> Self {
        let (rx, tx) = async_channel_create();
        Self { multi_handle, timer_epoch: 0, timer_op: None, io_events_rx: rx, io_events_tx: tx, responses: vec![] }
    }
}

#[derive(Debug, Clone)]
struct HttpClientDataPtr {
    ptr: Rc<RefCell<HttpClientData>>,
}

impl HttpClientDataPtr {
    fn new(multi_handle: *mut CURLM) -> Self {
        Self { ptr: Rc::new(RefCell::new(HttpClientData::new(multi_handle))) }
    }
}

impl HttpClientDataPtr {
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

    fn add_response(&self, response: HttpResponse) {
        self.ptr.borrow_mut().responses.push(response);
    }

    fn take_all_responses(&self) -> Vec<HttpResponse> {
        std::mem::take(&mut self.ptr.borrow_mut().responses)
    }

    async fn wait_for_event(&self) -> IOEvent {
        // clone is to avoid holding borrow across suspension point
        let mut rx = self.ptr.borrow_mut().io_events_rx.clone();
        rx.receive().await
    }

    fn multi_handle(&self) -> *mut CURLM {
        self.ptr.borrow().multi_handle
    }

    unsafe fn complete_requests(&self) {
        loop {
            let mut msg_in_queue: i32 = 0;
            let info = curl_multi_info_read(self.multi_handle(), &mut msg_in_queue);
            let info = match info.is_null() {
                true => break,
                false => &*info,
            };

            if info.msg == CURLMSG_DONE {
                let easy = info.easy_handle;
                let mut inner = self.ptr.borrow_mut();
                let found = inner.responses.iter_mut().position(|r| r.easy_handle() == easy);

                match (found, info.data.result as u32) {
                    (None, _) => {
                        curl_multi_remove_handle(inner.multi_handle, easy);
                    },
                    (Some(idx), CURLE_OK) => {
                        let response = inner.responses.remove(idx);
                        response.complete_request();

                        curl_multi_remove_handle(inner.multi_handle, easy);
                    },
                    (Some(idx), _) => {
                        let response = inner.responses.remove(idx);
                        response.fail_request();

                        curl_multi_remove_handle(inner.multi_handle, easy);
                    },
                }
            }
        }
    }

    unsafe fn fail_all_requests(&self) {
        self.take_all_responses().into_iter().for_each(|r| {
            r.fail_request();
            curl_multi_remove_handle(self.multi_handle(), r.easy_handle());
        });
    }
}

struct HttpPinnedData {
    multi_handle: *mut CURLM,
    poller: HttpClientDataPtr,
    event_processor: TaskHandle<()>,
    _pin: PhantomPinned,
}

impl HttpPinnedData {
    fn new() -> Result<Self, ()> {
        let curl = unsafe { curl_multi_init() };
        if curl.is_null() {
            return Err(());
        }

        Ok(Self {
            multi_handle: curl,
            poller: HttpClientDataPtr::new(curl),
            event_processor: TaskHandle::default(),
            _pin: PhantomPinned,
        })
    }

    fn init(mut self: Pin<&mut Self>) {
        unsafe {
            let this = self.as_mut().get_unchecked_mut() as *mut HttpPinnedData as *mut libc::c_void;

            curl_multi_setopt(self.multi_handle, CURLMOPT_SOCKETFUNCTION, socket_callback as *mut libc::c_void);
            curl_multi_setopt(self.multi_handle, CURLMOPT_SOCKETDATA, this);
            curl_multi_setopt(self.multi_handle, CURLMOPT_TIMERFUNCTION, timer_callback as *mut libc::c_void);
            curl_multi_setopt(self.multi_handle, CURLMOPT_TIMERDATA, this);

            let poller = self.poller.clone();
            let multi_handle = self.multi_handle;
            self.as_mut().get_unchecked_mut().event_processor = async_spawn(async move {
                loop {
                    let event = poller.wait_for_event().await;
                    match event {
                        IOEvent::FdReady(fd, read, write) => {
                            // println!("IOEvent::FdReady event");
                            let mask = match (read, write) {
                                (true, false)   => CURL_CSELECT_IN,
                                (false, true)   => CURL_CSELECT_OUT,
                                (true, true)    => CURL_CSELECT_IN | CURL_CSELECT_IN,
                                (_, _) => panic!("IO event false/false"),
                            };

                            let mut running: i32 = 0;
                            let error = curl_multi_socket_action(multi_handle, fd, mask as i32, &mut running);
                            match error as u32 {
                                CURLE_OK    => poller.complete_requests(),
                                _           => poller.fail_all_requests(),
                            }
                        },
                        IOEvent::TimerFired => {
                            // println!("IOEvent::TimerFired event");
                            let mut running: i32 = 0;
                            let error = curl_multi_socket_action(multi_handle, CURL_SOCKET_TIMEOUT, 0, &mut running);
                            match error as u32 {
                                CURLE_OK    => poller.complete_requests(),
                                _           => poller.fail_all_requests(),
                            }
                        },
                    }


                }
            });
        }
    }

    pub fn execute(mut self: Pin<&mut Self>, request: HttpRequest) -> Result<HttpResponse, ()> {
        let response = HttpResponse::new()?;
        response.setup(&request);

        self.poller.add_response(response.clone());
        self.as_mut().attach(&response);
        self.as_mut().perform();

        Ok(response)
    }

    fn attach(self: Pin<&mut Self>, response: &HttpResponse) {
        unsafe {
            curl_multi_add_handle(self.multi_handle, response.easy_handle());
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

impl Drop for HttpPinnedData {
    fn drop(&mut self) {
        unsafe {
            self.event_processor.cancel_by_ref();

            self.poller.take_all_responses().iter_mut().for_each(|e| {
                curl_multi_remove_handle(self.multi_handle, e.easy_handle());
            });

            curl_multi_cleanup(self.multi_handle);
        }
    }
}

pub struct HttpClient {
    ptr: Pin<Box<HttpPinnedData>>,
}

impl HttpClient {
    pub fn new() -> Result<Self, ()>  {
        let mut ptr = Box::pin(HttpPinnedData::new()?);
        ptr.as_mut().init();

        Ok(Self { ptr })
    }

    pub fn execute(&mut self, request: HttpRequest) -> Result<HttpResponse, ()> {
        self.ptr.as_mut().execute(request)
    }

    fn attach(&mut self, request: &HttpResponse) {
        self.ptr.as_mut().attach(request)
    }
}

unsafe extern "C" fn socket_callback(_curl: *mut CURL, sockfd: curl_socket_t, what: libc::c_int, userp: *mut libc::c_void, sockp: *mut libc::c_void) -> libc::c_int {
    // println!("socket callback {} {}", sockfd, what);
    let client = &mut *(userp as *mut HttpPinnedData);

    let socket = match sockp.is_null() {
        true => {
            let socket = Rc::new(SocketData::default());
            socket.set_fd(sockfd as i32);

            // socket refcount is increased here, this is paired with CURL_POLL_REMOVE handler below
            curl_multi_assign(client.multi_handle, sockfd, Rc::into_raw(socket.clone()) as *mut SocketData as *mut libc::c_void);
            socket
        },
        false => {
            // need to increase refcount to keep it alive
            Rc::increment_strong_count(sockp as *const SocketData);
            Rc::from_raw(sockp as *const SocketData)
        },
    };

    let mask = match what as u32 {
        CURL_POLL_REMOVE    => {
            // failsafe check
            if !sockp.is_null() {
                // see comment above
                Rc::decrement_strong_count(sockp as *const SocketData);
            } else {
                eprintln!("Got CURL_POLL_REMOVE call without socket data associated with it");
            }

            // documentation doesn't specify if socket specific data are cleared with CURL_POLL_REMOVE, so clear it manually
            curl_multi_assign(client.multi_handle, sockfd, std::ptr::null_mut::<libc::c_void>());
            poll_cleanup(socket);
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
    let client = &mut *(sockp as *mut HttpPinnedData);

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

unsafe fn poll_cleanup(socket: Rc<SocketData>) {
    if let Some(token) = socket.take_poll_op() {
        async_cancel(token).schedule(move |_|{});
    }
}

unsafe fn poll_socket(poller: HttpClientDataPtr, socket: Rc<SocketData>, wanted: PollMask) {
    if !socket.need_update(wanted) {
        return;
    }

    // Op is in flight, save desired state for later
    if socket.change_in_flight() {
        // println!("poll socket - change in flight");
        socket.set_wanted(wanted);
        return;
    }

    // Poll removal
    if wanted.empty() {
        if let Some(token) = socket.take_poll_op() {
            // println!("poll socket - poll removal - op in flight, canceling");
            socket.set_change_in_flight(true);

            async_cancel(token).schedule(move |_| {
                socket.set_armed(PollMask::default());
                socket.clear_poll_op();
                socket.set_change_in_flight(false);
            });
        } else {
            // println!("poll socket - poll removal, no op in flight");
        }

        return;
    }

    match socket.get_poll_op() {
        None => {
            // Poll add
            // println!("poll socket - poll add");
            socket.set_armed(wanted);
            let poller_ptr = poller.clone();

            let socket_data = socket.clone();
            let token = async_poll(&socket.fd(), wanted).schedule(move |result| {
                match result {
                    Ok(mask) => poller_ptr.push_event(IOEvent::FdReady(socket_data.fd(), (mask & libc::POLLIN as i32) != 0, (mask & libc::POLLOUT as i32) != 0)),
                    Err(error) if error.cancelled() => (),
                    Err(error) => panic!("Poll operation for fd {} returned {}", socket_data.fd(), error),
                };

                socket_data.set_armed(PollMask::default());
                socket_data.clear_poll_op();

                let wanted = socket_data.wanted();
                poll_socket(poller.clone(), socket_data, wanted);
            });

            socket.set_poll_op(token);
        },
        Some(token) => {
            // Poll update
            // println!("poll socket - poll update");
            socket.set_change_in_flight(true);
            socket.set_wanted(wanted);

            let socket_data = socket.clone();
            async_poll_update(token, wanted).schedule(move |result| {
                socket_data.set_change_in_flight(false);
                match result {
                    Ok(_) => {
                        socket_data.set_armed(wanted);
                        let wanted = socket_data.wanted();
                        poll_socket(poller.clone(), socket_data, wanted);
                    },
                    Err(error) if error.errno() == libc::ENOENT => {
                        let wanted = socket_data.wanted();
                        poll_socket(poller.clone(), socket_data, wanted);
                    },
                    Err(error) if error.cancelled() => (),
                    Err(error) if error.errno() == libc::EALREADY => (),
                    Err(error) => panic!("Unexpected error for poll_update op: {}", error),
                }
            });
        }
    }
}

fn schedule_timeout(poller: HttpClientDataPtr, seconds: i64, nanoseconds: i64) {
    let epoch = poller.get_new_epoch();

    // timer removal
    if seconds < 0 || nanoseconds < 0 {
        // println!("schedule_timeout - clear timer");
        if let Some(op_token) = poller.take_current_op() {
            async_cancel(op_token).schedule(|_|{});
        }

        return;
    }

    // no timeout - call now
    if seconds == 0 && nanoseconds == 0 {
        // println!("schedule_timeout - instant timer");
        poller.push_event(IOEvent::TimerFired);
        return;
    }

    // real timer setup
    match poller.get_current_op() {
        None => {
            // println!("schedule_timeout - new op {} {}", seconds, nanoseconds);
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
            // println!("schedule_timeout - update op {} {}", seconds, nanoseconds);
            async_sleep_update(token, Duration::new(seconds as u64, nanoseconds as u32)).schedule(move|result| {
                // println!("schedule_timeout - update result {:?}", result);
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
    use crate::async_run;

    use super::*;

    #[test]
    fn http_client_create() {
        let client = HttpClient::new();
        drop(client);
    }

    #[test]
    fn http_client_request() {
        async_run(async move {
            let mut client = HttpClient::new().unwrap();
            let mut request = HttpRequest::new();
            request.url = String::from("http://www.onet.pl");

            let response = client.execute(request).unwrap();
            let r = response.wait_for_completion().await;
            dbg!(r);
        });
    }
}
