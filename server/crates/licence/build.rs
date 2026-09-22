//! Compiles the licensing authority's public key into the binary.
//!
//! Read at build time from `GIAMS_LICENCE_PUBLIC_KEY`, or from the checked-in
//! development key when that is not set. Never read at run time: a public key
//! loaded from a file beside the binary is a public key anyone can replace
//! with their own, and then issue themselves any licence they like.

fn main() {
    // The development authority. A release build sets the variable to the
    // production key; this default only ever verifies development licences.
    const DEVELOPMENT: &str = "RKm7T_IC9L3DzpTmz0sveo988KWmIjGnmK58dBxZZPo";

    let key = std::env::var("GIAMS_LICENCE_PUBLIC_KEY").unwrap_or_else(|_| DEVELOPMENT.into());

    println!("cargo:rustc-env=GIAMS_LICENCE_PUBLIC_KEY={key}");
    println!("cargo:rerun-if-env-changed=GIAMS_LICENCE_PUBLIC_KEY");
}
