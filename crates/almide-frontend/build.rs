//! Build script for almide-frontend.
//!
//! `removed_spellings.rs` reads the dialect ledger, `proofs/dialect-epochs.toml`,
//! which is outside this package. The ledger is copied to the committed
//! `src/generated/dialect-epochs.toml` here, and only inside an almide
//! checkout; a vendored or packaged almide-frontend compiles the committed copy
//! (#3361). The native regex runtime, `runtime/rs/src/regex.rs`, which the
//! checker compiles in for E102, is copied to `src/generated/regex_rt.rs` the
//! same way.

#[path = "buildscript/in_checkout.rs"]
mod in_checkout;

fn main() {
    let manifest_dir = in_checkout::manifest_dir();
    let Some(root) = in_checkout::checkout_root(&manifest_dir) else {
        // Standalone (vendored / packaged): the committed copy is the input.
        println!("cargo:rerun-if-changed=build.rs");
        return;
    };
    copy_in(&root.join("proofs/dialect-epochs.toml"), &manifest_dir.join("src/generated/dialect-epochs.toml"));
    copy_in(&root.join("runtime/rs/src/regex.rs"), &manifest_dir.join("src/generated/regex_rt.rs"));
}

/// Copy a checkout input into the committed generated file, writing only on a
/// change so an unchanged input leaves the tree clean.
fn copy_in(src: &std::path::Path, out: &std::path::Path) {
    println!("cargo:rerun-if-changed={}", src.display());
    let text = std::fs::read_to_string(src).unwrap_or_else(|e| panic!("{} is readable in a checkout: {e}", src.display()));
    if std::fs::read_to_string(out).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(out, text).unwrap_or_else(|e| panic!("{} is writable: {e}", out.display()));
    }
}
