use fbs_library::signalfd::SignalFd;
use fbs_library::signalfd::SignalFdFlags;
use fbs_library::signalfd::SignalFdInfo;
use fbs_library::sigset::{SignalSet, SignalMask, set_process_signal_mask};
use fbs_runtime::*;
use fbs_library::socket_address::*;
use fbs_library::socket::*;
use fbs_library::sigset::Signal;

async fn try_connect()
{
    let sock = Socket::new(SocketDomain::Inet, SocketType::Stream, SocketFlags::new().close_on_exec(true).flags());
    let destination = SocketIpAddress::from_text("99.83.207.202:80", None).unwrap();

    let connect_result = async_connect(&sock, destination).await;
    match connect_result {
        Err(_) => println!("Error while connecting"),
        Ok(_) => println!("connected"),
    }

    async_close(sock).await;
}

async fn handle_client(fd: Socket)
{
    println!("Inside handle client");
    'accept : loop {
        let mut buffer = vec![];
        buffer.resize(100, 0);

        let read_result = async_read_into(&fd, buffer, None).await;
        match read_result {
            Ok(buffer) => {
                println!("Got: {:?}", &buffer);
                if buffer.is_empty() {
                    break 'accept;
                }

                try_connect().await;
            },
            Err((errno, _)) => {
                print!("Error: {}", errno);
                break 'accept;
            },
        }
    }

    async_close(fd).await
}

fn main() {
    println!("Hello, world!");

    async_run(async {

        async_spawn(async {
            let mut mask = SignalSet::empty();
            mask.add(Signal::SIGINT);
            mask.add(Signal::SIGHUP);
            mask.add(Signal::SIGCHLD);

            set_process_signal_mask(SignalMask::Block, mask).unwrap();

            let sigfd = SignalFd::new(mask, SignalFdFlags::new().close_on_exec(true).flags()).unwrap();

            loop {
                let received = async_read_struct::<SignalFdInfo>(&sigfd, None).await;
                match received {
                    Err(error) => panic!("Got error {}", error),
                    Ok(info) => {
                        println!("Got {:?}", info.signal());
                    }
                }
            }
        });

        let server_address = SocketIpAddress::from_text("0.0.0.0:2404", None).unwrap();
        let mut socket = Socket::new(SocketDomain::Inet, SocketType::Stream, SocketFlags::new().flags());

        socket.set_option(SocketOptions::ReuseAddr(true)).unwrap();
        socket.listen(&server_address, 100).unwrap();
        loop {
            let client = async_accept(&socket, 0).await;
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
