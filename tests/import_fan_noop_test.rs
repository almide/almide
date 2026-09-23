//! #2541 — `import fan` is a no-op import, exactly like a redundant
//! `import string`: `fan` is always in scope. It used to be a parse error, and
//! a model repeated the line through four repair rounds of that error's hint
//! and lost the task. The issue's completion condition, pinned end to end: a
//! program that starts with `import fan` and uses `fan.timeout` passes
//! `check` and `run` on native and on wasm. `almide fmt` drops the line (it
//! names nothing), so this evidence lives here rather than under the
//! fmt-gated spec/ tree.

use std::path::{Path, PathBuf};
use std::process::Command;

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

/// The Dojo `deadline-guard` shape: `import fan` first, `fan.timeout` used.
const IMPORT_FAN: &str = r#"import fan

fn work(n: Int) -> Int = n * 2

effect fn main() -> Unit = {
  let t = fan.timeout(duration.ms(5000)) { work(21) } ?? -1
  println(int.to_string(t))
}
"#;

fn write(name: &str, src: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-import-fan-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("mkdir");
    let p = d.join(name);
    std::fs::write(&p, src).expect("write");
    p
}

fn almide(args: &[&str]) -> std::process::Output {
    Command::new(almide_bin()).args(args).output().expect("spawn almide")
}

fn assert_ok(o: &std::process::Output, what: &str) {
    assert!(
        o.status.success(),
        "{what} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}

#[test]
fn import_fan_checks_on_both_legs() {
    let src = write("check.almd", IMPORT_FAN);
    let p = src.to_str().unwrap();
    assert_ok(&almide(&["check", p]), "almide check");
    assert_ok(&almide(&["check", p, "--target", "wasm"]), "almide check --target wasm");
}

#[test]
fn import_fan_runs_on_both_legs() {
    let src = write("run.almd", IMPORT_FAN);
    let p = src.to_str().unwrap();
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let mut args = vec!["run", p];
        args.extend_from_slice(extra);
        let o = almide(&args);
        assert_ok(&o, &format!("almide {}", args.join(" ")));
        assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "42", "almide {}", args.join(" "));
    }
}

#[test]
fn fmt_drops_the_no_op_import_and_keeps_its_comments() {
    let src = write(
        "fmt.almd",
        "// header\nimport fan\n// about json\nimport json\n\neffect fn main() -> Unit = {\n  println(json.stringify(value.int(1)))\n}\n",
    );
    assert_ok(&almide(&["fmt", src.to_str().unwrap()]), "almide fmt");
    let out = std::fs::read_to_string(&src).expect("read back");
    assert!(!out.contains("import fan"), "fmt kept the no-op import:\n{out}");
    assert!(out.starts_with("// header\n// about json\nimport json\n"), "fmt moved a comment:\n{out}");
}
