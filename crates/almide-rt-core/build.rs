//! Build script for almide-rt-core: refresh the in-crate snapshot of the two
//! almide-base texts its cores inline (`crates/almide-base/src/err_text.rs`,
//! C-418's escaper, and `fs_read_bounded.rs`, #4201's bounded reader →
//! `src/generated/`).
//!
//! almide-base is outside this package, so the snapshot is COMMITTED and
//! refreshed only inside an almide checkout; a vendored or packaged
//! almide-rt-core compiles the committed copy (#3361). Inside a checkout an
//! edit to almide-base reaches the cores on the next build, and the refreshed
//! snapshot is committed with it (`scripts/check-build-inputs-in-package.sh`
//! fails a commit that forgot).

#[path = "buildscript/in_checkout.rs"]
mod in_checkout;

/// The almide-base files the cores `include!`, by name.
const SHARED: &[&str] = &["err_text.rs", "fs_read_bounded.rs"];

fn main() {
    let manifest_dir = in_checkout::manifest_dir();
    let Some(root) = in_checkout::checkout_root(&manifest_dir) else {
        // Standalone (vendored / packaged): the committed snapshot is the input.
        println!("cargo:rerun-if-changed=build.rs");
        return;
    };
    let src = root.join("crates/almide-base/src");
    let dest = manifest_dir.join("src/generated");
    for name in SHARED {
        let path = src.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        in_checkout::write_if_changed(&dest.join(name), &text);
    }
}
