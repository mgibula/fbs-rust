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
            .define("CURL_ZLIB", "OFF")
            .profile("Release");

    let dst = config.build();

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-lib=static=curl");

    let bindings = bindgen::Builder::default()
        .ctypes_prefix("libc")
        .clang_arg("-D_GNU_SOURCE")
        .clang_arg(format!("-I{}/include", dst.display()))
        .allowlist_function("curl.*")
        .opaque_type("curl_httppost")
        .opaque_type("curl_fileinfo")
        .opaque_type("curl_sockaddr")
        .opaque_type("curl_khkey")
        .opaque_type("curl_hstsentry")
        .opaque_type("curl_index")
        .opaque_type("curl_mime")
        .opaque_type("curl_mimepart")
        .opaque_type("curl_forms")
        .opaque_type("curl_slist")
        .opaque_type("curl_ssl_backend")
        .opaque_type("curl_certinfo")
        .opaque_type("curl_tlssessioninfo")
        .opaque_type("curl_version_info_data")
        .opaque_type("curl_blob")
        .opaque_type("curl_waitfd")
        .opaque_type("curl_pushheaders")
        .opaque_type("curl_easyoption")
        .opaque_type("curl_ws_frame")
        .opaque_type("curl_ws_frame")
        .opaque_type("Curl_share")
        .opaque_type("Curl_URL")
        .opaque_type("Curl_easy")
        .opaque_type("Curl_multi")
        .allowlist_type("Curl_easy")
        .allowlist_type("Curl_multi")
        .allowlist_type("Curl_share")
        .allowlist_type("Curl_URL")
        .allowlist_type("^curl.*")
        .allowlist_type("^CURL.*")
        .allowlist_var("^CURL.*")
        .allowlist_recursively(false)
        .prepend_enum_name(false)
        .header("wrapper.h")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(out_dir);
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
