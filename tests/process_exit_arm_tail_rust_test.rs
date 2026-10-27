//! #4226: `process.exit(n)` as the tail of a match arm or an `if` branch of a
//! value-returning `effect fn` rendered as `Ok::<(), String>(almide_rt_process_exit(n))`
//! in the standard Rust codegen, against the fn's `Result<i64, _>` (rustc E0308),
//! while `almide check` accepted the program. The call is `Never`-typed: it
//! yields no value to wrap, so its `!` must reach the fn's channel bare, as a
//! `panic` tail does (#3118) and a guard else does (#1541).
//!
//! `almide run` ships these programs through the v1 render, which never had
//! the bug, so the pin is on the codegen's own output (`--target rust`), the
//! route a program the v1 render walls takes: it must compile warning-free,
//! print what the taken arm yields and exit with the code `process.exit` asks for.

use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// Emit `src` through `--target rust`, compile it with rustc and run it.
fn emit_compile_run(name: &str, src: &str) -> (String, Option<i32>) {
    let dir = std::env::temp_dir().join(format!("almide-4226-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
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
        "{name}: the generated Rust does not compile warning-free (#4226):\n{}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let out = Command::new(&bin).output().expect("run binary");
    let _ = std::fs::remove_dir_all(&dir);
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code())
}

fn check(name: &str, src: &str, stdout: &str, code: i32) {
    if Command::new("rustc").arg("--version").output().is_err() {
        eprintln!("skip: rustc unavailable");
        return;
    }
    let (out, status) = emit_compile_run(name, src);
    assert_eq!(out, stdout, "{name}: stdout");
    assert_eq!(status, Some(code), "{name}: exit code");
}

#[test]
fn exit_as_a_match_arm_of_an_int_effect_fn() {
    check(
        "match_int",
        "import process

effect fn pick(n: Int) -> Int = match n {
  0 => process.exit(3),
  _ => n + 1,
}

effect fn main() -> Unit = {
  println(int.to_string(pick(5)!))
  println(int.to_string(pick(0)!))
  println(\"unreachable\")
}
",
        "6\n",
        3,
    );
}

#[test]
fn exit_as_the_first_arm_of_a_string_effect_fn() {
    check(
        "match_string",
        "import process

effect fn label(n: Int) -> String = {
  let s = int.to_string(n)
  match n {
    0 => process.exit(4),
    _ => \"n=\" + s,
  }
}

effect fn main() -> Unit = {
  println(label(7)!)
  println(label(0)!)
}
",
        "n=7\n",
        4,
    );
}

#[test]
fn exit_as_the_then_branch_of_an_int_effect_fn() {
    check(
        "if_int",
        "import process

effect fn pick(n: Int) -> Int = if n == 0 then process.exit(5) else n * 2

effect fn main() -> Unit = {
  println(int.to_string(pick(4)!))
  println(int.to_string(pick(0)!))
}
",
        "8\n",
        5,
    );
}
