//! `fs.copy(p, q)` where `p` and `q` name the same file leaves it intact.
//!
//! `std::fs::copy` opens its destination truncating before it reads the
//! source, so native and the embedded host emptied the file and reported ok;
//! the stock p1 artifact (its self-hosted copy reads first) left it whole.
//! Found by the ADR-0027 sandbox campaign (`xtarget-fuzz run --focus fs.copy`):
//! `fs.copy("bin.dat", "bin.dat")` left a 0-byte file on native only. The
//! same file under another spelling (`./a.txt`, `sub/../a.txt`) is the same
//! file.
use std::path::{Path, PathBuf};
use std::process::Command;

const PROBE: &str = r#"import fs

effect fn main() -> Unit = {
  println(match fs.copy("a.txt", "a.txt") { ok(_) => "ok", err(m) => m })
  println(match fs.copy("a.txt", "./a.txt") { ok(_) => "ok", err(m) => m })
  println(match fs.copy("sub/../a.txt", "a.txt") { ok(_) => "ok", err(m) => m })
  println(match fs.read_text("a.txt") { ok(v) => v, err(m) => m })
}
"#;

const EXPECTED: &str = "ok\nok\nok\nhello\n\n";

fn probe_dir(leg: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-fs-copy-self-{leg}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("sub")).expect("mkdir");
    std::fs::write(d.join("a.txt"), "hello\n").expect("write");
    std::fs::write(d.join("probe.almd"), PROBE).expect("write");
    d
}

fn run(dir: &Path, args: &[&str]) -> String {
    let o = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg("probe.almd").args(args).current_dir(dir).output().expect("spawn");
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

fn run_stock_p1(dir: &Path) -> Option<String> {
    let wasm = dir.join("probe.wasm");
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", "probe.almd", "--target", "wasm", "-o"])
        .arg(&wasm)
        .current_dir(dir)
        .output()
        .expect("spawn");
    assert!(o.status.success(), "stock build: {}", String::from_utf8_lossy(&o.stderr));
    let o = Command::new("wasmtime")
        .args(["run", "--dir=/"])
        .arg(format!("--env=PWD={}", dir.display()))
        .arg(&wasm)
        .current_dir(dir)
        .output()
        .ok()?;
    assert!(o.status.success(), "stock run: {}", String::from_utf8_lossy(&o.stderr));
    Some(String::from_utf8(o.stdout).expect("utf8"))
}

#[test]
fn a_copy_onto_the_same_file_leaves_it_intact_on_every_leg() {
    let native = probe_dir("native");
    assert_eq!(run(&native, &[]), EXPECTED, "native");
    let embedded = probe_dir("embedded");
    assert_eq!(run(&embedded, &["--target", "wasm"]), EXPECTED, "the embedded host");
    let stock = probe_dir("stock");
    if let Some(out) = run_stock_p1(&stock) {
        assert_eq!(out, EXPECTED, "the stock-p1 artifact");
    }
    for d in [native, embedded, stock] {
        let _ = std::fs::remove_dir_all(d);
    }
}
