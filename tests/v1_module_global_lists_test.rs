//! Module-level globals and list literals on the certified native render
//! (#4640).
//!
//! A reference to a list global builds a copy of its initializer. The copy
//! was built for Int elements and for String elements only, so a `[1.0, …]`
//! or `[true, …]` global refused every fn that read it; it now builds as the
//! same literal bound to a local does. And a list literal took a bound var
//! as an element but not an immutable global, so `["a", NAME]` was refused;
//! the element now takes the global's copy as any other reference does.
use std::process::Command;

/// The program on the native leg under the ownership certificate with no
/// fallback to the standard codegen, and on wasm: both print `expected`.
fn renders_and_agrees(name: &str, source: &str, expected: &str) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, source).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "{name} {extra:?}");
    }
}

/// A Float list and a Bool list: mapped, indexed, used as a condition, measured.
const LITERAL_GLOBALS: &str = r##"let IDENT = [1.0, 0.0, 0.0, 0.5]
let FLAGS = [true, false, true]

fn scale(k: Float) -> List[Float] = IDENT |> list.map((x) => x * k)

fn pick(i: Int) -> Int = if FLAGS[i] then 1 else 2

effect fn main() -> Unit = {
  println(scale(2.0) |> list.map((x) => float.to_string(x)) |> list.join(","))
  println(float.to_string(IDENT[3] + IDENT[0]))
  println("${pick(0)}${pick(1)}${pick(2)} ${list.len(FLAGS)} ${list.len(IDENT)}")
  for i in 0..<3 { println(if FLAGS[i] then "t" else "f") }
}
"##;

const LITERAL_GLOBALS_STDOUT: &str = "2.0,0.0,0.0,1.0\n1.5\n121 3 4\nt\nf\nt\n";

/// String and list globals as elements: a returned literal, beside a param, nested, in an arm, in a loop.
const GLOBAL_ELEMENTS: &str = r##"let ST_NOT_RUN: String = "not-run"
let ST_HARNESS: String = "harness"
let NUMS = [1, 2]

fn category_order() -> List[String] = [
  "pass-1shot", "pass-retry",
  ST_NOT_RUN, ST_HARNESS,
  "parse-error", "unknown",
]

fn pair(tag: String) -> List[String] = [ST_NOT_RUN, tag, ST_NOT_RUN]

fn nested() -> List[List[Int]] = [NUMS, [3], NUMS]

fn in_arm(flag: Bool) -> List[String] = if flag then [ST_HARNESS, "x"] else ["y", ST_NOT_RUN]

effect fn main() -> Unit = {
  println(list.join(category_order(), ","))
  println(list.join(pair("t"), ","))
  println(nested() |> list.map((xs) => int.to_string(list.len(xs))) |> list.join(","))
  println(list.join(in_arm(true), ",") + " " + list.join(in_arm(false), ","))
  for i in 0..<2 { println(list.join([ST_NOT_RUN, int.to_string(i)], "-")) }
}
"##;

const GLOBAL_ELEMENTS_STDOUT: &str = "pass-1shot,pass-retry,not-run,harness,parse-error,unknown\nnot-run,t,not-run\n2,1,2\nharness,x y,not-run\nnot-run-0\nnot-run-1\n";

#[test]
fn a_float_or_bool_literal_list_global_is_read_from_every_position() {
    renders_and_agrees("literal_globals", LITERAL_GLOBALS, LITERAL_GLOBALS_STDOUT);
}

#[test]
fn a_list_literal_takes_an_immutable_global_as_an_element() {
    renders_and_agrees("global_elements", GLOBAL_ELEMENTS, GLOBAL_ELEMENTS_STDOUT);
}
