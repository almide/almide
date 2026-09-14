//! #2206 (C-215): the fs error text is ONE table (`almide_base::fs_errno`),
//! rendered by every leg — native's `std::io::Error` `Display`, the embedded
//! host, and the incumbent WAT's static data — so the message a program
//! observes is byte-identical across them. This runs the issue's own probe
//! (a file, a directory, a missing path; read/write/mkdir_p through and onto
//! each) on the three legs the CLI can drive from one binary and differs the
//! outputs; the p3 component lane is `component_p3_test.rs`'s.
//!
//! The EEXIST line is printed TWICE on purpose: the incumbent laid the table's
//! rows over the self-host's newline scratch once, and only the SECOND print
//! of a row showed it (the first fd_write had already stored the `\n`).
use std::path::{Path, PathBuf};
use std::process::Command;

use almide_base::fs_errno::{EEXIST, EISDIR, ENOENT, ENOTDIR};

const PROBE: &str = r#"import fs

effect fn main() -> Unit = {
  println(match fs.read_text("f.txt/x") { ok(v) => "ok(${v})", err(m) => m })
  println(match fs.write("f.txt/y", "z") { ok(_) => "ok", err(m) => m })
  println(match fs.mkdir_p("f.txt") { ok(_) => "ok", err(m) => m })
  println(match fs.mkdir_p("f.txt") { ok(_) => "ok", err(m) => m })
  println(match fs.mkdir_p("adir") { ok(_) => "ok", err(m) => m })
  println(match fs.mkdir_p("f.txt/sub") { ok(_) => "ok", err(m) => m })
  println(match fs.read_text("nope.txt") { ok(v) => "ok(${v})", err(m) => m })
  println(match fs.read_text("adir") { ok(v) => "ok(${v})", err(m) => m })
  println(match fs.write("adir", "z") { ok(_) => "ok", err(m) => m })
}
"#;

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

fn probe_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-fs-errno-legs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("adir")).expect("mkdir");
    std::fs::write(d.join("f.txt"), "hi\n").expect("write");
    std::fs::write(d.join("probe.almd"), PROBE).expect("write");
    d
}

fn run_leg(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> String {
    let mut c = Command::new(almide_bin());
    c.arg("run").arg("probe.almd").args(args).current_dir(dir);
    for (k, v) in env {
        c.env(k, v);
    }
    let o = c.output().expect("spawn almide");
    assert!(o.status.success(), "leg {args:?} {env:?} failed:\n{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

#[test]
fn every_leg_spells_the_table_byte_for_byte() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let dir = probe_dir();
    let expected = [
        format!("fs.read_text(\"f.txt/x\"): {}", ENOTDIR.text),
        format!("fs.write(\"f.txt/y\"): {}", ENOTDIR.text),
        format!("fs.mkdir_p(\"f.txt\"): {}", EEXIST.text),
        format!("fs.mkdir_p(\"f.txt\"): {}", EEXIST.text),
        "ok".to_string(),
        format!("fs.mkdir_p(\"f.txt/sub\"): {}", ENOTDIR.text),
        format!("fs.read_text(\"nope.txt\"): {}", ENOENT.text),
        format!("fs.read_text(\"adir\"): {}", EISDIR.text),
        format!("fs.write(\"adir\"): {}", EISDIR.text),
    ]
    .join("\n")
        + "\n";
    let native = run_leg(&dir, &[], &[]);
    assert_eq!(native, expected, "native is std::io::Error's Display, the table's ground truth");
    let wasm = run_leg(&dir, &["--target", "wasm"], &[]);
    assert_eq!(wasm, expected, "the embedded host leg");
    let incumbent = run_leg(&dir, &["--target", "wasm"], &[("ALMIDE_WASM_INCUMBENT", "1")]);
    assert_eq!(incumbent, expected, "the incumbent WAT leg (static data rendered from the table)");
    let _ = std::fs::remove_dir_all(&dir);
}
