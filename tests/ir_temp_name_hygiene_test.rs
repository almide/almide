//! Compiler-temp IR vars and user bindings live in disjoint Rust name spaces
//! (#3333).
//!
//! The Rust walker renders a var by its NAME. Lowering and the codegen passes
//! allocate temps (`__lit_guard_N`, `__tco_tmp_<param>`, `__eff_wrapN`,
//! `__cap_N`, `_fn_argN`, …) whose names a user can also spell, so a user
//! binding with the same spelling was one Rust identifier with the temp:
//!
//! - a user `__cap_0` bound to a captured-and-written var was rendered as the
//!   capture rename (`let __cap_0 = x.clone()`, an `Rc` clone of `x`'s cell),
//!   so writes through it reached `x` — native printed `111 111`, wasm `11 111`;
//! - a user `__lit_guard_0`, `__tco_tmp_a` or `__eff_wrap0` read under the
//!   temp's binding was refused with a `[COMPILER BUG]` by the #3049 gate.
//!
//! A temp is now marked `VarInfo::synthetic` at allocation (only a source
//! binder clears it) and renders as `__almide_ir{k}_{rest}`, inside the
//! reserved `__almide_` space that `escape_rust_ident` keeps every user name
//! out of. Each program runs on both legs and against a twin whose user names
//! are neutral, so a collision shows as a divergence from the twin.

use std::process::Command;

/// Every collision shape found by the audit, with the user name spelling the
/// temp at the position the temp is generated.
const COLLIDING: &str = r#"
fn adder(n: Int) -> (Int) -> Int = (x) => x + n

effect fn apply_slot(f: effect (Int) -> Int, v: Int) -> Int = f(v)!

fn pick(s: Option[String], __lit_guard_0: String) -> String =
  match s {
    some("a") => "A:" + __lit_guard_0,
    _ => "other",
  }

fn go(a: Int, b: Int, n: Int) -> Int = {
  let __tco_tmp_a = n * 100
  let __almide_ir2_tco_tmp_b = n * 1000
  if n == 0 then a + b
  else go(a + 1, b + __tco_tmp_a + __almide_ir2_tco_tmp_b, n - 1)
}

fn twice(x: Int) -> Int = x * 2

effect fn main() -> Unit = {
  let __eff_wrap0 = 1000
  let r = apply_slot(adder(__eff_wrap0), 5)!
  println("${r}")
  println(pick(some("a"), "user"))
  println("${go(0, 0, 3)}")
  var x = 1
  let bump_x = () => { x = x + 10 }
  bump_x()
  var __cap_0 = x
  let bump_c = () => { __cap_0 = __cap_0 + 100 }
  bump_c()
  println("${x} ${__cap_0}")
  let _fn_arg0 = 7
  let ys = [1, 2] |> list.map(twice) |> list.map((v) => v + _fn_arg0)
  println("${ys}")
}
"#;

/// The user spellings above and their neutral twins (longest first, so no
/// replacement rewrites part of another).
const RENAMES: &[(&str, &str)] = &[
    ("__almide_ir2_tco_tmp_b", "u_rendered"),
    ("__lit_guard_0", "u_lit"),
    ("__tco_tmp_a", "u_tco"),
    ("__eff_wrap0", "u_eff"),
    ("_fn_arg0", "u_fn"),
    ("__cap_0", "u_cap"),
];

const EXPECTED: &str = "1005\nA:user\n6603\n11 111\n[9, 11]\n";

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn write(dir: &std::path::Path, program: &str) -> std::path::PathBuf {
    let source = dir.join("main.almd");
    std::fs::write(&source, program).expect("source");
    source
}

/// Build and run `program` on one leg; the stdout.
fn run(program: &str, target: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write(dir.path(), program);
    let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
    let built = Command::new(almide_bin())
        .args(["build", source.to_str().expect("path"), "--target", target, "-o"])
        .arg(&artifact)
        .env_remove("ALMIDE_COMPONENT_P3")
        .output()
        .expect("build");
    let log = format!("{}{}", String::from_utf8_lossy(&built.stdout), String::from_utf8_lossy(&built.stderr));
    assert!(built.status.success(), "{target} build:\n{log}");
    let mut command = if target == "rust" {
        Command::new(&artifact)
    } else {
        let mut c = Command::new("wasmtime");
        c.arg("run").arg(&artifact);
        c
    };
    let out = command.output().expect("run");
    assert!(out.status.success(), "{target} exited {:?}", out.status.code());
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn neutral_twin() -> String {
    RENAMES.iter().fold(COLLIDING.to_string(), |src, (from, to)| src.replace(from, to))
}

#[test]
fn user_names_spelling_temps_answer_like_their_neutral_twin_on_native() {
    assert_eq!(run(COLLIDING, "rust"), EXPECTED);
    assert_eq!(run(&neutral_twin(), "rust"), EXPECTED);
}

#[test]
fn user_names_spelling_temps_answer_like_their_neutral_twin_on_wasm() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(run(COLLIDING, "wasm"), EXPECTED);
    assert_eq!(run(&neutral_twin(), "wasm"), EXPECTED);
}

/// The closed-by-construction property: a user binding that spells a temp's
/// RENDERED Rust name is escaped out of the reserved space, so the temp and
/// the user binding are two Rust identifiers.
#[test]
fn a_user_name_cannot_spell_a_rendered_temp() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write(dir.path(), COLLIDING);
    let out = Command::new(almide_bin())
        .args([source.to_str().expect("path"), "--target", "rust"])
        .output()
        .expect("emit");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout);
    assert!(rust.contains("let __almide_ir2_tco_tmp_b"), "the temp keeps its reserved spelling:\n{rust}");
    assert!(rust.contains("almide_kw___almide_ir2_tco_tmp_b"), "the user binding is escaped:\n{rust}");
    for (user, _) in &RENAMES[1..] {
        assert!(
            rust.contains(&format!("let {user}")) || rust.contains(&format!("let mut {user}")) || rust.contains(&format!("{user}: ")),
            "user binding `{user}` keeps its own spelling:\n{rust}"
        );
    }
}
