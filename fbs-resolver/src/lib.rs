#![allow(nonstandard_style)]
use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;
use std::mem::MaybeUninit;
use std::sync::{Arc, Mutex};
use std::num::ParseIntError;

use fbs_library::eventfd::{EventFd, EventFdFlags};
use fbs_library::ip_address::IpAddress;
use fbs_library::socket_address::SocketIpAddress;

use fbs_runtime::{AsyncReadStruct, async_read_struct};

use libc::{timespec, addrinfo, sigval, SIGEV_THREAD};
use libc::pthread_attr_t;
use thiserror::Error;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

fn gai_code_to_error(code: libc::c_int) -> String {
    unsafe { CStr::from_ptr(gai_strerror(code)).to_string_lossy().into_owned() }
}

#[derive(Error, Debug, Clone)]
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

#[derive(Debug, Clone, Copy)]
pub struct DnsQueryFlags {
    return_ipv4: bool,
    return_ipv6: bool,
}

impl Default for DnsQueryFlags {
    fn default() -> Self {
        Self { return_ipv4: true, return_ipv6: false }
    }
}

impl DnsQueryFlags {
    pub fn return_ipv4(mut self, value: bool) -> Self {
        self.return_ipv4 = value;
        self
    }

    pub fn return_ipv6(mut self, value: bool) -> Self {
        self.return_ipv6 = value;
        self
    }
}

#[derive(Debug, Clone)]
pub struct DnsResult {
    addresses: Vec<IpAddress>,
}

impl Into<IpAddress> for DnsResult {
    fn into(self) -> IpAddress {
        self.one_record()
    }
}

impl DnsResult {
    pub fn all_record(&self) -> Vec<IpAddress> {
        self.addresses.clone()
    }

    pub fn one_record(&self) -> IpAddress {
        self.addresses[0]
    }
}

pub struct DnsQuery {
    domain: String,
    internal: Arc<Mutex<GaiInnerData>>,
}

#[repr(C)]
struct GaiInnerData(gaicb, EventFd, Pin<Box<AsyncReadStruct<u64>>>, DnsQueryFlags);

impl GaiInnerData {
    fn new(flags: DnsQueryFlags) -> Self {
        let eventfd = EventFd::new(0, EventFdFlags::new()).unwrap();
        let waiter = async_read_struct::<u64>(&eventfd, None);

        Self(unsafe { MaybeUninit::zeroed().assume_init() }, eventfd, Box::pin(waiter), flags)
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

    fn get_result(&mut self) -> Result<DnsResult, ResolverError> {
        let mut result: HashSet<IpAddress> = HashSet::new();

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
                        if self.3.return_ipv4 {
                            let inet4_ptr = (*ptr).ai_addr as *mut libc::sockaddr_in;
                            result.insert(IpAddress::from_inet4(&(*inet4_ptr).sin_addr));
                        }
                    },
                    libc::AF_INET6 => {
                        if self.3.return_ipv6 {
                            let inet6_ptr = (*ptr).ai_addr as *mut libc::sockaddr_in6;
                            result.insert(IpAddress::from_inet6(&(*inet6_ptr).sin6_addr));
                        }
                    },
                    _ => (),
                }

                ptr = (*ptr).ai_next;
                if ptr.is_null() {
                    break;
                }
            }
        }

        if result.is_empty() {
            return Err(ResolverError::NoRecord);
        }

        Ok(DnsResult { addresses: result.into_iter().collect() })
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
    pub fn new(domain: String, flags: DnsQueryFlags) -> Self {
        Self {
            domain,
            internal: Arc::new(Mutex::new(GaiInnerData::new(flags))),
        }
    }
}

impl Default for DnsQuery {
    fn default() -> Self {
        DnsQuery::new(String::new(), DnsQueryFlags::default())
    }
}

impl Future for DnsQuery {
    type Output = Result<DnsResult, ResolverError>;

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
                if result != 0 {
                    return Poll::Ready(Err(ResolverError::InternalError(result, gai_code_to_error(result))));
                }
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

#[derive(Error, Debug, Clone)]
pub enum ResolveAddressError {
    #[error("Port format invalid")]
    PortInvalid(#[from] ParseIntError),
    #[error("Port missing")]
    PortMissing,
    #[error("Resolver error")]
    ResolverError(#[from] ResolverError),
}

pub async fn resolve_address(address: &str, default_port: Option<u16>) -> Result<SocketIpAddress, ResolveAddressError> {
    let maybe_address = SocketIpAddress::from_text(address, default_port);
    match maybe_address {
        Ok(address) => return Ok(address),
        Err(_) => (),
    };

    let double_colon = address.rfind(':');
    let (address, port) = match (double_colon, default_port) {
        (Some(index), _) => {
            let port = &address[index + 1 ..];
            let port = port.parse::<u16>()?;
            let address = &address[0..index];

            (address, port)
        },
        (None, Some(port)) => (address, port),
        (None, None) => return Err(ResolveAddressError::PortMissing),
    };

    let query = DnsQuery::new(address.to_string(), DnsQueryFlags::default());
    let result = query.await?;

    Ok(SocketIpAddress::from_ip_address(result.one_record(), port))
}

#[cfg(test)]
mod test {
    use fbs_runtime::async_run;
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
            let query = DnsQuery::new("google.com".to_string(), DnsQueryFlags::default());
            let result = query.await;

            dbg!(&result);
            assert!(result.is_ok());
        });
    }

    #[test]
    fn async_resolver_test2() {
        async_run(async {
            let query = DnsQuery::new("googlecom".to_string(), DnsQueryFlags::default());
            let result = query.await;

            assert!(result.is_err());
        });
    }

    #[test]
    fn resolve_address_test1() {
        async_run(async {
            let address = resolve_address("google.com", Some(80)).await;
            let address = address.unwrap();

            assert_eq!(address.port(), 80);
        });
    }

    #[test]
    fn resolve_address_test2() {
        async_run(async {
            let address = resolve_address("google.com:88", Some(80)).await;
            let address = address.unwrap();

            assert_eq!(address.port(), 88);
        });
    }

    #[test]
    fn resolve_address_test3() {
        async_run(async {
            let address = resolve_address("google.com:88", None).await;
            let address = address.unwrap();

            assert_eq!(address.port(), 88);
        });
    }

    #[test]
    fn resolve_address_test4() {
        async_run(async {
            let address = resolve_address("google.com", None).await;
            assert!(address.is_err());
        });
    }
}