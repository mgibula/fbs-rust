use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    let bindings = bindgen::Builder::default()
        .ctypes_prefix("libc")
        .allowlist_type("gaicb")
        .allowlist_type("sigevent")
        .allowlist_type("__sigval_t")
        .allowlist_type("__pid_t")
        .allowlist_function("getaddrinfo_a")
        .allowlist_function("gai_suspend")
        .allowlist_function("gai_error")
        .allowlist_function("gai_cancel")
        .allowlist_function("gai_strerror")
        .allowlist_var("GAI_WAIT")
        .allowlist_var("GAI_NOWAIT")
        .allowlist_var("EAI_AGAIN")
        .allowlist_var("EAI_MEMORY")
        .allowlist_var("EAI_SYSTEM")
        .allowlist_var("EAI_ALLDONE")
        .allowlist_var("EAI_INTR")
        .allowlist_var("EAI_CANCELED")
        .allowlist_var("EAI_NOTCANCELED")
        .allowlist_var("EAI_INPROGRESS")
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
