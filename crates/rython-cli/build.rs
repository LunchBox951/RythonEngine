//! Build script for `rython-cli`.
//!
//! Forwards the release-seal environment variables through to `rustc` as
//! compile-time constants readable via `option_env!` in `release_seal.rs`.
//!
//! When *none* of these variables are set, the binary is built "unsealed" —
//! it still compiles and `cargo test` still works, but at runtime the seal
//! verifier refuses to enter release mode.

fn main() {
    const FORWARDED: &[&str] = &[
        "RYTHON_BUNDLE_HASH",
        "RYTHON_STDLIB_HASH",
        "RYTHON_LIBDYNLOAD_HASH",
        "RYTHON_STDLIB_ZIP_NAME",
        "RYTHON_ENTRY_POINT",
        "RYTHON_SEALED",
    ];

    for var in FORWARDED {
        println!("cargo:rerun-if-env-changed={var}");
        if let Ok(val) = std::env::var(var) {
            println!("cargo:rustc-env={var}={val}");
        }
    }
}
