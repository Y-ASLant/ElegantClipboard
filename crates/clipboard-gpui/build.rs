fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        // The GPUI settings view builds a large native element tree on the main thread.
        // The MSVC default reserve of 1 MiB overflows when opening that window.
        println!("cargo:rustc-link-arg=/STACK:8388608");
    }
}
