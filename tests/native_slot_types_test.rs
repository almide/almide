//! #4006: untyped slot blocks typed from their stores render natively
//! (crates/almide-mir/src/render_native_slots.rs), byte-identical to the v0
//! native pipeline. A tuple of mixed slot kinds is a generated struct of its
//! slot kinds, built whole when its fields are stored in one straight run;
//! the last read of a heap slot of an owned record moves the value out
//! instead of copying it, and every other read copies.

use std::path::PathBuf;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almd_native_slots_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// stdout + exit code of the v0 native pipeline (the oracle).
fn run_v0(name: &str, src: &str) -> (String, i32) {
    let file = scratch(name).join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(almide())
        .args(["run", "--no-verified", file.to_str().unwrap()])
        .env("ALMIDE_NO_VERIFIED_OK", "1")
        .output()
        .expect("almide run");
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

/// The v1 render, and its stdout + exit code once rustc compiled it.
fn run_v1(name: &str, src: &str) -> (String, String, i32) {
    let rust = almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("v1 native walled on `{name}`: {e:?}"));
    let dir = scratch(name);
    let (rs, bin) = (dir.join("prog.rs"), dir.join("prog_bin"));
    std::fs::write(&rs, &rust).unwrap();
    let rc = Command::new("rustc").args(["-O", "-o", bin.to_str().unwrap(), rs.to_str().unwrap()]).output().expect("rustc");
    assert!(rc.status.success(), "rustc rejected `{name}`:\n{}", String::from_utf8_lossy(&rc.stderr));
    let out = Command::new(&bin).output().expect("run v1 binary");
    (rust, String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

fn same(name: &str, src: &str) -> String {
    let (rust, out, code) = run_v1(name, src);
    assert_eq!((out, code), run_v0(name, src), "`{name}`: v1 native differs from v0");
    rust
}

#[test]
fn a_mixed_tuple_is_a_struct_of_its_slot_kinds_and_its_one_read_moves() {
    let rust = same(
        "tuple_once",
        "fn main() -> Unit = {\n  let t: (Int, String) = (4, \"x\")\n  println(match t {\n    (n, s) => int.to_string(n) + s\n  })\n}\n",
    );
    assert!(rust.contains("f0: i64,") && rust.contains("f1: AlmideString,"), "no struct of the slot kinds:\n{rust}");
    assert!(
        rust.lines().any(|l| l.contains("= std::mem::take(&mut v") && l.contains(".f1)")),
        "the one read of the String slot copies instead of moving:\n{rust}"
    );
    assert!(
        rust.lines().any(|l| l.contains(" = AlmdRec") && l.contains("{ f0: ")),
        "the tuple is assigned field by field instead of built whole:\n{rust}"
    );
}

/// A slot read again later, or read inside a loop, is copied: moving it out
/// would leave the later read an empty string.
#[test]
fn a_slot_read_again_or_in_a_loop_is_copied() {
    same(
        "tuple_twice",
        "fn main() -> Unit = {\n  let t: (Int, String) = (4, \"xy\")\n  println(t.1)\n  println(t.1 + int.to_string(t.0))\n}\n",
    );
    same(
        "tuple_loop",
        "fn main() -> Unit = {\n  let t: (Int, String) = (4, \"xy\")\n  var acc = \"\"\n  for i in 0..<3 {\n    acc = acc + t.1\n  }\n  println(acc)\n}\n",
    );
}

/// A string literal renders as an immortal static block: one grown in place
/// copies before it writes, so the literal reads the same on every
/// evaluation, in a loop too.
#[test]
fn a_static_literal_grown_in_place_copies_first() {
    same(
        "literal_grow",
        "fn main() -> Unit = {\n  var s = \"ab\"\n  s = s + \"x\"\n  println(s)\n  for i in 0..<3 {\n    var t = \"k\"\n    t = t + \"z\"\n    println(t)\n  }\n  println(\"k\")\n}\n",
    );
}
