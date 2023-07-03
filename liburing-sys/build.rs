use std::env;
use std::process::Command;
use std::path::PathBuf;

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

    Command::new("make")
        .arg("config-host.mak")
        .current_dir(format!("{}/liburing", out_dir.clone()))
        .env("CFLAGS", "-fPIC")
        .status()
        .expect("failed to execute make");

    Command::new("make")
        .arg("-C")
        .arg("src")
        .arg("liburing-ffi.a")
        .current_dir(format!("{}/liburing", out_dir.clone()))
        .env("CFLAGS", "-fPIC")
        .status()
        .expect("failed to execute make");

    println!("cargo:rustc-link-lib=uring-ffi");
    println!(
        "cargo:rustc-link-search=native={}/liburing/src",
        out_dir.clone()
    );

    let bindings = bindgen::Builder::default()
        .ctypes_prefix("libc")
        .clang_arg("-D_GNU_SOURCE")
        .clang_arg(format!("-I{}/liburing/src/include", out_dir.clone()))
        .allowlist_function("io_uring.*")
        .opaque_type(r"^io_uring_(?!params)[^\s]+*")
        .allowlist_type("io_uring.*")
        .allowlist_type("io_cqring_offsets")
        .allowlist_type("io_sqring_offsets")
        .allowlist_type("__kernel_.*")
        .allowlist_var("IORING_.*")
        .allowlist_var("IOSQE_.*")
        .allowlist_recursively(false)
        .header("wrapper.h")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(out_dir);
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

}