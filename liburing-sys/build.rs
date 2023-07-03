use std::env;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    Command::new("rm")
        .arg("-rf")
        .arg(format!("{}/liburing", out_dir.clone()))
        .status()
        .expect("failed to remove");

    Command::new("cp")
        .arg("-r")
        .arg("liburing")
        .arg(out_dir.clone())
        .status()
        .expect("failed to copy");
}