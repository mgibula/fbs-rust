use std::num::ParseIntError;
use std::mem::{MaybeUninit, size_of};

use super::ip_address::*;

use thiserror::Error;

#[derive(Debug, PartialEq, Eq)]
pub struct SocketIpAddress {
    address: IpAddress,
    port: u16,
}

#[repr(C)]
pub union SocketAddressBinary {
    generic: libc::sockaddr,
    ipv4: libc::sockaddr_in,
    ipv6: libc::sockaddr_in6,
}

impl Default for SocketAddressBinary {
    fn default() -> Self {
        unsafe {
            let mut result = MaybeUninit::<SocketAddressBinary>::zeroed().assume_init();
            result.generic.sa_family = libc::AF_UNSPEC as u16;
            result
        }
    }
}

impl SocketAddressBinary {
    pub fn to_ip_address(&self) -> Option<IpAddress> {
        unsafe {
            match self.generic.sa_family as i32 {
                libc::AF_INET => Some(IpAddress::from_inet4(&self.ipv4.sin_addr)),
                libc::AF_INET6 => Some(IpAddress::from_inet6(&self.ipv6.sin6_addr)),
                _ => None
            }
        }
    }

    #[inline]
    pub fn length(&self) -> usize {
        unsafe {
            match self.generic.sa_family as i32 {
                libc::AF_INET => size_of::<libc::sockaddr_in>(),
                libc::AF_INET6 => size_of::<libc::sockaddr_in6>(),
                _ => 0
            }
        }
    }

    #[inline(always)]
    pub fn sockaddr_ptr(&self) -> *const libc::sockaddr {
        unsafe {
            &self.generic
        }
    }

    #[inline(always)]
    pub fn sockaddr_ptr_mut(&mut self) -> *mut libc::sockaddr {
        unsafe {
            &mut self.generic
        }
    }
}

#[derive(Error, Debug)]
pub enum SocketAddressFormatError {
    #[error("Port separator missing")]
    NoPortSeparator,
    #[error("Port format invalid")]
    PortInvalid(#[from] ParseIntError),
    #[error("Address format invalid")]
    AddressInvalid(#[from] IpAddressFormatError),
}

impl SocketIpAddress {
    pub fn from_ip_address(address: IpAddress, port: u16) -> SocketIpAddress {
        SocketIpAddress { address, port }
    }

    pub fn from_text(value: &str, default_port: Option<u16>) -> Result<SocketIpAddress, SocketAddressFormatError> {
        let double_colon = value.rfind(':');
        let bracket = value.rfind(']');

        let (address, port) = match (bracket, double_colon, default_port) {
            // ipv6 with port (double_colon is behind closing bracket)
            (Some(bracket), Some(double_colon), _) if double_colon > bracket => {
                let port = &value[double_colon + 1 ..];
                let port = port.parse::<u16>()?;

                let address = &value[0..double_colon];
                let address = address.trim_matches(|c| c == ']' || c == '[');
                (address, port)
            },
            // ipv6 without port
            (Some(bracket), Some(double_colon), Some(port)) => {
                let address = value.trim_matches(|c| c == ']' || c == '[');
                (address, port)
            },
            // ipv4 with port
            (None, Some(double_colon), _) => {
                let port = &value[double_colon + 1 ..];
                let port = port.parse::<u16>()?;

                let address = &value[0..double_colon];
                let address = address.trim_matches(|c| c == ']' || c == '[');
                (address, port)
            },
            // ipv4 without port
            (None, None, Some(port)) => (value, port),
            // others
            (_, _, _) => return Err(SocketAddressFormatError::NoPortSeparator),
        };

        Ok(SocketIpAddress {
            address: IpAddress::from_text(address)?,
            port,
        })
    }

    pub unsafe fn from_sockaddr_in(&value: &libc::sockaddr_in) -> SocketIpAddress {
        SocketIpAddress { address: IpAddress::from_inet4(&value.sin_addr), port: u16::from_be(value.sin_port) }
    }

    pub unsafe fn from_sockaddr_in6(&value: &libc::sockaddr_in6) -> SocketIpAddress {
        SocketIpAddress { address: IpAddress::from_inet6(&value.sin6_addr), port: u16::from_be(value.sin6_port) }
    }

    pub fn to_text(&self) -> String {
        if self.address.is_ipv4() {
            format!("{}:{}", self.address.to_text(), self.port)
        } else {
            format!("[{}]:{}", self.address.to_text(), self.port)
        }
    }

    #[inline(always)]
    pub fn address(&self) -> IpAddress {
        self.address
    }

    #[inline(always)]
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn to_binary(&self) -> SocketAddressBinary {
        let mut result = unsafe { MaybeUninit::<SocketAddressBinary>::zeroed().assume_init() };

        match self.address {
            IpAddress::V4(addr) => {
                result.ipv4.sin_family = libc::AF_INET as u16;
                result.ipv4.sin_port = self.port.to_be();
                result.ipv4.sin_addr = addr;
            },
            IpAddress::V6(addr) => {
                result.ipv6.sin6_family = libc::AF_INET6 as u16;
                result.ipv6.sin6_port = self.port.to_be();
                result.ipv6.sin6_flowinfo = 0;
                result.ipv6.sin6_addr = addr;
                result.ipv6.sin6_scope_id = 0;
            },
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_address_from_text_ipv4() {
        let address = SocketIpAddress::from_text("127.0.0.1:2404", None).unwrap();

        assert_eq!(address.address().to_text(), "127.0.0.1");
        assert_eq!(address.port(), 2404);
        assert_eq!(address.to_text(), "127.0.0.1:2404");
    }

    #[test]
    fn socket_address_from_text_ipv6() {
        let address = SocketIpAddress::from_text("[2001:db8:3333:4444:5555:6666:7777:8888]:2404", None).unwrap();

        assert_eq!(address.address().to_text(), "2001:db8:3333:4444:5555:6666:7777:8888");
        assert_eq!(address.port(), 2404);
        assert_eq!(address.to_text(), "[2001:db8:3333:4444:5555:6666:7777:8888]:2404");
    }

    #[test]
    fn socket_address_from_text_ipv4_default() {
        let address = SocketIpAddress::from_text("127.0.0.1", Some(2404)).unwrap();

        assert_eq!(address.address().to_text(), "127.0.0.1");
        assert_eq!(address.port(), 2404);
        assert_eq!(address.to_text(), "127.0.0.1:2404");
    }

    #[test]
    fn socket_address_from_text_ipv6_default() {
        let address = SocketIpAddress::from_text("[2001:db8:3333:4444:5555:6666:7777:8888]", Some(2404)).unwrap();

        assert_eq!(address.address().to_text(), "2001:db8:3333:4444:5555:6666:7777:8888");
        assert_eq!(address.port(), 2404);
        assert_eq!(address.to_text(), "[2001:db8:3333:4444:5555:6666:7777:8888]:2404");
    }
}
