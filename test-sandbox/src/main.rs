use fbs_runtime::*;
use std::net::TcpListener;
use std::os::fd::{OwnedFd, FromRawFd, AsRawFd};

fn main() {
    println!("Hello, world!");

    async_run(async {
        let listener = TcpListener::bind("0.0.0.0:2404").unwrap();


    });

    println!("Bye, world!");
}
