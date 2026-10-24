//! #4110: a native build refuses a rustc below `MIN_RUSTC`
//! (src/cli/rustc_floor.rs). An old toolchain re-opens fixed std
//! vulnerabilities in every binary it builds (CVE-2024-24576: Windows
//! `Command` + .bat/.cmd argument injection, fixed in 1.77.2).
//!
//! A fake `rustc` first on PATH answers `--version` with a chosen version and
//! hands every other invocation to the real rustc. Reporting 1.77.1 (and one
//! patch below the floor) the build is refused before anything compiles;
//! reporting the floor itself the build runs to a working binary. The
//! control is the accepted case's twin: the refusal comes from the version
//! alone, not from the stub.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `MIN_RUSTC` read from its one definition.
fn floor() -> (u32, u32, u32) {
    let src = std::fs::read_to_string(root().join("src/cli/rustc_floor.rs")).unwrap();
    let line = src.lines().find(|l| l.starts_with("pub(crate) const MIN_RUSTC")).expect("MIN_RUSTC");
    let tuple = line.split('=').nth(1).unwrap().trim().trim_end_matches(';').trim_matches(|c| c == '(' || c == ')');
    let v: Vec<u32> = tuple.split(',').map(|p| p.trim().parse().unwrap()).collect();
    (v[0], v[1], v[2])
}

fn dotted((a, b, c): (u32, u32, u32)) -> String {
    format!("{a}.{b}.{c}")
}

#[test]
fn the_floor_includes_the_cve_fix_and_the_docs_and_ci_agree_with_it() {
    let f = floor();
    assert!(f >= (1, 77, 2), "MIN_RUSTC {} is below 1.77.2 (CVE-2024-24576)", dotted(f));
    for doc in ["docs/specs/cli.md", "proofs/TRUSTED_BASE.md"] {
        let text = std::fs::read_to_string(root().join(doc)).unwrap();
        assert!(text.contains(&format!("rustc {}", dotted(f))), "{doc} does not state the rustc floor {} (MIN_RUSTC)", dotted(f));
    }
    // The floor is never above what CI verifies the generated code with.
    let ci = std::fs::read_to_string(root().join(".github/workflows/ci.yml")).unwrap();
    let pin = ci.lines().find_map(|l| l.trim().strip_prefix("RUST_TOOLCHAIN: \"")).and_then(|r| r.split('"').next()).expect("RUST_TOOLCHAIN in ci.yml");
    let p: Vec<u32> = pin.split('.').map(|x| x.parse().unwrap()).collect();
    assert!((p[0], p[1], p[2]) >= f, "CI pins {pin}, below MIN_RUSTC {}", dotted(f));
}

#[cfg(unix)]
fn fake_rustc(dir: &Path, version: &str) {
    use std::os::unix::fs::PermissionsExt;
    let real = String::from_utf8(Command::new("sh").args(["-c", "command -v rustc"]).output().unwrap().stdout).unwrap();
    let real = real.trim();
    assert!(!real.is_empty(), "no rustc on PATH to hand the real compiles to");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ] && [ $# -eq 1 ]; then echo 'rustc {version} (0000000 2020-01-01)'; exit 0; fi\nexec '{real}' \"$@\"\n"
    );
    let p = dir.join("rustc");
    std::fs::write(&p, script).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn build_with(version: &str) -> (bool, String, PathBuf, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    fake_rustc(&bin, version);
    // A program no build cache has seen, so the build really compiles.
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let src = tmp.path().join("p.almd");
    std::fs::write(&src, format!("effect fn main() -> Unit = {{\n  println(\"floor {nonce}\")\n}}\n")).unwrap();
    let out = tmp.path().join("p");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", src.to_str().unwrap(), "-o", out.to_str().unwrap()])
        .env("PATH", path)
        .env_remove("RUSTC")
        .current_dir(tmp.path())
        .output()
        .unwrap();
    (o.status.success(), String::from_utf8_lossy(&o.stderr).into_owned(), out, tmp)
}

#[cfg(unix)]
#[test]
fn a_rustc_below_the_floor_is_refused_and_the_floor_itself_builds() {
    let f = floor();
    let below = if f.2 > 0 { (f.0, f.1, f.2 - 1) } else { (f.0, f.1 - 1, 99) };
    for old in ["1.77.1".to_string(), dotted(below)] {
        let (ok, err, _, _t) = build_with(&old);
        assert!(!ok, "a native build with rustc {old} was accepted");
        assert!(err.contains(&format!("rustc {old} is too old")), "no refusal naming {old}:\n{err}");
        assert!(err.contains(&dotted(f)) && err.contains("rustup update"), "the refusal does not name the floor and the fix:\n{err}");
    }
    let (ok, err, out, _t) = build_with(&dotted(f));
    assert!(!err.contains("too old"), "rustc {} (the floor) was refused:\n{err}", dotted(f));
    assert!(ok, "the build with rustc {} failed:\n{err}", dotted(f));
    let run = Command::new(&out).output().unwrap();
    assert!(String::from_utf8_lossy(&run.stdout).starts_with("floor "), "the accepted build does not run");
}
