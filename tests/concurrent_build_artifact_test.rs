//! #4269: `almide build` printed `Built` and exited 0 while another almide
//! process was building, and the binary it installed was not the program it
//! was asked to build (a 420 KB binary that printed nothing for any input).
//!
//! The promise under test: a build that reports success installed THIS
//! program. Distinct programs are built at once — package builds (the cargo
//! route, which shares one scratch crate) and single-file builds (the rlib
//! fast route, which shares one generated source) — beside `almide test
//! --target native` runs, round after round, and every installed binary must
//! print its own program's line. A build may fail under contention (it must
//! then say so); it may never succeed with someone else's artifact.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    env!("CARGO_BIN_EXE_almide").to_string()
}

fn fresh_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-4269-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    root
}

/// A program whose output names it, distinct in its code as well as its text.
fn program(n: usize) -> String {
    format!(
        "fn weight(x: Int) -> Int = x * {m} + {n}\n\neffect fn main() -> Unit = {{\n  println(\"program {n} ${{weight(3)}}\")\n}}\n",
        m = n + 2
    )
}

fn expected(n: usize) -> String {
    format!("program {n} {}\n", 3 * (n as i64 + 2) + n as i64)
}

/// A package with `src/main.almd` = program `n` (the cargo route).
fn package(root: &Path, n: usize) -> PathBuf {
    let dir = root.join(format!("pkg{n}"));
    std::fs::create_dir_all(dir.join("src")).expect("pkg dir");
    std::fs::write(dir.join("almide.toml"), format!("[package]\nname = \"pkg{n}\"\nversion = \"0.1.0\"\n")).expect("manifest");
    std::fs::write(dir.join("src").join("main.almd"), program(n)).expect("main");
    dir.join("src").join("main.almd")
}

/// A lone file = program `n` (the rlib fast route).
fn single(root: &Path, n: usize) -> PathBuf {
    let dir = root.join(format!("one{n}"));
    std::fs::create_dir_all(&dir).expect("single dir");
    let f = dir.join("main.almd");
    std::fs::write(&f, program(n)).expect("single");
    f
}

/// A directory of native test files for the concurrent `almide test` runs.
fn test_dir(root: &Path, k: usize) -> PathBuf {
    let dir = root.join(format!("tests{k}"));
    std::fs::create_dir_all(&dir).expect("tests dir");
    for i in 0..3 {
        std::fs::write(
            dir.join(format!("t{i}_test.almd")),
            format!("test \"t{k}_{i}\" {{\n  assert_eq({i} + {k}, {s})\n}}\n", s = i + k),
        )
        .expect("test file");
    }
    dir
}

#[test]
fn a_build_that_reports_success_installed_its_own_program_under_concurrent_builds_and_tests() {
    let root = fresh_root("race");
    let mut wrong = Vec::new();
    let mut built = 0;
    for round in 0..3 {
        let mut handles = Vec::new();
        for n in 0..6 {
            let id = round * 10 + n;
            let src = if n % 2 == 0 { package(&root, id) } else { single(&root, id) };
            let out = root.join(format!("bin{id}"));
            handles.push(std::thread::spawn(move || {
                let o = Command::new(almide_bin())
                    .arg("build")
                    .arg(&src)
                    .arg("-o")
                    .arg(&out)
                    .output()
                    .expect("spawn almide build");
                (id, o.status.success(), String::from_utf8_lossy(&o.stderr).into_owned(), out)
            }));
        }
        let testers: Vec<_> = (0..2)
            .map(|k| {
                let dir = test_dir(&root, round * 10 + k);
                std::thread::spawn(move || {
                    Command::new(almide_bin())
                        .args(["test", "--target", "native"])
                        .arg(&dir)
                        .output()
                        .expect("spawn almide test")
                })
            })
            .collect();
        for h in handles {
            let (id, ok, stderr, out) = h.join().expect("build thread");
            if !ok {
                // A refused build is honest; only a success is checked.
                continue;
            }
            built += 1;
            let run = Command::new(&out).output().expect("run built binary");
            let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
            if !run.status.success() || stdout != expected(id) {
                wrong.push(format!(
                    "program {id}: build said ok ({}) but the binary printed {stdout:?} (exit {:?})",
                    stderr.trim(),
                    run.status.code()
                ));
            }
        }
        for t in testers {
            let o = t.join().expect("test thread");
            assert!(o.status.success(), "a concurrent almide test failed:\n{}", String::from_utf8_lossy(&o.stderr));
        }
    }
    let _ = std::fs::remove_dir_all(&root);
    assert!(wrong.is_empty(), "{} build(s) reported success with the wrong artifact:\n{}", wrong.len(), wrong.join("\n"));
    assert!(built > 0, "no build succeeded at all");
}
