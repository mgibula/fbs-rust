#![allow(nonstandard_style)]
use std::ffi::{CStr, CString};

#[repr(C)]
struct gaicb {
    ar_name: *const libc::c_char,
    ar_service: *const libc::c_char,
    ar_request: *const libc::addrinfo,
    ar_result: *mut libc::addrinfo,
}

extern "C" {
    fn getaddrinfo_a(mode: libc::c_int, list: *mut *mut gaicb, nitems: libc::c_int, sevp: *mut libc::sigevent) -> libc::c_int;

    fn gai_suspend(list: *const *const gaicb, nitems: libc::c_int, timeout: *const libc::timespec) -> libc::c_int;

    fn gai_error(req: *mut gaicb) -> libc::c_int;

    fn gai_cancel(req: *mut gaicb) -> libc::c_int;

    fn gai_strerror(code: libc::c_int) -> *const libc::c_char;
}

fn gai_code_to_error(code: libc::c_int) -> String {
    unsafe { CStr::from_ptr(gai_strerror(code)).to_string_lossy().into_owned() }
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

}