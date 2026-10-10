//! A user program cannot reach the generated crate's runtime spellings
//! (#3486, #3487).
//!
//! The native runtime is spliced into the same Rust module as the user's code,
//! and several facts about the generated crate are keyed on runtime
//! spellings. A user could write them:
//!
//! - #3487: a user fn `almide_rt_list_len` was collapsed into the runtime call
//!   of that name and defined twice beside it (rustc E0428 / E0308), and a
//!   user `type AlmdRec_x_y` was the name of the struct emitted for the
//!   anonymous record `{ x, y }` (E0428 / E0107). Lowering now escapes the fn
//!   (`almide_fn_almide_rt_…`, the #3483 escape) and `IrLinkFlatten`
//!   qualifies the type (`almide_rt_self_AlmdRec_x_y`, the #2842 rename).
//! - #3486: decisions were made by `contains` on the generated Rust TEXT,
//!   which carries every user string literal verbatim. `"almide_rt_prim_budget_x"`
//!   refused a working program (run) and failed a passing test file (test);
//!   `"AlmideHttpRequest almide_rt_zlib_ almide_rt_matrix_"` spliced 98
//!   runtime fns and declared rustls / flate2. The metered-prim refusal is now
//!   read from the IR's `RuntimeCall`s, and module / crate inclusion from the
//!   code's identifier tokens.
//!
//! Every colliding program is checked against a twin whose user spelling is
//! neutral, on both legs where the leg can run it. `ALMIDE_BIN` points the
//! whole file at another build for an A/B.

use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn log(out: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// `almide <args…> <file>` on `program` written as `name`; the output.
fn almide(name: &str, program: &str, args: &[&str]) -> std::process::Output {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join(name);
    std::fs::write(&source, program).expect("source");
    Command::new(almide_bin())
        .args(args)
        .arg(&source)
        .env_remove("ALMIDE_COMPONENT_P3")
        .output()
        .expect("almide")
}

/// `almide run` on one leg must succeed; its stdout.
fn run(program: &str, target: &str) -> String {
    let out = almide("main.almd", program, &["run", "--target", target]);
    assert!(out.status.success(), "{target} run failed:\n{}", log(&out));
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// The `--target rust` emission of `program`.
fn emit_rust(program: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let out = Command::new(almide_bin()).arg(&source).args(["--target", "rust"]).output().expect("almide");
    assert!(out.status.success(), "emit failed:\n{}", log(&out));
    String::from_utf8_lossy(&out.stdout).to_string()
}

// ── #3487.1: a user fn spelled `almide_rt_<module>_<fn>` ─────────────────

/// The `@extern` fn walls the v1 native render, so `run` takes the v0
/// fallback where the collision lived.
const USER_RT_FN: &str = r#"@extern(rs, "std::cmp", "max")
fn my_max(a: Int, b: Int) -> Int

fn almide_rt_list_len(x: Int) -> Int = x + 100

effect fn main() -> Unit = {
  println(int.to_string(my_max(1, 2)))
  println(int.to_string(almide_rt_list_len(1)))
  println(int.to_string(list.len([1, 2, 3])))
}
"#;

#[test]
fn a_user_fn_spelled_like_a_runtime_symbol_is_its_own_fn_on_native() {
    assert_eq!(run(USER_RT_FN, "rust"), "2\n101\n3\n");
    assert_eq!(run(&USER_RT_FN.replace("almide_rt_list_len", "almide_rx_list_len"), "rust"), "2\n101\n3\n");
}

/// Wasm cannot run the `@extern` fn, so its twin drops it; the user fn and the
/// runtime call must still be two different fns there.
#[test]
fn a_user_fn_spelled_like_a_runtime_symbol_is_its_own_fn_on_wasm() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let program = USER_RT_FN
        .replace("@extern(rs, \"std::cmp\", \"max\")\nfn my_max(a: Int, b: Int) -> Int\n", "fn my_max(a: Int, b: Int) -> Int = if a > b then a else b\n");
    assert_eq!(run(&program, "wasm"), "2\n101\n3\n");
}

/// Closed by construction on the emitted Rust: the user's fn is defined and
/// called under its escaped name, and the runtime's keeps its own.
#[test]
fn a_user_fn_cannot_spell_a_runtime_symbol_in_the_emitted_rust() {
    let rust = emit_rust(USER_RT_FN);
    assert!(rust.contains("pub fn almide_fn_almide_rt_list_len("), "user fn is escaped:\n{rust}");
    assert!(rust.contains("almide_fn_almide_rt_list_len(1i64)"), "the user's call reaches the user fn");
    assert_eq!(rust.matches("fn almide_rt_list_len<").count() + rust.matches("fn almide_rt_list_len(").count(), 1, "one runtime definition");
}

// ── #3487.2: a user type spelled `AlmdRec_<sorted field names>` ──────────

const USER_ANON_REC_TYPE: &str = r#"type AlmdRec_x_y = { x: String, y: String }

fn mk(n: Int) -> { x: Int, y: Int } = { x: n, y: n + 1 }

effect fn main() -> Unit = {
  let a = mk(1)
  let b = AlmdRec_x_y { x: "p", y: "q" }
  println("${a.x} ${a.y} ${b.x} ${b.y}")
}
"#;

#[test]
fn a_user_type_spelled_like_an_anonymous_record_struct_builds_on_native() {
    assert_eq!(run(USER_ANON_REC_TYPE, "rust"), "1 2 p q\n");
    assert_eq!(run(&USER_ANON_REC_TYPE.replace("AlmdRec_x_y", "AlmdRex_x_y"), "rust"), "1 2 p q\n");
}

#[test]
fn a_user_type_spelled_like_an_anonymous_record_struct_runs_on_wasm() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(run(USER_ANON_REC_TYPE, "wasm"), "1 2 p q\n");
}

// ── #3486.1: a literal spelled like a metered prim ───────────────────────

const PRIM_LITERAL: &str = r#"@extern(rs, "std::cmp", "max")
fn my_max(a: Int, b: Int) -> Int

effect fn main() -> Unit = {
  println(int.to_string(my_max(1, 2)))
  println("almide_rt_prim_budget_x")
}
"#;

#[test]
fn a_literal_spelled_like_a_metered_prim_does_not_refuse_the_native_fallback() {
    assert_eq!(run(PRIM_LITERAL, "rust"), "2\nalmide_rt_prim_budget_x\n");
    let twin = PRIM_LITERAL.replace("almide_rt_prim_budget_x", "almide_rt_prim_budgex_x");
    assert_eq!(run(&twin, "rust"), "2\nalmide_rt_prim_budgex_x\n");
    let timeout = PRIM_LITERAL.replace("almide_rt_prim_budget_x", "almide_rt_prim_timeout_x");
    assert_eq!(run(&timeout, "rust"), "2\nalmide_rt_prim_timeout_x\n");
}

const PRIM_LITERAL_TEST: &str = r#"// wasm:skip
test "a" {
  assert_eq(string.len("almide_rt_prim_budget_x"), 23)
}
"#;

#[test]
fn a_literal_spelled_like_a_metered_prim_does_not_fail_the_native_test_harness() {
    for program in [PRIM_LITERAL_TEST.to_string(), PRIM_LITERAL_TEST.replace("budget_x", "budgex_x")] {
        let out = almide("main_test.almd", &program, &["test"]);
        assert!(out.status.success(), "test failed:\n{}", log(&out));
    }
}

/// The negative control: a program that DOES call the metered prims still
/// gets the honest refusal on both native routes when the v1 render walls.
#[test]
fn a_real_metered_prim_is_still_refused_on_the_native_fallback() {
    let program = r#"@extern(rs, "std::cmp", "max")
fn my_max(a: Int, b: Int) -> Int

fn heavy(n: Int) -> Int = if n <= 0 then 0 else n + heavy(n - 1)

effect fn main() -> Unit = {
  println(int.to_string(my_max(1, 2)))
  let r = fan.bounded(compute.ns(1000000)) { heavy(10) } ?? -1
  println(int.to_string(r))
}
"#;
    let out = almide("main.almd", program, &["run", "--target", "rust"]);
    assert!(!out.status.success(), "a metered program ran on v0:\n{}", log(&out));
    assert!(log(&out).contains("fan.bounded / fan.race require the v1 native render"), "{}", log(&out));

    let test = "// wasm:skip\nfn heavy(n: Int) -> Int = if n <= 0 then 0 else n + heavy(n - 1)\n\ntest \"a\" {\n  let r = fan.bounded(compute.ns(1000000)) { heavy(10) } ?? -1\n  assert_eq(r, 55)\n}\n";
    let out = almide("main_test.almd", test, &["test"]);
    assert!(!out.status.success(), "a metered test ran on the native harness:\n{}", log(&out));
    assert!(log(&out).contains("run on the WASM test leg"), "{}", log(&out));
}

// ── #3486.2: a literal spelled like a runtime module or type ─────────────

const RUNTIME_LITERALS: &str = r#"effect fn main() -> Unit = {
  println("AlmideHttpRequest almide_rt_zlib_ almide_rt_matrix_ almide_rt_sse_x")
}
"#;

#[test]
fn a_literal_spelled_like_a_runtime_module_splices_nothing_and_adds_no_crate() {
    let rust = emit_rust(RUNTIME_LITERALS);
    let twin = emit_rust(&RUNTIME_LITERALS.replace("almide_rt_", "almide_rx_").replace("AlmideHttp", "AlmidxHttp"));
    for module in ["http", "zlib", "matrix", "sse"] {
        let def = format!("fn almide_rt_{module}_");
        assert_eq!(rust.matches(&def).count(), twin.matches(&def).count(), "a literal spliced `{module}`");
        assert_eq!(rust.matches(&def).count(), 0, "a literal spliced `{module}`");
    }
    assert!(almide_codegen::runtime_crate_deps(&rust).is_empty(), "{:?}", almide_codegen::runtime_crate_deps(&rust));
    assert!(!almide_codegen::rust_idents::has_ident_with_prefix(&rust, "almide_rt_matrix_"), "the matrix route is keyed on a literal");
    assert_eq!(run(RUNTIME_LITERALS, "rust"), "AlmideHttpRequest almide_rt_zlib_ almide_rt_matrix_ almide_rt_sse_x\n");
}

/// The positive control: a real use still splices its module and declares
/// its crate — the identifier scan did not lose the references it exists for.
#[test]
fn a_real_runtime_use_still_splices_its_module_and_crate() {
    let rust = emit_rust("import zlib\n\neffect fn main() -> Unit = {\n  let c = zlib.compress(bytes.from_string(\"aaaa\"))!\n  println(int.to_string(bytes.len(c)))\n}\n");
    assert!(rust.contains("fn almide_rt_zlib_"), "zlib was not spliced");
    assert!(almide_codegen::runtime_crate_deps(&rust).iter().any(|(n, _)| *n == "flate2"), "flate2 was not declared");

    let rust = emit_rust("effect fn main() -> Unit = {\n  let x = 2.0 ** 3.0\n  println(float.to_string(x))\n}\n");
    assert!(rust.contains("fn almide_rt_math_fpow"), "an operator's runtime symbol no longer splices its module");
}
