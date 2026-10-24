//! #3661 — native: a param rebound into a `var` before a self tail call
//! MOVES into it when that rebind is the param's last read.
//!
//! `fn go(s, x) = … else { var a = s; list.push(a, x); go(a, x + 1) }`
//! rendered `let mut a = s.clone();` inside the TCO loop: the loop rewrite
//! moved a param's final read only when it sat in the tail call's
//! arguments, so the rebind in a statement before the call copied the
//! accumulator on every iteration (O(n²)). The rebind is that param's only
//! read in the block, and the call reassigns it, so it is the final read
//! and moves (rustc re-proves every move: a wrong one is a loud E0382).
//!
//! The negative cells keep the copy where the param is read again after the
//! rebind, and the differential runs the aliasing program on both legs and
//! byte-compares their stdout with native's.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almd_dying_rebind_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The v3 Rust the native build falls back to for these programs (the v1
/// render walls on a heap self tail call, #3550).
fn emit_rust(name: &str, src: &str) -> String {
    let file = scratch(name).join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(almide())
        .args([file.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("almide emit");
    assert!(out.status.success(), "emit failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The body of `fn go` in the emitted Rust.
fn go_body(rust: &str) -> &str {
    let start = rust.find("pub fn go(").expect("fn go is emitted");
    let rest = &rust[start..];
    &rest[..rest.find("\n}\n").expect("fn go ends")]
}

fn tail(ty: &str, mutate: &str, after: &str) -> String {
    format!(
        "fn go(s: {ty}, x: Int) -> {ty} = if x == 100 then s else {{\n  var a = s\n  {mutate}\n  {after}go(a, x + 1)\n}}\n\nfn main() -> Unit = {{\n  let r = go({}, 0)\n  println(\"done\")\n}}\n",
        match ty {
            "List[Int]" => "[]",
            "String" => "\"\"",
            _ => "bytes.zeros(100)",
        }
    )
}

#[test]
fn a_dying_param_rebound_before_a_self_tail_call_moves() {
    for (ty, mutate) in [
        ("List[Int]", "list.push(a, x)"),
        ("String", "string.push(a, \"b\")"),
        ("Bytes", "bytes.put_u8(a, x, 1)"),
        ("Bytes", "bytes.append_u8(a, 1)"),
    ] {
        let rust = emit_rust("move", &tail(ty, mutate, ""));
        let go = go_body(&rust);
        assert!(!go.contains("s.clone()"), "{ty} / {mutate}: the rebind still copies the param:\n{go}");
    }
}

#[test]
fn a_param_read_after_the_rebind_is_still_copied() {
    let after = "println(int.to_string(list.len(s)))\n  ";
    let rust = emit_rust("keep", &tail("List[Int]", "list.push(a, x)", after));
    let go = go_body(&rust);
    assert!(go.contains("s.clone()"), "the param is read after the rebind, so the rebind must copy it:\n{go}");
}

/// The aliasing program of crates/almide-wasm/tests/dying_rebind_in_place.rs:
/// a source read after its rebind, in a fold, a tail call, a var and a fn.
const ALIASING: &str = r#"fn go(s: List[Int], x: Int) -> List[Int] = if x == 3 then s else {
  var a = s
  list.push(a, x)
  println("go ${list.len(s)} ${list.len(a)}")
  go(a, x + 1)
}

fn keep(s: String) -> String = {
  var a = s
  a = a + "!"
  s + "|" + a
}

fn main() -> Unit = {
  let r = list.fold(list.range(0, 3), [9], (acc, x) => {
    var a = acc
    list.push(a, x)
    println("fold ${list.len(acc)} ${list.len(a)}")
    a
  })
  println("${r}")
  let held = [7]
  let t = go(held, 0)
  println("${held} ${t}")
  var b = bytes.zeros(2)
  var c = b
  bytes.put_u8(c, 0, 5)
  println("${bytes.get_or(b, 0, 0 - 1)} ${bytes.get_or(c, 0, 0 - 1)}")
  println(keep("hi"))
  let base = "x"
  let both = list.fold(list.range(0, 2), base, (acc, k) => {
    var a = acc
    a = a + "y"
    acc + a
  })
  println("${base} ${both}")
}
"#;
const EXPECTED: &str = "fold 1 2\nfold 2 3\nfold 3 4\n[9, 0, 1, 2]\ngo 1 2\ngo 2 3\ngo 3 4\n[7] [7, 0, 1, 2]\n0 5\nhi|hi!\nx xxyxxyy\n";

#[test]
fn native_and_wasm_agree_when_the_source_outlives_the_rebind() {
    let file = scratch("alias").join("aliasing.almd");
    std::fs::write(&file, ALIASING).unwrap();
    for target in [None, Some("wasm")] {
        let mut cmd = Command::new(almide());
        cmd.args(["run", file.to_str().unwrap()]);
        if let Some(t) = target {
            cmd.args(["--target", t]);
        }
        let out = cmd.output().expect("almide run");
        assert!(out.status.success(), "{target:?}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED, "{target:?} stdout");
    }
}
