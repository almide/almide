//! #568: static-memory discipline for the Critical profile, evidenced on
//! the flight reference app (spec/wasm_cross/flight_pid_control.almd,
//! C-230). Three claims, each mechanical:
//!
//! 1. the app is CRITICAL-CLEAN — `almide check --profile critical`
//!    accepts it (bounded rules on every fn, capabilities deny-all; its
//!    only effect is the blessed print family);
//! 2. the KERNEL fns' emitted Rust is allocation-free (the WCET-shape
//!    token gate, as tests/wcet_kernel_test.rs pins for the Float
//!    kernel) — steady-state control arithmetic touches no heap;
//! 3. the wasm artifact runs to completion under a FIXED heap budget
//!    (`--heap-cap`), and the budget is REAL: an unbounded-allocation
//!    program under the same cap meets the defined OOM abort ("Error:
//!    out of memory", exit 1) instead of growing without bound. The
//!    partition-friendly shape (single memory, non-shared, no thread
//!    imports) is asserted on the artifact bytes.
//!
//! The requirements a partitioned host must provide are documented in
//! docs/project/PARTITIONED-RUNTIME.md.

use std::path::Path;
use std::process::Command;

const APP: &str = "spec/wasm_cross/flight_pid_control.almd";

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// The emitted text between `pub fn <name>` and its closing brace at
/// column 0 — crude but stable for the kernel's flat fns.
fn fn_body<'a>(code: &'a str, name: &str) -> &'a str {
    let start = code
        .find(&format!("pub fn {name}"))
        .unwrap_or_else(|| panic!("emitted Rust lost fn {name}"));
    let rest = &code[start..];
    let end = rest.find("\n}\n").map(|i| i + 3).unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn reference_app_is_critical_clean() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let out = Command::new(almide_bin())
        .args(["check", APP, "--profile", "critical"])
        .output()
        .expect("spawn almide check");
    assert!(
        out.status.success(),
        "the flight reference app fell out of the critical profile:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn kernel_fns_emit_allocation_free_rust() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let out = Command::new(almide_bin())
        .args([APP, "--target", "rust"])
        .output()
        .expect("spawn almide");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let code = String::from_utf8_lossy(&out.stdout).to_string();
    for name in ["fmul", "pid_step"] {
        let body = fn_body(&code, name);
        for forbidden in ["Vec", "Box<", "String", "format!", ".clone()", "RcCow"] {
            assert!(
                !body.contains(forbidden),
                "{name}'s emitted body grew `{forbidden}` — the static-memory shape broke:\n{body}"
            );
        }
    }
}

#[test]
fn fixed_heap_budget_suffices_and_is_enforced() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    if !wasmtime_available() {
        return;
    }
    let dir = std::env::temp_dir().join("almide-static-memory");
    std::fs::create_dir_all(&dir).expect("mkdir");

    // The reference app completes its full trace under a fixed 1 MiB
    // heap budget baked into the artifact.
    let app_wasm = dir.join("pid-cap.wasm");
    let out = Command::new(almide_bin())
        .args([
            "build",
            APP,
            "--target",
            "wasm",
            "--heap-cap",
            "1048576",
            "-o",
            app_wasm.to_str().unwrap(),
        ])
        .output()
        .expect("spawn almide build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new("wasmtime")
        .args(["run", app_wasm.to_str().unwrap()])
        .output()
        .expect("spawn wasmtime");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(stdout.lines().count(), 16, "the 16-step trace lost lines:\n{stdout}");

    // The same budget is ENFORCED: unbounded growth meets the defined
    // OOM abort at a deterministic point, not silent expansion.
    let glutton = dir.join("glutton.almd");
    std::fs::write(
        &glutton,
        "effect fn main() -> Unit = {\n  var xs: List[Int] = []\n  for i in 0..<100000 {\n    xs = xs + [i]\n  }\n  println(int.to_string(list.len(xs)))\n}\n",
    )
    .expect("write");
    let glutton_wasm = dir.join("glutton-cap.wasm");
    let out = Command::new(almide_bin())
        .args([
            "build",
            glutton.to_str().unwrap(),
            "--target",
            "wasm",
            "--heap-cap",
            "1048576",
            "-o",
            glutton_wasm.to_str().unwrap(),
        ])
        .output()
        .expect("spawn almide build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new("wasmtime")
        .args(["run", glutton_wasm.to_str().unwrap()])
        .output()
        .expect("spawn wasmtime");
    assert_eq!(run.status.code(), Some(1), "the OOM abort must answer exit 1");
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("out of memory"),
        "the OOM abort must be the defined message:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn artifact_is_partition_shaped() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let dir = std::env::temp_dir().join("almide-static-memory");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let app_wasm = dir.join("pid-shape.wasm");
    let out = Command::new(almide_bin())
        .args([
            "build",
            APP,
            "--target",
            "wasm",
            "--heap-cap",
            "1048576",
            "-o",
            app_wasm.to_str().unwrap(),
        ])
        .output()
        .expect("spawn almide build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let bytes = std::fs::read(&app_wasm).expect("read wasm");

    // Single non-shared memory, no thread-flavoured imports: the space
    // half of the partition claim, read off the artifact itself.
    let mut memories = 0;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        match payload.expect("parse wasm") {
            wasmparser::Payload::MemorySection(rdr) => {
                for m in rdr {
                    let m = m.expect("memory");
                    assert!(!m.shared, "the artifact grew a SHARED memory");
                    memories += 1;
                }
            }
            wasmparser::Payload::ImportSection(rdr) => {
                for group in rdr {
                    for item in group.expect("imports") {
                        let (_, imp) = item.expect("import");
                        assert!(
                            !imp.module.contains("thread") && !imp.name.contains("thread"),
                            "the artifact grew a thread import: {}::{}",
                            imp.module,
                            imp.name
                        );
                    }
                }
            }
            _ => {}
        }
    }
    assert_eq!(memories, 1, "expected exactly one linear memory");
}
