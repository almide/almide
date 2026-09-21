//! The domain-edge matrix must not skip a signature SILENTLY (#2402).
//!
//! `tools/domain_edge_matrix.py` skips a slot whose parameter or return type
//! it cannot synthesize, and used to print only the COUNT. For `json` that
//! count was 12, nobody read it back, and it held `json.index(path: JsonPath,
//! i: Int)` — the signature that carried #2396 (a wasm-only i32 truncation of
//! the path index). The skipped population is now named in the tool's coverage
//! line, written to `proofs/domain-edges.toml` as `[[skip]]` rows, and
//! `scripts/check-domain-edges.sh` diffs it both ways: a skip that APPEARS is
//! coverage lost and fails; a declared skip that becomes buildable must have
//! its row deleted.
//!
//! This is the negative test for that direction. It forges a copy of
//! `stdlib/json.almd` in which the PUBLIC `json.index` takes a parameter type
//! the tables do not know (`Widget`), points the tool at the forged directory,
//! and asserts that (1) the coverage line names `json.index(i)` with the
//! reason, and (2) the gate exits non-zero naming it as an UNDECLARED skip.
//! The forged fn keeps a public name on purpose: a self-host helper would be
//! attributed to the "outside the public surface" list rather than a skip, and
//! the failure this guards against is a type leaving the tables under a
//! signature the matrix is supposed to measure.
//!
//! The positive control runs the same steps on the real `stdlib/` and expects
//! the gate to pass with `json.index(i)` exercised — so a broken harness cannot
//! report a vacuous red.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn almide_bin() -> Option<String> {
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| {
        let release = repo().join("target/release/almide");
        if release.exists() {
            release.to_string_lossy().into_owned()
        } else {
            "almide".to_string()
        }
    });
    Command::new(&bin)
        .arg("--version")
        .output()
        .ok()
        .map(|_| bin)
}

fn python_available() -> bool {
    Command::new("python3").arg("--version").output().is_ok()
}

/// Run the tool in `--skips-only` mode over `stdlib_dir`, scoped to json.
fn measure(bin: &str, stdlib_dir: &Path, out: &Path) -> String {
    let p = Command::new("python3")
        .arg(repo().join("tools/domain_edge_matrix.py"))
        .args(["--skips-only", "--only", "json", "--almide", bin])
        .arg("--stdlib")
        .arg(stdlib_dir)
        .arg("--json")
        .arg(out)
        .env(
            "PATH",
            format!(
                "/opt/homebrew/bin:{}",
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .expect("python3 tools/domain_edge_matrix.py");
    let stdout = String::from_utf8_lossy(&p.stdout).into_owned();
    assert!(
        p.status.success(),
        "tool failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&p.stderr)
    );
    stdout
}

/// Diff a measured JSON against the committed ledger, scoped to json.
fn gate(measured: &Path) -> (bool, String) {
    let p = Command::new("bash")
        .arg(repo().join("scripts/check-domain-edges.sh"))
        .args(["--only", "json", "--measured"])
        .arg(measured)
        .output()
        .expect("bash scripts/check-domain-edges.sh");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&p.stdout),
        String::from_utf8_lossy(&p.stderr)
    );
    (p.status.success(), text)
}

#[test]
fn a_forged_unbuildable_public_signature_is_named_and_fails_the_gate() {
    let (Some(bin), true) = (almide_bin(), python_available()) else {
        eprintln!("skip: almide binary or python3 unavailable");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("domain-edge-forged-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    // Forge: the PUBLIC `json.index` now takes a type the tables cannot build.
    let src = std::fs::read_to_string(repo().join("stdlib/json.almd")).unwrap();
    let needle = "fn index(path: JsonPath, i: Int) -> JsonPath = _";
    assert!(
        src.contains(needle),
        "stdlib/json.almd no longer carries `{needle}`; update the forge"
    );
    let forged = src.replace(needle, "fn index(path: Widget, i: Int) -> JsonPath = _");
    std::fs::write(tmp.join("json.almd"), forged).unwrap();

    let out = tmp.join("measured.json");
    let summary = measure(&bin, &tmp, &out);
    assert!(
        summary.contains("coverage: 0 of 1 signatures exercised, skipped: [json.index(i): unbuildable: no VALUES entry for parameter type `Widget`]"),
        "the coverage line must NAME the forged skip and its reason:\n{summary}"
    );

    let (ok, text) = gate(&out);
    assert!(!ok, "the gate must fail on a skip that appeared:\n{text}");
    assert!(
        text.contains("UNDECLARED skipped slot") && text.contains("json.index(i): unbuildable"),
        "the gate must name the undeclared skip:\n{text}"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn the_real_json_surface_is_fully_exercised_and_the_gate_passes() {
    let (Some(bin), true) = (almide_bin(), python_available()) else {
        eprintln!("skip: almide binary or python3 unavailable");
        return;
    };
    let out = std::env::temp_dir().join(format!("domain-edge-real-{}.json", std::process::id()));
    let summary = measure(&bin, &repo().join("stdlib"), &out);
    assert!(
        summary.contains("coverage: 1 of 1 signatures exercised, skipped: []"),
        "json's one public Int slot (`json.index(i)`) must be exercised, not skipped:\n{summary}"
    );
    let (ok, text) = gate(&out);
    assert!(
        ok,
        "positive control: the committed ledger must pass:\n{text}"
    );
    let _ = std::fs::remove_file(&out);
}
