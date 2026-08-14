//! Kernel conformance (edit-locality Stage 3, C-280): the native backend's
//! stdout for `spec/wasm_cross/kernel_conformance.almd` must be EXACTLY the
//! trace the λ_almd kernel semantics assigns to its image `kAll` — pinned at
//! Lean compile time in crates/almide-edit-belt/AlmideEditBelt/Conformance.lean
//! (#guard + eval_sound + ev_det). The wasm leg is covered by the wasm_cross
//! harness. The literal below duplicates the Lean literal by hand; that
//! reviewed link is the trusted seam, per docs/contracts/proven-vs-trusted.md.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[test]
fn kernel_conformance_native_stdout_matches_kernel_trace() {
    let repo_root = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(almide())
        .args(["run", "spec/wasm_cross/kernel_conformance.almd"])
        .current_dir(repo_root)
        .output()
        .expect("run kernel conformance fixture");
    assert!(
        out.status.success(),
        "fixture must run clean, stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The kernel trace of `kAll` (Conformance.lean), one line per `print`.
    let expected = "alpha\nbeta\ngamma\ngot-ok\ngot-err\nreified\nfive-ok\ninside\noutside\n";
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        expected,
        "native stdout diverged from the kernel-semantic trace"
    );
}
