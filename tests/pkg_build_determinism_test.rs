//! #1808: a PACKAGE build is a deterministic function of its sources. The
//! host/browser determinism sweeps cover `spec/wasm_cross` (single files);
//! the cross-module top-level initializer order lives only in a package,
//! and a hash-seeded map in the initializer dependency walk moved a whole
//! module's init block between two builds of ONE binary. Build the package
//! fixture several times in-process-fresh binaries and demand one byte
//! sequence — the ratchet the sweeps cannot see.

use std::process::Command;

fn almide_bin() -> String {
    env!("CARGO_BIN_EXE_almide").to_string()
}

#[test]
fn package_build_is_byte_identical_across_repeated_builds() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let entry = root.join("spec/wasm_cross_pkg/main.almd");
    let tmp = std::env::temp_dir().join(format!("almide-pkg-det-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("temp dir");
    let mut digests = Vec::new();
    for i in 0..4 {
        let out = tmp.join(format!("m{i}.wasm"));
        let st = Command::new(almide_bin())
            .args(["build", entry.to_str().unwrap(), "--target", "wasm", "-o", out.to_str().unwrap()])
            .current_dir(root)
            .output()
            .expect("spawn almide build");
        assert!(st.status.success(), "build {i} failed: {}", String::from_utf8_lossy(&st.stderr));
        digests.push(std::fs::read(&out).expect("read module"));
    }
    let _ = std::fs::remove_dir_all(&tmp);
    for (i, d) in digests.iter().enumerate().skip(1) {
        assert!(
            d == &digests[0],
            "package build {i} differs from build 0 ({} vs {} bytes): the initializer order or another table is hash-seeded",
            d.len(),
            digests[0].len()
        );
    }
}
