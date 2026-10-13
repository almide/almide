//! `fs.fold_lines_range` with a callback held in a `let` (#3668).
//!
//! The native runtime takes the callback in a raw generic `F: Fn` slot, so a
//! closure literal stays a static call. A closure VALUE (a let-bound lambda, a
//! let-bound fn item, a capturing lambda in a `let`, a fn parameter) is an
//! `Rc<dyn Fn>` on the native leg, which does not implement `Fn`: rustc E0277
//! while wasm ran the program. Both legs must print the same bytes and exit 0. The literal call is
//! pinned beside them, and the emitted Rust must keep it un-boxed.
//!
//! The program makes its own temp dir, so it runs on both legs unchanged. Every
//! subprocess is capped, so a hang fails by name instead of stalling the suite.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn tool_available(tool: &str) -> bool {
    let found = Command::new(tool).arg("--version").output().is_ok();
    if !found && std::env::var("ALMIDE_EXPECT_TOOLS").is_ok() {
        panic!("{tool} is required (ALMIDE_EXPECT_TOOLS is set) but was not found");
    }
    found
}

const PROGRAM: &str = r#"import fs

fn longest(acc: Int, line: String) -> Int = if string.len(line) > acc then string.len(line) else acc

// A callback reaching the slot as a fn parameter, used twice and called directly.
effect fn through_param(p: String, step: (Int, String) -> Int) -> Int = {
  let a = fs.fold_lines_range(p, 0, 100, 0, step)!
  let b = fs.fold_lines_range(p, 0, 3, 0, step)!
  a + b + step(0, "zz")
}

effect fn main() -> Unit = {
  let d = fs.create_temp_dir("fold_range_let_fn")!
  let p = d + "/lines.txt"
  fs.write(p, "a\nbb\nccc\ndddd\neeeee\n")!
  let f = (acc: Int, line: String) => acc + string.len(line)
  println("int-var ${fs.fold_lines_range(p, 0, 9, 0, f)!}")
  let g = longest
  println("fnref-var ${fs.fold_lines_range(p, 0, 100, 0, g)!}")
  let sep = "|"
  let h = (acc: String, line: String) => acc + sep + line
  println("capture-var ${fs.fold_lines_range(p, 3, 100, "", h)!}")
  let k = (acc: List[String], line: String) => acc + [line]
  println("list-var ${fs.fold_lines_range(p, 0, 100, [], k)!}")
  println("reuse ${fs.fold_lines_range(p, 0, 2, 0, f)!} ${f(1, "xy")}")
  println("param ${through_param(p, f)!}")
  println("literal ${fs.fold_lines_range(p, 0, 100, 0, (acc, line) => acc * 2 + string.len(line))!}")
  fs.remove_all(d)!
}
"#;

const EXPECTED: &str = r#"int-var 6
fnref-var 5
capture-var |ccc|dddd|eeeee
list-var ["a", "bb", "ccc", "dddd", "eeeee"]
reuse 1 3
param 20
literal 57
"#;

/// Drain a child pipe on its own thread, so a large emit cannot fill the pipe
/// and stall the child while the poll loop below waits for it to exit.
fn drain<R: std::io::Read + Send + 'static>(mut r: R) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// Run the almide CLI with `args`, killed after 180 s so a hang fails here.
fn capped(args: &[&str], what: &str) -> (String, String, i32) {
    let mut child = Command::new(almide_bin())
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn almide");
    let out = drain(child.stdout.take().expect("stdout"));
    let err = drain(child.stderr.take().expect("stderr"));
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().expect("wait") {
            break s;
        }
        if start.elapsed() > Duration::from_secs(180) {
            let _ = child.kill();
            panic!("{what}: still running after 180 s");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    (out.join().expect("stdout"), err.join().expect("stderr"), status.code().unwrap_or(-1))
}

fn write_program(tag: &str) -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("almide-3668-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let almd = dir.join("main.almd");
    std::fs::write(&almd, PROGRAM).expect("write source");
    (dir, almd.to_string_lossy().into_owned())
}

#[test]
fn let_bound_range_fold_callback_matches_on_both_legs() {
    if !tool_available("rustc") || !tool_available("wasmtime") {
        return;
    }
    for target in ["rust", "wasm"] {
        let (dir, almd) = write_program(target);
        let (out, err, code) = capped(&["run", &almd, "--target", target], &format!("[{target}] almide run"));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(code, 0, "[{target}] exit code (stderr: {err})");
        assert_eq!(out, EXPECTED, "[{target}] stdout (stderr: {err})");
    }
}

#[test]
fn the_literal_callback_stays_unboxed() {
    if !tool_available("rustc") {
        return;
    }
    let (dir, almd) = write_program("emit");
    let (rust, err, code) = capped(&[&almd, "--target", "rust"], "almide emit");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 0, "emit failed: {err}");
    let calls: Vec<&str> = rust.lines().filter(|l| l.contains("almide_rt_fs_fold_lines_range(")).collect();
    assert_eq!(calls.len(), 8, "fold_lines_range call sites: {calls:#?}");
    let literal: Vec<&&str> = calls.iter().filter(|l| l.contains("literal {}")).collect();
    assert_eq!(literal.len(), 1, "the literal call site: {calls:#?}");
    assert!(!literal[0].contains("Rc::new"), "the literal callback is boxed: {}", literal[0]);
}
