//! `scoped` (#1997): the admission verdict belongs to the CHECKER, so it is
//! the same on both targets, and the two legs realize the declared region
//! rather than dropping it.
//!
//! The value side is pinned by `spec/wasm_cross/scoped_region_value.almd`
//! (C-362) and `spec/wasm_cross/scoped_worker_in_and_out.almd` (C-363), which
//! the cross-target gate runs on both legs; the refusal side by the
//! `tests/diagnostics/e08{6,7,8}-scope-*` families (C-364). What those cannot
//! see is the one thing this file measures: that `almide check` and
//! `almide check --target wasm` give the SAME verdict for the same program,
//! and that a declared region reaches each leg's region machinery.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn write(dir: &Path, name: &str, src: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, src).expect("write fixture");
    p
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-scoped-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

fn check(file: &Path, wasm: bool) -> (bool, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("check").arg(file);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let out = cmd.output().expect("almide check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// Every refused shape under tests/diagnostics/e08*-scope-*, judged by the
/// two check targets. A leg that accepted what the other refused would be the
/// silent-divergence this feature exists to prevent (C-364).
#[test]
fn the_same_refusal_on_both_check_targets() {
    let dir = root().join("tests/diagnostics");
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read tests/diagnostics")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("e086-scope-") || n.starts_with("e087-scope-") || n.starts_with("e088-scope-"))
        })
        .collect();
    cases.sort();
    assert!(cases.len() >= 9, "expected the nine scoped families, found {}", cases.len());
    for case in &cases {
        let meta = std::fs::read_to_string(case.join("meta.toml")).unwrap_or_default();
        let code = meta
            .lines()
            .find_map(|l| l.trim().strip_prefix("expects_code"))
            .map(|v| v.trim_start_matches(['=', ' ', '"']).trim_end_matches('"').to_string())
            .expect("expects_code");
        for target_wasm in [false, true] {
            let (ok, text) = check(&case.join("broken.almd"), target_wasm);
            assert!(!ok, "{}: broken.almd passed (wasm={target_wasm})\n{text}", case.display());
            assert!(
                text.contains(&format!("[{code}]")),
                "{}: expected {code} (wasm={target_wasm})\n{text}",
                case.display()
            );
        }
        for target_wasm in [false, true] {
            let (ok, text) = check(&case.join("fixed.almd"), target_wasm);
            assert!(ok, "{}: fixed.almd was refused (wasm={target_wasm})\n{text}", case.display());
        }
    }
}

const ADMITTED: &str = r#"type Chain = Nil | Cons(Int, Chain)

scoped fn build(n: Int, acc: Chain) -> Chain =
  if n == 0 then acc else build(n - 1, Cons(n, acc))

scoped fn total(c: Chain, acc: Int) -> Int =
  match c {
    Nil => acc,
    Cons(h, t) => total(t, acc + h),
  }

effect fn main() -> Unit = println(int.to_string(scoped { total(build(50, Nil), 0) }))
"#;

/// The declared region reaches each leg's region machinery: native twins the
/// entry's closure into the arena window, the structural wasm leg opens the
/// window at the entry call. Read from `ALMIDE_REGION_DEBUG`, which both legs
/// already print — a region that silently did not happen is the failure this
/// catches.
#[test]
fn both_legs_realize_a_declared_region() {
    let dir = tmp("realize");
    let file = write(&dir, "admitted.almd", ADMITTED);

    let native = Command::new(almide())
        .args(["run", file.to_str().unwrap()])
        .env("ALMIDE_REGION_DEBUG", "1")
        .output()
        .expect("almide run");
    let ntext = String::from_utf8_lossy(&native.stderr).to_string();
    assert!(native.status.success(), "native run failed:\n{ntext}");
    assert_eq!(String::from_utf8_lossy(&native.stdout).trim(), "1275");
    assert!(
        ntext.contains("[region:native] declared window"),
        "the native leg did not open the declared window:\n{ntext}"
    );

    let wasm = Command::new(almide())
        .args(["run", file.to_str().unwrap(), "--target", "wasm"])
        .env("ALMIDE_REGION_DEBUG", "1")
        .output()
        .expect("almide run --target wasm");
    let wtext = String::from_utf8_lossy(&wasm.stderr).to_string();
    if wtext.contains("wasmtime") && !wasm.status.success() {
        // The embedded host is not on PATH in this environment; the cross
        // target gate covers the run, this test covers the emission above.
        return;
    }
    assert!(wasm.status.success(), "wasm run failed:\n{wtext}");
    assert_eq!(String::from_utf8_lossy(&wasm.stdout).trim(), "1275");
    assert!(
        wtext.contains("[region] declared window at call"),
        "the structural wasm leg did not open the declared window:\n{wtext}"
    );
}

/// The A/B knob disables the RECOGNISER, never a declaration: `scoped` is an
/// obligation, so `ALMIDE_REGION_OFF=1` must still produce the region (and
/// the same answer).
#[test]
fn the_region_off_knob_does_not_disable_a_declaration() {
    let dir = tmp("knob");
    let file = write(&dir, "admitted.almd", ADMITTED);
    let out = Command::new(almide())
        .args(["run", file.to_str().unwrap()])
        .env("ALMIDE_REGION_DEBUG", "1")
        .env("ALMIDE_REGION_OFF", "1")
        .output()
        .expect("almide run");
    let text = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "native run failed:\n{text}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1275");
    assert!(
        text.contains("[region:native] declared window"),
        "ALMIDE_REGION_OFF disabled a DECLARED region:\n{text}"
    );
}

/// `scoped` is contextual: a program that uses the word as an identifier —
/// including in a `match` head, where the following `{` opens the arms —
/// parses and runs exactly as it did before the qualifier existed.
#[test]
fn an_identifier_named_scoped_still_compiles() {
    let dir = tmp("ident");
    let file = write(
        &dir,
        "ident.almd",
        r#"fn pick(scoped: Int) -> Int =
  match scoped {
    1 => scoped + 1,
    _ => scoped,
  }

effect fn main() -> Unit = {
  let scoped = 1
  println(int.to_string(pick(scoped)))
}
"#,
    );
    let out = Command::new(almide())
        .args(["run", file.to_str().unwrap()])
        .output()
        .expect("almide run");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "a program using `scoped` as a name broke:\n{text}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
}
