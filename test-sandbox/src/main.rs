use fbs_runtime::*;
use fbs_runtime::socket_address::*;
use fbs_runtime::Socket;

async fn handle_client(fd: Socket)
{
    println!("Inside handle client");
    'accept : loop {
        let mut buffer = vec![];
        buffer.resize(100, 0);

        let read_result = async_read(&fd, buffer).await;
        match read_result {
            Ok(buffer) => {
                println!("Got: {:?}", &buffer);
                if buffer.is_empty() {
                    break 'accept;
                }
            },
            Err((errno, _)) => {
                print!("Error: {}", errno);
                break 'accept;
            },
        }
    }

    let result = async_close(fd).await;
    match result {
        Ok(status) => println!("async close returned: {}", status),
        Err(err) => println!("async close errored: {}", err),
    }
}

fn main() {
    println!("Hello, world!");

    async_run(async {
        let server_address = SocketIpAddress::from_text("0.0.0.0:2404").unwrap();
        let mut socket = Socket::new(SocketDomain::Inet, SocketType::Stream, SocketFlags::new().flags());

        socket.set_option(SocketOptions::ReuseAddr(true)).unwrap();
        socket.listen(&server_address, 100).unwrap();
        loop {
            let client = socket.async_accept().await;
            match client {
                Ok(fd) => {
                    println!("Client accepted!");
                    async_spawn(async move { handle_client(fd).await });
                },
                Err(_) => { println!("Error while accepting") },
            }
        }

    });

    println!("Bye, world!");
}
