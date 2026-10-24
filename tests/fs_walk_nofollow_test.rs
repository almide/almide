//! #4143: `fs.walk` and `fs.glob` never descend into a symlinked
//! directory; the symlink is an entry like any other. A link to the outside
//! therefore lists nothing outside the walked directory, and a link loop
//! cannot repeat the walk.
//!
//! The stdlib cannot make a symlink, so the tree is built here:
//! `root/top.txt`, `root/sub/a.txt`, `root/sub/link -> ../../outside` (which
//! holds `secret.txt`) and `root/sub/loop -> ..`. Native, the embedded wasm
//! host and the stock artifact (`almide build --target wasm` under wasmtime)
//! print the same lines. Negative control: the pre-#4143 walk descended on
//! `is_dir()`, which follows links, and answered 164 entries for this tree,
//! `root/sub/link/secret.txt` among them.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;

const PROBE: &str = r#"import fs

effect fn main() -> Unit = {
  let w = fs.walk("root")!
  println("walk ${list.len(w)}: ${list.join(w, ",")}")
  let g = fs.glob("root/**")!
  println("glob ** ${list.len(g)}: ${list.join(g, ",")}")
  let s = fs.glob("root/*/*")!
  println("glob */* ${list.len(s)}: ${list.join(s, ",")}")
  let t = fs.glob("root/**/*.txt")!
  println("glob txt ${list.len(t)}: ${list.join(t, ",")}")
}
"#;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run(dir: &Path, args: &[&str]) -> String {
    let o = Command::new(almide()).arg("run").arg("probe.almd").args(args).current_dir(dir).output().expect("spawn almide");
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

#[test]
fn walk_and_glob_list_a_symlinked_directory_as_an_entry_on_every_leg() {
    let t = tempfile::tempdir().expect("scratch");
    let base = std::fs::canonicalize(t.path()).expect("canonical");
    let dir = base.join("proj");
    std::fs::create_dir_all(dir.join("root/sub")).unwrap();
    std::fs::create_dir_all(base.join("outside")).unwrap();
    std::fs::write(base.join("outside/secret.txt"), "secret").unwrap();
    std::fs::write(dir.join("root/top.txt"), "t").unwrap();
    std::fs::write(dir.join("root/sub/a.txt"), "a").unwrap();
    std::os::unix::fs::symlink("../../outside", dir.join("root/sub/link")).unwrap();
    std::os::unix::fs::symlink("..", dir.join("root/sub/loop")).unwrap();
    std::fs::write(dir.join("probe.almd"), PROBE).unwrap();

    let expected = [
        "walk 5: root/sub,root/sub/a.txt,root/sub/link,root/sub/loop,root/top.txt",
        "glob ** 6: root,root/sub,root/sub/a.txt,root/sub/link,root/sub/loop,root/top.txt",
        "glob */* 3: root/sub/a.txt,root/sub/link,root/sub/loop",
        "glob txt 2: root/sub/a.txt,root/top.txt",
    ]
    .join("\n")
        + "\n";
    let native = run(&dir, &[]);
    assert_eq!(native, expected, "native");
    assert_eq!(run(&dir, &["--target", "wasm"]), expected, "the embedded wasm host");

    let wasm = dir.join("probe.wasm");
    let o = Command::new(almide()).args(["build", "probe.almd", "--target", "wasm", "-o"]).arg(&wasm).current_dir(&dir).output().expect("spawn");
    assert!(o.status.success(), "stock build: {}", String::from_utf8_lossy(&o.stderr));
    let Ok(o) = Command::new("wasmtime").args(["run", "--dir=/"]).arg(format!("--env=PWD={}", dir.display())).arg(&wasm).current_dir(&dir).output() else {
        eprintln!("SKIP stock leg: no wasmtime CLI on PATH");
        return;
    };
    assert!(o.status.success(), "stock run: {}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(String::from_utf8_lossy(&o.stdout), expected, "the stock artifact");
}
