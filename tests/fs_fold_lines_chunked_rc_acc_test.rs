//! `fs.fold_lines_chunked[A]` with an accumulator or callback that holds an
//! `Rc` on the native leg (#3649, C-220).
//!
//! The stdlib signature bounds nothing, so the checker admits a Matrix or Bytes
//! accumulator, a callback that captures a Bytes, and a callback held in a
//! `let` (a closure value). Native ran each chunk on a scoped thread under
//! `A: Send` and `F: Send + Sync`, so all four were rustc E0277 while wasm ran
//! them. C-220's observable is the partials, one per chunk in chunk order, and
//! they must not depend on whether the chunks ran on threads. The List, Map,
//! Value and Int shapes keep the threaded form and are pinned beside them.
//!
//! A differential test rather than a spec/wasm_cross fixture: the reference
//! interpreters every wasm_cross fixture also runs through cannot evaluate a
//! chunked fold over these accumulators, and their skip counts are shrink-only.
//! The program makes its own temp dir, so it runs on both legs unchanged.

use std::process::Command;

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

fn grow(m: Matrix, l: String) -> Matrix = matrix.from_lists([[matrix.get(m, 0, 0) + float.from_int(string.len(l))]])

effect fn main() -> Unit = {
  let d = fs.create_temp_dir("chunked_rc_acc")!
  let p = d + "/lines.txt"
  fs.write_text(p, "a\nbb\nccc\ndddd\neeeee\n")!
  let ms = fs.fold_lines_chunked(p, 2, matrix.from_lists([[0.5]]), grow)!
  println("matrix ${ms |> list.map((m) => matrix.get(m, 0, 0))}")
  let ms_id = fs.fold_lines_chunked(p, 3, matrix.from_lists([[1.0]]), (m, l) => m)!
  println("matrix-id ${ms_id |> list.map((m) => matrix.get(m, 0, 0))}")
  let bs = fs.fold_lines_chunked(p, 3, bytes.zeros(0), (b, l) => bytes.concat(b, bytes.from_string(l)))!
  println("bytes ${bs |> list.map((b) => bytes.to_string(b) ?? "?")}")
  let k = bytes.from_string("kk")
  let cs = fs.fold_lines_chunked(p, 2, 0, (n, l) => n + bytes.len(k) + string.len(l))!
  println("capture ${cs}")
  let f = (n: Int, l: String) => n * 10 + string.len(l)
  let vs = fs.fold_lines_chunked(p, 2, 0, f)!
  println("closure-var ${vs}")
  let ls = fs.fold_lines_chunked(p, 2, [], (acc, l) => acc + [l])!
  println("list ${ls}")
  let mp = fs.fold_lines_chunked(p, 2, map.new(), (acc, l) => map.set(acc, l, string.len(l)))!
  println("map ${mp |> list.map((m) => map.len(m))}")
  let vv = fs.fold_lines_chunked(p, 2, value.int(0), (acc, l) => value.int((value.as_int(acc) ?? 0) + string.len(l)))!
  println("value ${vv |> list.map((v) => value.stringify(v))}")
  let ns = fs.fold_lines_chunked(p, 2, 0, (n, l) => n + string.len(l))!
  println("int ${ns}")
  let e = d + "/empty.txt"
  fs.write_text(e, "")!
  let parts = fs.fold_lines_chunked(e, 4, bytes.zeros(1), (b, l) => b)!
  println("empty ${list.len(parts)}")
  fs.remove_all(d)!
}
"#;

const EXPECTED: &str = r#"matrix [10.5, 5.5]
matrix-id [1, 1, 1]
bytes ["abbccc", "dddd", "eeeee"]
capture [18, 7]
closure-var [1234, 5]
list [["a", "bb", "ccc", "dddd"], ["eeeee"]]
map [4, 1]
value ["10", "5"]
int [10, 5]
empty 0
"#;

/// (stdout, stderr, exit code) of `almide run` on one target.
fn run(target: &str) -> (String, String, i32) {
    let dir = std::env::temp_dir().join(format!("almide-3649-{}-{target}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let almd = dir.join("main.almd");
    std::fs::write(&almd, PROGRAM).expect("write source");
    let out = Command::new(almide_bin())
        .arg("run")
        .arg(&almd)
        .args(["--target", target])
        .output()
        .expect("spawn almide run");
    let _ = std::fs::remove_dir_all(&dir);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn chunked_fold_over_rc_accumulators_matches_on_both_legs() {
    if !tool_available("rustc") || !tool_available("wasmtime") {
        return;
    }
    for target in ["rust", "wasm"] {
        let (out, err, code) = run(target);
        assert_eq!(code, 0, "[{target}] exit code (stderr: {err})");
        assert_eq!(out, EXPECTED, "[{target}] stdout (stderr: {err})");
    }
}

#[test]
fn only_the_rc_holding_calls_leave_the_threaded_form() {
    if !tool_available("rustc") {
        return;
    }
    let dir = std::env::temp_dir().join(format!("almide-3649-{}-emit", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let almd = dir.join("main.almd");
    std::fs::write(&almd, PROGRAM).expect("write source");
    let out = Command::new(almide_bin()).arg(&almd).args(["--target", "rust"]).output().expect("spawn almide");
    let _ = std::fs::remove_dir_all(&dir);
    let rust = String::from_utf8_lossy(&out.stdout);
    let seq = rust.matches("almide_rt_fs_fold_lines_chunked_seq(").count();
    let threaded = rust.matches("almide_rt_fs_fold_lines_chunked(").count();
    // Matrix ×2, Bytes ×2 (the empty-file call included), the Bytes capture,
    // the closure value: sequential. List, Map, Value, Int: threaded.
    assert_eq!((seq, threaded), (6, 4), "sequential vs threaded call sites");
}
