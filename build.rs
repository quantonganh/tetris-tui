fn main() {
    if let Ok(version) = std::env::var("VERSION") {
        println!("cargo:rustc-env=CARGO_PKG_VERSION={}", version);
    }
    println!("cargo:rerun-if-env-changed=VERSION");
}
