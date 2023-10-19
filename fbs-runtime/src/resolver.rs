#![allow(nonstandard_style)]
use std::ffi::{CStr, CString};
use std::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;
use std::mem::MaybeUninit;
use std::sync::{Arc, Mutex};

use fbs_library::eventfd::{EventFd, EventFdFlags};
use libc::{timespec, addrinfo, sigval, SIGEV_THREAD};
use libc::pthread_attr_t;

use fbs_library::ip_address::IpAddress;

use crate::{AsyncReadStruct, async_read_struct};

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

fn gai_code_to_error(code: libc::c_int) -> String {
    unsafe { CStr::from_ptr(gai_strerror(code)).to_string_lossy().into_owned() }
}

pub enum DnsRecord {
    Empty,
    A(IpAddress),
    Cname(String),
}

pub struct DnsQuery {
    domain: String,
    internal: Arc<Mutex<GaiInnerData>>,
}

#[repr(C)]
struct GaiInnerData(gaicb, EventFd, Pin<Box<AsyncReadStruct<u64>>>);

impl GaiInnerData {
    fn new() -> Self {
        let eventfd = EventFd::new(0, EventFdFlags::new()).unwrap();
        let waiter = async_read_struct::<u64>(&eventfd, None);

        Self(unsafe { MaybeUninit::zeroed().assume_init() }, eventfd, Box::pin(waiter))
    }

    fn is_filled(&self) -> bool {
        !self.0.ar_name.is_null()
    }

    fn fill(&mut self, query: &DnsQuery) {
        self.0.ar_name = CString::new(query.domain.clone()).expect("Forbidden characters in dns record name").into_raw();
    }
}

impl Drop for GaiInnerData {
    fn drop(&mut self) {
        unsafe {
            if !self.0.ar_name.is_null() {
                std::mem::drop(CString::from_raw(self.0.ar_name.cast_mut()));
            }

            if !self.0.ar_result.is_null() {
                libc::freeaddrinfo(self.0.ar_result);
            }
        }
    }
}

impl DnsQuery {
    pub fn new(domain: String) -> Self {
        Self {
            domain,
            internal: Arc::new(Mutex::new(GaiInnerData::new())),
        }
    }
}

impl Default for DnsQuery {
    fn default() -> Self {
        DnsQuery::new(String::new())
    }
}

impl Drop for DnsQuery {
    fn drop(&mut self) {

    }
}

impl Future for DnsQuery {
    type Output = DnsRecord;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = self.as_ref().get_ref();
        let mut gai_data = inner.internal.lock().unwrap();

        if !gai_data.is_filled() {
            gai_data.fill(inner);

            unsafe {
                let mut handler: sigevent = MaybeUninit::zeroed().assume_init();
                handler.sigev_notify = SIGEV_THREAD;
                handler._sigev_un._sigev_thread._function = Some(sigev_notifier);
                handler._sigev_un._sigev_thread._attribute = std::ptr::null_mut();
                handler.sigev_value.sival_ptr = Arc::into_raw(self.internal.clone()) as *mut libc::c_void;

                let mut entries = &mut gai_data.0 as *mut gaicb;
                let result = getaddrinfo_a(GAI_NOWAIT as i32, &mut entries as *mut *mut gaicb, 1, &mut handler);

                assert_eq!(result, 0);
            }
        }

        let result = gai_data.2.as_mut().poll(cx);
        match result {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => {
                Poll::Ready(DnsRecord::Empty)
            }
        }
    }
}

unsafe extern "C" fn sigev_notifier(ptr: sigval) {
    let ptr = ptr.sival_ptr as *const Mutex<GaiInnerData>;
    let inner_ptr = Arc::from_raw(ptr);
    let inner = inner_ptr.lock().unwrap();

    inner.1.write(1);

    std::mem::drop(inner);
    std::mem::forget(inner_ptr);
}

#[cfg(test)]
mod test {
    use crate::async_run;
    use super::*;

    #[test]
    fn async_resolver_build_test() {
        async_run(async {

            let result = unsafe { gai_cancel(std::ptr::null_mut()) };
            println!("result {}", gai_code_to_error(result));

            // just linking and not crashing is a success
            assert!(true);
        });
    }

    #[test]
    fn async_resolver_test1() {
        async_run(async {
            let query = DnsQuery::new("google.com".to_string());
            let result = query.await;
        });
    }
}