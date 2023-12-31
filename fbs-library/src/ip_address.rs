use std::ffi::NulError;
use std::mem::MaybeUninit;
use thiserror::Error;
use std::ffi::{CString, CStr};
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};

const INET_ADDRSTRLEN: usize = 16;
const INET6_ADDRSTRLEN: usize = 46;

extern "C" {
    pub fn inet_pton(af: libc::c_int, src: *const libc::c_char, dst: *mut libc::c_void) -> libc::c_int;
    pub fn inet_ntop(af: libc::c_int, src: *const libc::c_char, dst: *mut libc::c_char, size: libc::socklen_t) -> *const libc::c_char;
}

#[derive(Error, Debug)]
pub enum IpAddressFormatError {
    #[error("Null byte inside text")]
    NulError(#[from] NulError),
    #[error("Invalid address format")]
    AddressInvalid,
}

#[derive(Clone, Copy)]
pub enum IpAddress {
    V4(libc::in_addr),
    V6(libc::in6_addr),
}

impl Hash for IpAddress {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            IpAddress::V4(addr) => {
                let addr: &[u8] = unsafe { std::slice::from_raw_parts(addr as *const libc::in_addr as *const u8, std::mem::size_of::<libc::in_addr>()) };
                state.write(addr);
            },
            IpAddress::V6(addr) => {
                let addr: &[u8] = unsafe { std::slice::from_raw_parts(addr as *const libc::in6_addr as *const u8, std::mem::size_of::<libc::in6_addr>()) };
                state.write(addr);
            },
        }
    }
}

impl Debug for IpAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_text())
    }
}

impl PartialEq for IpAddress {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (IpAddress::V4(addr1), IpAddress::V4(addr2)) => {
                let addr1: &[u8] = unsafe { std::slice::from_raw_parts(addr1 as *const libc::in_addr as *const u8, std::mem::size_of::<libc::in_addr>()) };
                let addr2: &[u8] = unsafe { std::slice::from_raw_parts(addr2 as *const libc::in_addr as *const u8, std::mem::size_of::<libc::in_addr>()) };

                addr1 == addr2
            },
            (IpAddress::V6(addr1), IpAddress::V6(addr2)) => {
                let addr1: &[u8] = unsafe { std::slice::from_raw_parts(addr1 as *const libc::in6_addr as *const u8, std::mem::size_of::<libc::in6_addr>()) };
                let addr2: &[u8] = unsafe { std::slice::from_raw_parts(addr2 as *const libc::in6_addr as *const u8, std::mem::size_of::<libc::in6_addr>()) };

                addr1 == addr2
            },
            (_, _) => false
        }
    }
}

impl Eq for IpAddress {}

impl IpAddress {
    pub fn from_text(value: &str) -> Result<IpAddress, IpAddressFormatError> {
        let c_value = CString::new(value)?;

        let address4 = IpAddress::try_ipv4_from_text(c_value.as_c_str());
        match address4 {
            Some(address) => { return Ok(address) },
            None => { },
        }

        let address6 = IpAddress::try_ipv6_from_text(c_value.as_c_str());
        match address6 {
            Some(address) => Ok(address),
            None => Err(IpAddressFormatError::AddressInvalid)
        }
    }

    pub unsafe fn from_inet4(value: &libc::in_addr) -> IpAddress {
        IpAddress::V4(*value)
    }

    pub unsafe fn from_inet6(value: &libc::in6_addr) -> IpAddress {
        IpAddress::V6(*value)
    }

    fn try_ipv4_from_text(value: &CStr) -> Option<IpAddress> {
        unsafe {
            let mut addr4  = MaybeUninit::<libc::in_addr>::zeroed().assume_init();
            let dst = &mut addr4 as *mut libc::in_addr as *mut libc::c_void;
            let succeeded = inet_pton(libc::AF_INET, value.as_ptr(), dst);

            if succeeded > 0 {
                Some(IpAddress::V4(addr4))
            } else {
                None
            }
        }
    }

    fn try_ipv6_from_text(value: &CStr) -> Option<IpAddress> {
        unsafe {
            let mut addr6  = MaybeUninit::<libc::in6_addr>::zeroed().assume_init();
            let dst = &mut addr6 as *mut libc::in6_addr as *mut libc::c_void;
            let succeeded = inet_pton(libc::AF_INET6, value.as_ptr(), dst);

            if succeeded > 0 {
                Some(IpAddress::V6(addr6))
            } else {
                None
            }
        }
    }

    pub fn to_text(&self) -> String {
        match self {
            IpAddress::V4(addr) => self.to_text_ipv4(addr),
            IpAddress::V6(addr) => self.to_text_ipv6(addr)
        }
    }

    fn to_text_ipv4(&self, addr: &libc::in_addr) -> String {
        unsafe {
            let src = addr as *const libc::in_addr as *const libc::c_char;
            let mut dst: [u8; INET_ADDRSTRLEN] = MaybeUninit::zeroed().assume_init();

            inet_ntop(libc::AF_INET, src, dst.as_mut_ptr() as *mut i8, dst.len() as libc::socklen_t);
            return String::from_utf8_lossy(CStr::from_bytes_until_nul(&dst).unwrap().to_bytes()).to_string();
        }
    }

    fn to_text_ipv6(&self, addr: &libc::in6_addr) -> String {
        unsafe {
            let src = addr as *const libc::in6_addr as *const libc::c_char;
            let mut dst: [u8; INET6_ADDRSTRLEN] = MaybeUninit::zeroed().assume_init();

            inet_ntop(libc::AF_INET6, src, dst.as_mut_ptr() as *mut i8, dst.len() as libc::socklen_t);
            return String::from_utf8_lossy(CStr::from_bytes_until_nul(&dst).unwrap().to_bytes()).to_string();
        }
    }

    #[inline(always)]
    pub fn is_ipv4(&self) -> bool {
        match self {
            IpAddress::V4(_) => true,
            _ => false,
        }
    }

    #[inline(always)]
    pub fn is_ipv6(&self) -> bool {
        match self {
            IpAddress::V6(_) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_and_from_addr4() {
        let binary = IpAddress::from_text("127.0.0.1").unwrap();
        assert_eq!(binary.is_ipv4(), true);

        let text = binary.to_text();
        assert_eq!(text, String::from("127.0.0.1"));
    }

    #[test]
    fn to_and_from_addr6() {
        let binary = IpAddress::from_text("2001:db8:3333:4444:5555:6666:7777:8888").unwrap();
        assert_eq!(binary.is_ipv6(), true);

        let text = binary.to_text();
        assert_eq!(text, String::from("2001:db8:3333:4444:5555:6666:7777:8888"));
    }

    #[test]
    fn compare_addr() {
        let addr1 = IpAddress::from_text("127.0.0.1").unwrap();
        let addr2 = IpAddress::from_text("127.0.0.2").unwrap();
        let addr3 = IpAddress::from_text("2001:db8:3333:4444:5555:6666:7777:8888").unwrap();
        let addr4 = IpAddress::from_text("2001:db8:3333:4444:5555:6666:7777:8889").unwrap();

        assert_eq!(addr1, addr1);
        assert_ne!(addr1, addr2);
        assert_ne!(addr1, addr3);
        assert_eq!(addr3, addr3);
        assert_ne!(addr3, addr4);
    }
}
