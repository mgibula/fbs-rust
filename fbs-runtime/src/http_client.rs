use libcurl_sys::*;

pub struct HttpClient {
    curl: *mut libcurl_sys::CURLM,
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            curl: unsafe { curl_multi_init() }
        }
    }
}

impl Drop for HttpClient {
    fn drop(&mut self) {
        unsafe {
            curl_multi_cleanup(self.curl);
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
}
