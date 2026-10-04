//! Build script for almide-interp: refresh the in-crate snapshot of the
//! runtime's vendored musl-libm (`runtime/rs/src/libm*.rs` →
//! `src/generated/libm/`), which `src/vendored_libm.rs` includes.
//!
//! `runtime/rs/` is outside this package, so the snapshot is COMMITTED and
//! refreshed only inside an almide checkout; a vendored or packaged
//! almide-interp compiles the committed copy (#3361). Inside a checkout an
//! edit to the runtime's libm reaches the interp on the next build, exactly as
//! the old direct `include!` did, and the refreshed snapshot is committed with
//! it (`scripts/check-build-inputs-in-package.sh` fails a commit that forgot).

#[path = "buildscript/in_checkout.rs"]
mod in_checkout;

fn main() {
    let manifest_dir = in_checkout::manifest_dir();
    let Some(root) = in_checkout::checkout_root(&manifest_dir) else {
        // Standalone (vendored / packaged): the committed snapshot is the input.
        println!("cargo:rerun-if-changed=build.rs");
        return;
    };
    let src = root.join("runtime/rs/src");
    let dest = manifest_dir.join("src/generated/libm");
    // libm.rs plus every part it `include!`s (libm_p2.rs …): all of them, so
    // the parts resolve next to the snapshot exactly as they do in the runtime.
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&src)
        .unwrap_or_else(|e| panic!("read {}: {e}", src.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with("libm") && name.ends_with(".rs")
        })
        .collect();
    files.sort();
    assert!(
        files.iter().any(|p| p.ends_with("libm.rs")),
        "{} has no libm.rs — the interp's transcendental floor moved; point build.rs at it",
        src.display()
    );
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        in_checkout::write_if_changed(&dest.join(path.file_name().expect("file name")), &text);
    }
}
