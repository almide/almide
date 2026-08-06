//! #1115: an interpolation segment whose type keeps an undecidable slot must
//! be a CHECK-time E025 — uniformly. Before the fix the five shapes below
//! split into three failure modes (rustc E0282 after check passed, the
//! AllTypesConcrete gate ICE, and silent E-concretization against the
//! never-silently-defaulted doctrine). Probes are the 2026-08-05 matrix cells.

use std::io::Write;
use std::process::Command;

static PROBE_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn check_output(source: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!("almide-interp-gate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Unique file per probe — the two #[test] fns run in parallel threads.
    let seq = PROBE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file = dir.join(format!("probe{seq}.almd"));
    let mut f = std::fs::File::create(&file).unwrap();
    f.write_all(source.as_bytes()).unwrap();
    drop(f);
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["check", file.to_str().unwrap()])
        .output()
        .expect("run almide check");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), combined)
}

#[test]
fn undecidable_interpolation_segments_are_e025() {
    let probes = [
        r#"fn main() -> Unit = { println("${none}") }"#,
        r#"fn main() -> Unit = { println("${ok(1)}") }"#,
        r#"fn main() -> Unit = { println("${some(none)}") }"#,
        r#"fn main() -> Unit = { println("${some(ok(1))}") }"#,
        r#"fn main() -> Unit = { println("${ok(none)}") }"#,
    ];
    for probe in probes {
        let (success, output) = check_output(probe);
        assert!(!success, "expected check to fail for {probe}, got success:\n{output}");
        assert!(
            output.contains("E025"),
            "expected E025 for {probe}, got:\n{output}"
        );
    }
}

#[test]
fn concrete_interpolation_segments_stay_clean() {
    let probes = [
        // Annotated bindings pin every slot — interpolation stays legal
        // (the Result form keeps its existing debug-form warning, which is
        // not an error).
        r#"fn main() -> Unit = { let o: Option[Int] = none
  println("${o}") }"#,
        r#"fn main() -> Unit = { let r: Result[Int, String] = ok(1)
  println("${r}") }"#,
    ];
    for probe in probes {
        let (success, output) = check_output(probe);
        assert!(success, "expected check to pass for {probe}, got:\n{output}");
    }
}
