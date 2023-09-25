use std::env;
use std::process::Command;
use std::path::PathBuf;
use cmake;

fn main() {
    println!("cargo:rerun-if-changed=curl");
    let out_dir = env::var("OUT_DIR").unwrap();

    Command::new("rm")
        .arg("-rf")
        .arg(format!("{}/curl", out_dir.clone()))
        .status()
        .expect("failed to remove");

    Command::new("cp")
        .arg("-r")
        .arg("curl")
        .arg(out_dir.clone())
        .status()
        .expect("failed to copy");

    let mut config = cmake::Config::new(format!("{}/curl", out_dir.clone()));
    config.define("BUILD_CURL_EXE", "OFF")
            .define("BUILD_SHARED_LIBS", "OFF")
            .define("BUILD_STATIC_LIBS", "ON")
            .define("BUILD_TESTING", "OFF")
            .define("CURL_ENABLE_SSL", "OFF")
            .define("USE_LIBIDN2", "OFF")
            .profile("Release");

    let dst = config.build();

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-lib=static=curl");

    let bindings = bindgen::Builder::default()
        .ctypes_prefix("libc")
        .clang_arg("-D_GNU_SOURCE")
        .clang_arg(format!("-I{}/include", dst.display()))
        .allowlist_function("curl.*")
        .opaque_type("^curl.*")
        .opaque_type("^CURL.*")
        .allowlist_type("^curl.*")
        .allowlist_type("^CURL.*")
        .allowlist_recursively(false)
        .header("wrapper.h")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(out_dir);
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
