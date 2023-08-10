use fbs_runtime::*;

fn main() {
    println!("Hello, world!");

    async_run(async {
        // async_open("/tmp/testowy-uring.txt").await;
    });

    println!("Bye, world!");
}
