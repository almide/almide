//! NATIVE trust-spine differential gate (#764, rung 1).
//!
//! Every program in the rung-1 corpus is rendered by the v1 native leg
//! (`almide_mir::pipeline::try_render_rust_source` — Perceus MIR, Drop erased to
//! Rust scope-end), compiled with rustc, executed, and byte-compared (stdout +
//! exit code) against the SHIPPED v0 native pipeline (`almide run`). The honest
//! wall is asserted too: an out-of-subset program must decline (`Err`), never
//! render wrong code.

use std::path::PathBuf;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almd_native_v1_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// stdout + exit code of the v0 native pipeline.
fn run_v0(name: &str, src: &str) -> (String, i32) {
    let dir = scratch(name);
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(almide())
        .args(["run", file.to_str().unwrap()])
        .output()
        .expect("almide run");
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

/// stdout + exit code of the v1 native leg (render → rustc → execute).
fn run_v1(name: &str, src: &str) -> (String, i32) {
    let rust = almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("v1 native walled on in-subset program `{name}`: {e:?}"));
    let dir = scratch(name);
    let rs = dir.join("prog.rs");
    let bin = dir.join("prog_bin");
    std::fs::write(&rs, &rust).unwrap();
    let rc = Command::new("rustc")
        .args(["-O", "-o", bin.to_str().unwrap(), rs.to_str().unwrap()])
        .output()
        .expect("rustc");
    assert!(
        rc.status.success(),
        "rustc rejected v1 native render for `{name}`:\n{}\n--- source ---\n{rust}",
        String::from_utf8_lossy(&rc.stderr)
    );
    let out = Command::new(&bin).output().expect("run v1 binary");
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

const CORPUS: &[(&str, &str)] = &[
    ("pure_exit", "fn add(a: Int, b: Int) -> Int = a + b\n\nfn main() -> Unit = {\n  let x = add(1, 2)\n}\n"),
    ("print_int", "fn main() -> Unit = {\n  let y = 3 * 4\n  println(int.to_string(y))\n}\n"),
    ("fn_calls", "fn sq(x: Int) -> Int = x * x\nfn tri(x: Int) -> Int = sq(x) + x\n\nfn main() -> Unit = {\n  println(int.to_string(tri(7)))\n}\n"),
    ("if_value", "fn main() -> Unit = {\n  let n = 42\n  let label = if n > 40 then \"big\" else \"small\"\n  println(label)\n}\n"),
    ("while_loop", "fn main() -> Unit = {\n  var i = 0\n  var acc = 0\n  while i < 10 {\n    acc = acc + i\n    i = i + 1\n  }\n  println(int.to_string(acc))\n}\n"),
    ("string_literal", "fn main() -> Unit = {\n  println(\"hello, native spine\")\n}\n"),
    ("dup_clone", "fn main() -> Unit = {\n  let s = \"shared\"\n  let t = s\n  println(t)\n  println(s)\n}\n"),
    ("division", "fn main() -> Unit = {\n  println(int.to_string(97 / 8))\n  println(int.to_string(97 % 8))\n}\n"),
    // Literal + literal is const-folded by the frontend into one literal — in subset.
    ("folded_concat", "fn main() -> Unit = {\n  let s = \"a\" + \"b\"\n  println(s)\n}\n"),
    // ── Rung 2: dynamic String ops + String signatures ──
    ("dyn_concat", "fn main() -> Unit = {\n  let s = int.to_string(1) + \"b\"\n  println(s)\n}\n"),
    ("str_eq_branch", "fn main() -> Unit = {\n  let s = int.to_string(42)\n  if s == \"42\" then println(\"yes\") else println(\"no\")\n  if s == \"43\" then println(\"yes\") else println(\"no\")\n}\n"),
    ("str_len_unicode", "fn main() -> Unit = {\n  println(int.to_string(string.len(\"héllo\")))\n  println(int.to_string(string.len(int.to_string(12345))))\n}\n"),
    ("str_param_fn", "fn shout(s: String) -> String = s + \"!\"\nfn twice(s: String) -> String = shout(s) + shout(s)\n\nfn main() -> Unit = {\n  println(twice(\"hey\"))\n}\n"),
    ("loop_concat", "fn main() -> Unit = {\n  var acc = \"\"\n  var i = 0\n  while i < 5 {\n    acc = acc + int.to_string(i)\n    i = i + 1\n  }\n  println(acc)\n  println(int.to_string(string.len(acc)))\n}\n"),
    ("str_if_value", "fn label(n: Int) -> String = if n > 40 then int.to_string(n) + \"-big\" else \"small\"\n\nfn main() -> Unit = {\n  println(label(42))\n  println(label(7))\n}\n"),
    // ── Rung 3: broadened String floor (each shim = the v0 oracle expression) ──
    ("str_predicates", "fn main() -> Unit = {\n  let s = \"pre-\" + int.to_string(42) + \"-post\"\n  if string.contains(s, \"42\") then println(\"c1\") else println(\"c0\")\n  if string.starts_with(s, \"pre\") then println(\"s1\") else println(\"s0\")\n  if string.ends_with(s, \"post\") then println(\"e1\") else println(\"e0\")\n  if string.contains(s, \"x\") then println(\"c1\") else println(\"c0\")\n}\n"),
    // ── Rung 4: scalar lists via the shared MIR ops (ListLit/ListGetScalar/ListSetScalar) ──
    ("list_param", "fn head(xs: List[Int]) -> Int = xs[0]\n\nfn main() -> Unit = {\n  println(int.to_string(head([9])))\n}\n"),
    ("list_index_math", "fn pick(xs: List[Int], i: Int) -> Int = xs[i]\n\nfn main() -> Unit = {\n  let a = [10, 20, 30]\n  println(int.to_string(pick(a, 0) + pick(a, 2)))\n}\n"),
    ("list_set", "fn main() -> Unit = {\n  var b = [1, 2, 3]\n  b[1] = 99\n  println(int.to_string(b[0] + b[1] + b[2]))\n}\n"),
    ("str_transforms", "fn main() -> Unit = {\n  let s = \"héllo ßtraße \" + int.to_string(7)\n  println(string.to_upper(s))\n  println(string.to_lower(string.to_upper(s)))\n  println(string.trim(\"  padded  \" + int.to_string(1)))\n  println(string.repeat(\"ab\", 3))\n}\n"),
    ("str_ordering", "fn main() -> Unit = {\n  let a = int.to_string(41)\n  let b = int.to_string(42)\n  if a < b then println(\"lt\") else println(\"ge\")\n  if b < a then println(\"lt\") else println(\"ge\")\n  if a != b then println(\"ne\") else println(\"eq\")\n}\n"),
];

#[test]
fn native_v1_matches_v0_byte_for_byte() {
    for (name, src) in CORPUS {
        let (v0_out, v0_code) = run_v0(name, src);
        let (v1_out, v1_code) = run_v1(name, src);
        assert_eq!(
            (v0_out.as_str(), v0_code),
            (v1_out.as_str(), v1_code),
            "native v1 diverges from v0 on `{name}`"
        );
    }
}

#[test]
fn divzero_abort_matches_v0() {
    let src = "fn main() -> Unit = {\n  let a = 10\n  let b = 0\n  println(int.to_string(a / b))\n}\n";
    // v0 oracle
    let dir = scratch("divzero_v0");
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let v0 = Command::new(almide()).args(["run", file.to_str().unwrap()]).output().unwrap();
    // v1
    let rust = almide_mir::pipeline::try_render_rust_source(src).expect("in subset");
    let rs = dir.join("prog.rs");
    let bin = dir.join("prog_bin");
    std::fs::write(&rs, &rust).unwrap();
    let rc = Command::new("rustc").args(["-O", "-o", bin.to_str().unwrap(), rs.to_str().unwrap()]).output().unwrap();
    assert!(rc.status.success(), "{}", String::from_utf8_lossy(&rc.stderr));
    let v1 = Command::new(&bin).output().unwrap();
    assert_eq!(v0.status.code(), v1.status.code(), "divzero exit code diverges");
    assert_eq!(
        String::from_utf8_lossy(&v0.stdout),
        String::from_utf8_lossy(&v1.stdout),
        "divzero stdout diverges"
    );
    assert_eq!(
        String::from_utf8_lossy(&v0.stderr),
        String::from_utf8_lossy(&v1.stderr),
        "divzero stderr diverges"
    );
}

#[test]
fn out_of_subset_walls_honestly() {
    let walls = [
        ("list", "fn main() -> Unit = {\n  let xs = [1, 2, 3]\n  println(int.to_string(list.len(xs)))\n}\n"),
        ("float", "fn main() -> Unit = {\n  println(float.to_string(1.5))\n}\n"),
        ("str_split", "fn main() -> Unit = {\n  let parts = string.split(\"a,b\", \",\")\n  println(parts[0])\n}\n"),
        // list_param moved to the POSITIVE corpus — rung 4 renders scalar-list
        // params/literals/indexing natively (`vec![…]` + the bounds shims).
    ];
    for (name, src) in walls {
        assert!(
            almide_mir::pipeline::try_render_rust_source(src).is_err(),
            "`{name}` should WALL (outside rung 1) but rendered"
        );
    }
}
