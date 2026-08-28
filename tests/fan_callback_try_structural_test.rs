//! #1663 — the fan callback's top-level `!` instantiates the fallible
//! form (ADR-0006): an UN-banged `fan.map` whose callback errs answers
//! the err as a VALUE, never an abort. Native and interp agree; the
//! structural wasm leg was fixed by `strip_callback_try` (the inlined
//! callback's Try used to lower as enclosing-frame propagation and
//! aborted main). Pinned here on the forced-structural leg; the
//! incumbent leg still diverges (#1663 stays open for it), so the
//! spec/wasm_cross fixture cannot carry this shape yet.

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

fn dir() -> PathBuf {
    let d = std::env::temp_dir().join("almide-fan-cb-try");
    std::fs::create_dir_all(&d).expect("mkdir");
    d
}

const UNBANGED: &str = r#"import fs

effect fn main() -> Unit = {
  let r = fan.map(["present.txt", "gone.txt"], (p) => fs.read_text(p)!)
  match r {
    ok(_)  => println("ok-arm"),
    err(m) => println("err=${m}"),
  }
  println("after")
}
"#;

fn run(leg_env: Option<(&str, &str)>, args: &[&str], cwd: &Path) -> (String, String, i32) {
    let mut c = Command::new(almide_bin());
    c.args(args).current_dir(cwd);
    if let Some((k, v)) = leg_env {
        c.env(k, v);
    }
    let out = c.output().expect("spawn almide");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn unbanged_fallible_fan_map_err_is_a_value_on_the_structural_leg() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir();
    std::fs::write(d.join("present.txt"), "hello").expect("write");
    let src = d.join("unbanged.almd");
    std::fs::write(&src, UNBANGED).expect("write");
    let want = "err=No such file or directory (os error 2)\nafter\n";

    let (nout, nerr, ncode) = run(None, &["run", "unbanged.almd"], &d);
    assert_eq!(nout, want, "native diverged from the contracted value form (stderr: {nerr})");
    assert_eq!(ncode, 0);

    let (wout, werr, wcode) = run(
        Some(("ALMIDE_WASM_STRUCTURAL", "1")),
        &["run", "unbanged.almd", "--target", "wasm"],
        &d,
    );
    assert_eq!(
        wout, want,
        "the structural leg must answer the err as a value, not abort (stderr: {werr})"
    );
    assert_eq!(wcode, 0, "no abort: the un-banged form carries the err as a value");
}
