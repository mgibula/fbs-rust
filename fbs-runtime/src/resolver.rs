#![allow(nonstandard_style)]
use std::ffi::{CStr, CString};
use std::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;
use std::mem::MaybeUninit;
use std::sync::{Arc, Mutex};

use fbs_library::eventfd::{EventFd, EventFdFlags};
use fbs_library::ip_address::IpAddress;

use libc::{timespec, addrinfo, sigval, SIGEV_THREAD};
use libc::pthread_attr_t;
use thiserror::Error;

use crate::{AsyncReadStruct, async_read_struct};

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

fn gai_code_to_error(code: libc::c_int) -> String {
    unsafe { CStr::from_ptr(gai_strerror(code)).to_string_lossy().into_owned() }
}

#[derive(Error, Debug)]
pub enum ResolverError {
    #[error("The name server returned a temporary failure indication.  Try again later")]
    TemporaryError,
    #[error("Invalid parameters")]
    InvalidParameters,
    #[error("The name server returned a permanent failure indication")]
    PermanentError,
    #[error("Internal error")]
    InternalError(i32, String),
    #[error("No records found")]
    NoRecord,
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

    fn is_completed(&self) -> bool {
        unsafe {
            let ptr = &self.0 as *const gaicb;
            let status = gai_error(ptr.cast_mut());

            match status {
                EAI_INPROGRESS => false,
                _ => true,
            }
        }
    }

    fn get_result(&mut self) -> Result<Vec<IpAddress>, ResolverError> {
        let mut result = Vec::new();

        unsafe {
            let mut ptr = self.0.ar_result;
            let query_result = gai_error(&mut self.0);
            match query_result {
                0 => (),
                libc::EAI_AGAIN => return Err(ResolverError::TemporaryError),
                libc::EAI_BADFLAGS => return Err(ResolverError::InvalidParameters),
                libc::EAI_FAIL => return Err(ResolverError::PermanentError),
                libc::EAI_NODATA | libc::EAI_NONAME | libc::EAI_SERVICE => return Err(ResolverError::NoRecord),
                error => return Err(ResolverError::InternalError(error, gai_code_to_error(query_result))),
            }

            loop {
                match (*ptr).ai_family {
                    libc::AF_INET => {
                        let inet4_ptr = (*ptr).ai_addr as *mut libc::sockaddr_in;
                        result.push(IpAddress::from_inet4(&(*inet4_ptr).sin_addr));
                    },
                    libc::AF_INET6 => {
                        let inet6_ptr = (*ptr).ai_addr as *mut libc::sockaddr_in6;
                        result.push(IpAddress::from_inet6(&(*inet6_ptr).sin6_addr));
                    },
                    _ => (),
                }

                ptr = (*ptr).ai_next;
                if ptr.is_null() {
                    break;
                }
            }
        }

        Ok(result)
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

impl Future for DnsQuery {
    type Output = Result<Vec<IpAddress>, ResolverError>;

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

        // is_completed() is to protect against spurious wakeups
        match (result, gai_data.is_completed()) {
            (Poll::Pending, _)      => Poll::Pending,
            (Poll::Ready(_), false) => Poll::Pending,
            (Poll::Ready(_), true)  => {
                Poll::Ready(gai_data.get_result())
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

            assert!(result.is_ok());
        });
    }

    #[test]
    fn async_resolver_test2() {
        async_run(async {
            let query = DnsQuery::new("googlecom".to_string());
            let result = query.await;

            assert!(result.is_err());
        });
    }

}