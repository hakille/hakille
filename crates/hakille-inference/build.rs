fn main() {
    println!("cargo:rerun-if-changed=c/glibc_compat.cpp");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    // On Linux gnu environments, compile compatibility shims for precompiled ort binaries
    if target_os == "linux" && (target_env == "gnu" || target_env.is_empty()) {
        cc::Build::new()
            .cpp(true)
            .file("c/glibc_compat.cpp")
            .compile("glibc_compat");
    }
}
