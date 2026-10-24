//! #4113: `fs.create_temp_file` / `fs.create_temp_dir` on every leg name the
//! entry `<prefix><16 hex digits of OS entropy>` and create it exclusively;
//! on native and the embedded wasm host the file is 0o600 and the directory
//! 0o700 (WASI preview1, the stock artifact's interface, sets no mode). The
//! pre-#4113 names were the prefix plus the clock's nanoseconds, created with
//! a truncating write and `create_dir_all`; tests/../crates/almide-rt-core
//! tests/fs_temp_create.rs holds the planted-symlink case.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const PROBE: &str = r#"import fs

effect fn main() -> Unit = {
  let a = fs.create_temp_file("almtmp")!
  let b = fs.create_temp_file("almtmp")!
  let d = fs.create_temp_dir("almtmpd")!
  println(a)
  println(b)
  println(d)
  println("${fs.is_file(a)} ${fs.is_file(b)} ${fs.is_dir(d)} ${a != b}")
}
"#;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(leg: &str, out: &str, modes: bool) {
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "{leg}: {out}");
    assert_eq!(lines[3], "true true true true", "{leg}: {out}");
    for (p, prefix, mode) in [(lines[0], "almtmp", 0o600), (lines[1], "almtmp", 0o600), (lines[2], "almtmpd", 0o700)] {
        let name = std::path::Path::new(p).file_name().unwrap().to_str().unwrap();
        let rand = name.strip_prefix(prefix).unwrap_or_else(|| panic!("{leg}: {name}"));
        assert!(rand.len() == 16 && rand.chars().all(|c| c.is_ascii_hexdigit()), "{leg}: not an entropy name: {name}");
        if modes {
            assert_eq!(std::fs::metadata(p).unwrap().permissions().mode() & 0o777, mode, "{leg}: {p}");
        }
    }
    let _ = std::fs::remove_file(lines[0]);
    let _ = std::fs::remove_file(lines[1]);
    let _ = std::fs::remove_dir(lines[2]);
}

#[test]
fn temp_entries_are_exclusive_entropy_named_and_private_on_every_leg() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path();
    std::fs::write(dir.join("probe.almd"), PROBE).unwrap();
    for (leg, args, modes) in [("native", &[][..], true), ("embedded wasm", &["--target", "wasm"][..], true)] {
        let o = Command::new(almide()).arg("run").arg("probe.almd").args(args).current_dir(dir).output().unwrap();
        assert!(o.status.success(), "{leg}: {}", String::from_utf8_lossy(&o.stderr));
        check(leg, &String::from_utf8_lossy(&o.stdout), modes);
    }
    let wasm = dir.join("probe.wasm");
    let o = Command::new(almide()).args(["build", "probe.almd", "--target", "wasm", "-o"]).arg(&wasm).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "stock build: {}", String::from_utf8_lossy(&o.stderr));
    let tmp = std::env::temp_dir();
    let Ok(o) = Command::new("wasmtime")
        .args(["run", "--dir=/"])
        .arg(format!("--env=PWD={}", dir.display()))
        .arg(format!("--env=TMPDIR={}", tmp.display()))
        .arg(&wasm)
        .current_dir(dir)
        .output()
    else {
        eprintln!("SKIP stock leg: no wasmtime CLI on PATH");
        return;
    };
    assert!(o.status.success(), "stock run: {}", String::from_utf8_lossy(&o.stderr));
    check("stock artifact", &String::from_utf8_lossy(&o.stdout), false);
}
