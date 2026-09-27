//! ADR-0021 step 3 (#2724): a lambda whose failure channel ε is a user type
//! `E` lowers to a Rust closure returning `Result<_, E>` whose `?` converts
//! nothing, and a lambda whose channel fell to `String` keeps the Debug
//! `map_err` on each typed operand.
//!
//! The lowering reads the closure's OWN channel (`with_fn_err_ty` in
//! `walker/expressions.rs`, #2722), so this test pins the outcome across the
//! family rather than one rendering: every fallible core list HOF, a user HOF
//! slot, a typed `let`, a nested callback, a guard's typed `err(..)`, and the
//! `String` channel. Each program's generated Rust must compile with no
//! warnings beyond the two classes every generated file carries (the runtime
//! prelude's non-snake-case helper names and its unused macros), and must
//! print exactly the pinned bytes.

use std::path::{Path, PathBuf};
use std::process::Command;

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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
        && Command::new("rustc").arg("--version").output().is_ok()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-2724-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// Emit Rust, compile it warning-free, run it; the program's stdout.
fn emit_compile_run(name: &str, src: &str) -> String {
    let dir = scratch(name);
    let almd = dir.join("main.almd");
    std::fs::write(&almd, src).expect("write source");
    let emitted = Command::new(almide_bin()).arg(&almd).args(["--target", "rust"]).output().expect("spawn almide");
    assert!(emitted.status.success(), "{name}: emit failed:\n{}", String::from_utf8_lossy(&emitted.stderr));
    let rs = dir.join("main.rs");
    std::fs::write(&rs, &emitted.stdout).expect("write rust");
    let bin = dir.join("main");
    let rustc = Command::new("rustc")
        .args(["--edition", "2021", "-D", "warnings", "-A", "non_snake_case", "-A", "unused_macros", "-o"])
        .arg(&bin)
        .arg(&rs)
        .output()
        .expect("spawn rustc");
    assert!(
        rustc.status.success(),
        "{name}: the generated Rust does not compile warning-free:\n{}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let run = Command::new(&bin).output().expect("run binary");
    String::from_utf8_lossy(&run.stdout).into_owned()
}

const PRELUDE: &str = "type E: Eq, Repr = | Neg(Int) | Bad(String)

fn chk(x: Int) -> Int!E = if x < 0 then err(Neg(x)) else ok(x)
";

#[test]
fn every_fallible_list_hof_carries_a_typed_channel() {
    if !tools_available() {
        eprintln!("skip: almide or rustc unavailable");
        return;
    }
    let src = format!(
        "{PRELUDE}
fn f_map(xs: List[Int]) -> List[Int]!E = xs |> list.map((x) => {{
  let v = chk(x)!
  v * 2
}})!

fn f_filter(xs: List[Int]) -> List[Int]!E = xs |> list.filter((x) => {{
  let v = chk(x)!
  v > 1
}})!

fn f_flat_map(xs: List[Int]) -> List[Int]!E = xs |> list.flat_map((x) => {{
  let v = chk(x)!
  [v, v]
}})!

fn f_filter_map(xs: List[Int]) -> List[Int]!E = xs |> list.filter_map((x) => {{
  let v = chk(x)!
  if v > 1 then some(v) else none
}})!

fn f_fold(xs: List[Int]) -> Int!E = xs |> list.fold(0, (a, x) => {{
  let v = chk(x)!
  a + v
}})!

fn f_find(xs: List[Int]) -> Option[Int]!E = xs |> list.find((x) => {{
  let v = chk(x)!
  v > 1
}})!

fn f_each(xs: List[Int]) -> Unit!E = xs |> list.each((x) => {{
  let v = chk(x)!
  println(\"each ${{v}}\")
}})!

fn main() -> Unit = {{
  println(\"${{f_map([1, 2])}} ${{f_map([1, -2])}}\")
  println(\"${{f_filter([1, 2])}} ${{f_filter([1, -2])}}\")
  println(\"${{f_flat_map([1, 2])}} ${{f_flat_map([1, -2])}}\")
  println(\"${{f_filter_map([1, 2])}} ${{f_filter_map([1, -2])}}\")
  println(\"${{f_fold([1, 2])}} ${{f_fold([1, -2])}}\")
  println(\"${{f_find([1, 2])}} ${{f_find([1, -2])}}\")
  let a = match f_each([1, 2]) {{ ok(_) => \"ok\", err(e) => \"${{e}}\" }}
  let b = match f_each([-2]) {{ ok(_) => \"ok\", err(e) => \"${{e}}\" }}
  println(\"${{a}} ${{b}}\")
}}
"
    );
    assert_eq!(
        emit_compile_run("hofs", &src),
        "ok([2, 4]) err(Neg(-2))\n\
         ok([2]) err(Neg(-2))\n\
         ok([1, 1, 2, 2]) err(Neg(-2))\n\
         ok([2]) err(Neg(-2))\n\
         ok(3) err(Neg(-2))\n\
         ok(some(2)) err(Neg(-2))\n\
         each 1\neach 2\n\
         ok Neg(-2)\n"
    );
}

#[test]
fn slots_lets_nesting_and_guards_carry_the_typed_channel() {
    if !tools_available() {
        eprintln!("skip: almide or rustc unavailable");
        return;
    }
    let src = format!(
        "{PRELUDE}
fn sum_with(xs: List[Int], f: (Int) -> Int!E) -> Int!E = xs |> list.fold(0, (a, x) => a + f(x)!)!

fn slot(xs: List[Int]) -> Int!E = sum_with(xs, (n) => chk(n)! * 2)

fn typed_let(xs: List[Int]) -> Int!E = {{
  let f: (Int) -> Int!E = (n) => chk(n)! + 1
  sum_with(xs, f)
}}

fn nested(xss: List[List[Int]]) -> List[Int]!E = xss |> list.map((xs) => {{
  let inner = xs |> list.map((x) => chk(x)!)!
  list.len(inner)
}})!

fn guarded(xs: List[Int]) -> Int!E = xs |> list.fold(0, (a, x) => {{
  guard x != 100 else err(Bad(\"hundred\"))
  a + chk(x)!
}})!

fn main() -> Unit = {{
  println(\"${{slot([1, 2])}} ${{slot([1, -2])}}\")
  println(\"${{typed_let([1, 2])}} ${{typed_let([-4])}}\")
  println(\"${{nested([[1], [2, 3]])}} ${{nested([[1], [-3]])}}\")
  println(\"${{guarded([1, 2])}} ${{guarded([1, 100])}}\")
}}
"
    );
    assert_eq!(
        emit_compile_run("slots", &src),
        "ok(6) err(Neg(-2))\n\
         ok(5) err(Neg(-4))\n\
         ok([1, 2]) err(Neg(-3))\n\
         ok(3) err(Bad(\"hundred\"))\n"
    );
}

/// ADR-0021 D2: a `-> T!` fn consuming the HOF result with `)!` prints the
/// same Debug text as before — the channel falls to `String` and each typed
/// operand keeps its Debug `map_err`. The bytes are 0.64.0's.
#[test]
fn a_string_channel_keeps_the_debug_text() {
    if !tools_available() {
        eprintln!("skip: almide or rustc unavailable");
        return;
    }
    let src = format!(
        "{PRELUDE}
fn canonical(xs: List[Int]) -> List[Int]! = xs |> list.map((x) => chk(x)!)!

fn block(xs: List[Int]) -> List[Int]! = xs |> list.map((x) => {{
  let v = chk(x)!
  v + 1
}})!

fn mixed(xs: List[String]) -> Int! = xs |> list.fold(0, (a, s) => {{
  let x = int.parse(s)!
  a + chk(x)!
}})!

fn main() -> Unit = {{
  println(\"${{canonical([1, -2])}}\")
  println(\"${{block([1, -2])}}\")
  println(\"${{mixed([\\\"1\\\", \\\"-2\\\"])}} ${{mixed([\\\"x\\\"])}}\")
}}
"
    );
    assert_eq!(
        emit_compile_run("string", &src),
        "err(\"Neg(-2)\")\n\
         err(\"Neg(-2)\")\n\
         err(\"Neg(-2)\") err(\"invalid digit found in string\")\n"
    );
}
