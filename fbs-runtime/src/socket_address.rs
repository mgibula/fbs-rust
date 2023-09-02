use super::ip_address::*;
use thiserror::Error;
use std::num::ParseIntError;

pub struct SocketIpAddress {
    address: IpAddress,
    port: u16,
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
    pub fn from_text(value: &str) -> Result<SocketIpAddress, SocketAddressFormatError> {
        let separator_index = value.rfind(':');
        let index = match separator_index {
            Some(index) => index,
            None => { return Err(SocketAddressFormatError::NoPortSeparator) }
        };

        let port = &value[index + 1 ..];
        let port = port.parse::<u16>()?;

        let address = &value[0..index];
        let address = address.trim_matches(|c| c == ']' || c == '[');

        Ok(SocketIpAddress {
            address: IpAddress::from_text(address)?,
            port,
        })
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_address_from_text_ipv4() {
        let address = SocketIpAddress::from_text("127.0.0.1:2404").unwrap();

        assert_eq!(address.address().to_text(), "127.0.0.1");
        assert_eq!(address.port(), 2404);
        assert_eq!(address.to_text(), "127.0.0.1:2404");
    }

    #[test]
    fn socket_address_from_text_ipv6() {
        let address = SocketIpAddress::from_text("[2001:db8:3333:4444:5555:6666:7777:8888]:2404").unwrap();

        assert_eq!(address.address().to_text(), "2001:db8:3333:4444:5555:6666:7777:8888");
        assert_eq!(address.port(), 2404);
        assert_eq!(address.to_text(), "[2001:db8:3333:4444:5555:6666:7777:8888]:2404");
    }
}
