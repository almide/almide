//! Build script for almide-frontend.
//!
//! `removed_spellings.rs` reads the dialect ledger, `proofs/dialect-epochs.toml`,
//! which is outside this package. The ledger is copied to the committed
//! `src/generated/dialect-epochs.toml` here, and only inside an almide
//! checkout; a vendored or packaged almide-frontend compiles the committed copy
//! (#3361).

#[path = "buildscript/in_checkout.rs"]
mod in_checkout;

fn main() {
    let manifest_dir = in_checkout::manifest_dir();
    let Some(root) = in_checkout::checkout_root(&manifest_dir) else {
        // Standalone (vendored / packaged): the committed copy is the input.
        println!("cargo:rerun-if-changed=build.rs");
        return;
    };
    let src = root.join("proofs/dialect-epochs.toml");
    println!("cargo:rerun-if-changed={}", src.display());
    let ledger = std::fs::read_to_string(&src).expect("proofs/dialect-epochs.toml is readable in a checkout");
    let out = manifest_dir.join("src/generated/dialect-epochs.toml");
    // Write only on a change, so an unchanged ledger leaves the tree clean.
    if std::fs::read_to_string(&out).ok().as_deref() != Some(ledger.as_str()) {
        std::fs::write(&out, ledger).expect("src/generated/dialect-epochs.toml is writable");
    }
}
