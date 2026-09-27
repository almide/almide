//! A typed-error `!` inside a `list.map` callback, with the map's result
//! consumed by VALUE (`let r = ...; match r { ... }`) rather than by `)!`:
//! whatever `almide check` accepts in this family, the native build compiles
//! and runs (#2722, ADR-0021 step 1).
//!
//! On 0.64.0 two holes let this family pass check and die in rustc:
//!
//! - an `err(..)` match arm joins as `Never`, so its payload was never judged
//!   against the error type the match produces — `err(e)` with `e: String`
//!   re-wrapped into a `-> T!E` fn was E0308 in rustc, while the same two ctors
//!   as `if` branches were already E001 at check time;
//! - the closure body's `!` compared its operand's error with the ENCLOSING
//!   fn's error type, not the closure's own `String` channel, so a typed `!`
//!   inside a callback in a `-> T!E` fn lost its Debug `map_err` — E0277
//!   "`?` couldn't convert the error to `String`".
//!
//! The test pins the agreement, not one verdict: every program is put to
//! `almide check`, and every program check accepts is built and run natively
//! with its stdout compared. When ADR-0021 step 2 lets the rejected shapes
//! type-check, they must still build.

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
    let bin_ok = Command::new(almide_bin()).arg("--version").output().is_ok();
    let cargo_ok = Command::new("cargo").arg("--version").output().is_ok();
    bin_ok && cargo_ok
}

fn write_case(name: &str, src: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-2722-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("main.almd");
    std::fs::write(&path, src).expect("write source");
    path
}

fn run(args: &[&str], path: &Path) -> (bool, String) {
    let out = Command::new(almide_bin()).args(args).arg(path).output().expect("spawn almide");
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

const PRELUDE: &str = "type E: Eq, Repr = | Neg(Int)

fn step(acc: Int, x: Int) -> Int!E = {
  guard x >= 0 else err(Neg(x))
  acc + x
}
";

/// The callback spellings: none is the canonical `(x) => f(x)!`, so every one
/// keeps the typed `!` inside a closure body.
const CALLBACKS: &[(&str, &str)] = &[
    ("match_leaf", "(x) => match x { 0 => 0, _ => step(0, x)! }"),
    ("if_leaf", "(x) => if x == 0 then 0 else step(0, x)!"),
    ("block_let", "(x) => {\n    let v = step(0, x)!\n    v\n  }"),
    ("braced", "(x) => { step(0, x)! }"),
    ("operand", "(x) => step(0, x)! + 0"),
];

/// Consumed by value and converted: the String error is read as a String.
fn converting(cb: &str) -> String {
    format!(
        "{PRELUDE}
fn b(xs: List[Int]) -> List[Int]!E = {{
  let r = xs |> list.map({cb})
  match r {{
    ok(v) => ok(v),
    err(s) => err(Neg(string.len(s))),
  }}
}}

fn c(xs: List[Int]) -> List[Int] = {{
  let r = xs |> list.map({cb})
  match r {{
    ok(v) => v,
    err(s) => [string.len(s)],
  }}
}}

fn main() -> Unit = {{
  println(\"${{b([1, 0])}}\")
  println(\"${{b([1, -2])}}\")
  println(\"${{c([1, -2])}}\")
}}
"
    )
}

/// Consumed by value and re-wrapped unchanged into the typed-error fn: the
/// shape of #2722. Its `err(e)` arm carries whatever the callback's channel is.
fn rewrapping(cb: &str) -> String {
    format!(
        "{PRELUDE}
fn b(xs: List[Int]) -> List[Int]!E = {{
  let r = xs |> list.map({cb})
  match r {{
    ok(v) => ok(v),
    err(e) => err(e),
  }}
}}

fn main() -> Unit = {{
  println(\"${{b([1, 0])}}\")
  println(\"${{b([1, -2])}}\")
}}
"
    )
}

/// Check, and when check accepts, build + run natively. Returns `None` for a
/// program check rejects, else the program's stdout — a build failure after an
/// accepting check is the bug this test exists for and fails the test.
fn check_then_run(name: &str, src: &str) -> Option<String> {
    let path = write_case(name, src);
    let (checked, check_out) = run(&["check"], &path);
    if !checked {
        assert!(
            check_out.contains("error[E"),
            "{name}: check failed without a coded diagnostic:\n{check_out}"
        );
        return None;
    }
    let (ran, run_out) = run(&["run"], &path);
    assert!(
        ran,
        "{name}: `almide check` accepted this program but the native build failed:\n{run_out}\n--- source ---\n{src}"
    );
    Some(run_out.lines().filter(|l| !l.is_empty() && !l.starts_with(' ')).filter(|l| {
        l.starts_with("ok(") || l.starts_with("err(") || l.starts_with('[')
    }).collect::<Vec<_>>().join("\n"))
}

#[test]
fn a_converted_value_consumption_checks_and_runs() {
    if !tools_available() {
        eprintln!("skip: almide binary or cargo unavailable");
        return;
    }
    // `Neg(-2)` is 7 characters: the callback's String channel carries the
    // typed error's Debug text, which `err(s)` then reads.
    for (name, cb) in CALLBACKS {
        let out = check_then_run(&format!("convert-{name}"), &converting(cb))
            .unwrap_or_else(|| panic!("{name}: check rejected a well-typed program"));
        assert_eq!(out, "ok([1, 0])\nerr(Neg(7))\n[7]", "{name}: stdout");
    }
}

#[test]
fn an_unchanged_rewrap_never_passes_check_and_fails_the_build() {
    if !tools_available() {
        eprintln!("skip: almide binary or cargo unavailable");
        return;
    }
    for (name, cb) in CALLBACKS {
        if let Some(out) = check_then_run(&format!("rewrap-{name}"), &rewrapping(cb)) {
            // Accepted only once the callback's channel carries `E` itself
            // (ADR-0021 step 2); the typed error then arrives unconverted.
            assert_eq!(out, "ok([1, 0])\nerr(Neg(-2))", "{name}: stdout");
        }
    }
}

/// ADR-0021: the fs streaming walkers' fallible carriers share one channel
/// with the walk's own read error, which is a `String` — a typed callback
/// there erases, and propagating it into a typed-error fn must be a check
/// error, not the rustc E0277 an `E`-generic declaration led to.
#[test]
fn an_fs_walker_callback_channel_is_string() {
    if !tools_available() {
        eprintln!("skip: almide binary or cargo unavailable");
        return;
    }
    let dir = std::env::temp_dir().join(format!("almide-2723-fs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let data = dir.join("lines.txt");
    std::fs::write(&data, "1\n-2\n").expect("write data");
    let body = |ret: &str, err: &str| format!(
        "import fs
{PRELUDE}
effect fn tot(p: String) -> Int!{ret} = fs.fold_lines(p, 0, (a, l) => {{
  let v = step(a, int.parse(l) ?? 0)!
  v
}})!

effect fn main() -> Unit = {{
  let r: Result[Int, {err}] = tot(\"{}\")
  println(\"${{r}}\")
}}
",
        data.display()
    );
    assert_eq!(check_then_run("fs-typed", &body("E", "E")), None, "a typed-error fn cannot take the walker's String channel");
    assert_eq!(check_then_run("fs-string", &body("", "String")).as_deref(), Some("err(\"Neg(-2)\")"));
}

/// An operand still a bare variable at its `!` (a `let`-bound lambda's
/// unannotated parameter) is judged once solving decides it: a `String` error
/// into a callback whose channel the typed fn pins is a check error, not the
/// rustc E0277 it was when the operand's link to the channel was dropped.
#[test]
fn a_let_bound_callback_param_operand_is_judged_once_decided() {
    if !tools_available() {
        eprintln!("skip: almide binary or cargo unavailable");
        return;
    }
    let src = |e: &str| format!(
        "type ShiftErr: Eq, Repr = | Bad(String)

fn bump(x: Result[Int, {e}]) -> Int!ShiftErr = {{
  let cb = (r) => r! + 1
  cb(x)!
}}

fn main() -> Unit = {{
  println(\"${{bump(ok(1))}}\")
}}
"
    );
    assert_eq!(check_then_run("param-string", &src("String")), None);
    assert_eq!(check_then_run("param-typed", &src("ShiftErr")).as_deref(), Some("ok(2)"));
}
