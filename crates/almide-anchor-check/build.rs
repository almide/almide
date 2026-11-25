//! Build script for almide-anchor-check: refresh the in-crate snapshot of the
//! proof script whose routine byte lists `src/proved.rs` reads
//! (`proofs/StructuralDecode.v` → `src/generated/`).
//!
//! The proof script is outside this package, so the snapshot is COMMITTED and
//! refreshed only inside an almide checkout; a vendored or packaged
//! almide-anchor-check compiles the committed copy (#3361). Inside a checkout
//! an edit to the proof script reaches the check on the next build, and the
//! refreshed snapshot is committed with it
//! (`scripts/check-build-inputs-in-package.sh` fails a commit that forgot).

#[path = "buildscript/in_checkout.rs"]
mod in_checkout;

/// The proof scripts `src/proved.rs` reads, by name.
const SHARED: &[&str] = &["StructuralDecode.v"];

fn main() {
    let manifest_dir = in_checkout::manifest_dir();
    let Some(root) = in_checkout::checkout_root(&manifest_dir) else {
        // Standalone (vendored / packaged): the committed snapshot is the input.
        println!("cargo:rerun-if-changed=build.rs");
        return;
    };
    let src = root.join("proofs");
    let dest = manifest_dir.join("src/generated");
    for name in SHARED {
        let path = src.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        in_checkout::write_if_changed(&dest.join(name), &text);
    }
}
