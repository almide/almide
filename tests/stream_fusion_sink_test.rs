//! #3584 / #3585, native leg: the two reasons spectralnorm's `enumerate`
//! spelling (`v |> list.enumerate |> list.map(…) |> fsum`) ran 1.55x its
//! imperative twin.
//!
//! - #3584: a chain ending in a call to a user fn whose whole body is one
//!   chain over its only parameter (`fsum`, `total`) collected a `Vec` and
//!   passed it, where the same fold written inline fused. The sink's chain
//!   now continues the argument's, under the same merge rule.
//! - #3585: an `enumerate`-sourced chain rendered its list param owned, so
//!   every caller cloned the list. A `Copy` element needs no ownership
//!   whatever the adapters between it and a lambda.

use std::process::Command;

fn almide(dir: &std::path::Path, program: &str, args: &[&str]) -> String {
    let source = dir.join("sink.almd");
    std::fs::write(&source, program).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args.iter().copied().chain([source.to_str().expect("path")]).collect::<Vec<_>>())
        .args(if args.is_empty() { &["--target", "rust"][..] } else { &[][..] })
        .output()
        .expect("almide");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn emit(program: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    almide(dir.path(), program, &[])
}

fn run(program: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    almide(dir.path(), program, &["run"])
}

/// The emitted body of one function, from its signature to the closing brace
/// in column 0.
fn emitted_fn(rust: &str, name: &str) -> String {
    let head = format!("fn {name}(");
    let mut lines = rust.lines().skip_while(|l| !l.contains(&head));
    let first = lines.next().unwrap_or_else(|| panic!("no fn {name} in the emit:\n{rust}"));
    std::iter::once(first)
        .chain(lines.take_while(|l| *l != "}"))
        .collect::<Vec<_>>()
        .join("\n")
}

const SINK: &str = r#"
fn total(xs: List[Float]) -> Float = xs |> list.fold(0.0, (a, x) => a + x)

fn row_sink(v: List[Float]) -> Float = v |> list.map((x) => x * 2.0) |> total

fn row_enum(v: List[Float], i: Int) -> Float =
  v |> list.enumerate |> list.map(((j, x)) => int.to_float(j + i) * x) |> total

fn rows(v: List[Float], n: Int) -> List[Float] = list.range(0, n) |> list.map((i) => row_sink(v) + row_enum(v, i))

fn main() -> Unit = {
  let v = [1.0, 2.0, 3.0]
  println("${rows(v, 3)}")
}
"#;

#[test]
fn a_chain_into_a_user_fn_sink_fuses_through_the_call() {
    let rust = emit(SINK);
    for f in ["row_sink", "row_enum"] {
        let body = emitted_fn(&rust, f);
        assert!(!body.contains("collect"), "{f} still materializes before the sink:\n{body}");
        assert!(!body.contains("total("), "{f} still calls the sink:\n{body}");
        assert!(body.contains(".fold(0f64"), "{f} lost the sink's fold:\n{body}");
    }
    // The sink itself is still emitted for any other caller.
    assert!(emitted_fn(&rust, "total").contains(".fold("));
}

#[test]
fn an_enumerate_source_over_copy_elements_is_borrowed() {
    let rust = emit(SINK);
    let row = emitted_fn(&rust, "row_enum");
    assert!(row.contains("v: &[f64]"), "the enumerate source param is owned:\n{row}");
    assert!(row.contains(".iter().cloned().enumerate()"), "{row}");
    let rows = emitted_fn(&rust, "rows");
    assert!(!rows.contains("to_vec()") && !rows.contains(".clone()"), "a caller still copies the list:\n{rows}");
}

#[test]
fn the_fused_sink_computes_what_the_call_did() {
    // row_sink = 12; row_enum(v, i) = sum((j + i) * v[j]) = 8 + 6i.
    assert_eq!(run(SINK).trim(), "[20, 26, 32]");
}

/// Not a sink: the parameter is read twice, so substituting the argument
/// chain for it would run the argument twice.
const TWICE: &str = r#"
fn mean(xs: List[Float]) -> Float = (xs |> list.fold(0.0, (a, x) => a + x)) / int.to_float(list.len(xs))

fn f(v: List[Float]) -> Float = v |> list.map((x) => x * 2.0) |> mean

fn main() -> Unit = println("${f([1.0, 2.0, 3.0])}")
"#;

#[test]
fn a_fn_that_reads_its_param_twice_is_not_a_sink() {
    let rust = emit(TWICE);
    let body = emitted_fn(&rust, "f");
    assert!(body.contains("mean(") && body.contains("collect"), "{body}");
    assert_eq!(run(TWICE).trim(), "4");
}

/// A sink whose stage can abort, after an argument stage that can abort too:
/// fusing would decide which abort fires first per element instead of per
/// stage, so `merge` refuses and the call stays.
const TWO_ABORTS: &str = r#"
fn inv_sum(xs: List[Int]) -> Int = xs |> list.fold(0, (a, x) => a + 100 / x)

fn g(v: List[Int]) -> Int = v |> list.map((x) => 60 / x) |> inv_sum

fn main() -> Unit = println("${g([1, 2, 3])}")
"#;

#[test]
fn a_sink_the_merge_rule_refuses_stays_a_call() {
    let rust = emit(TWO_ABORTS);
    let body = emitted_fn(&rust, "g");
    assert!(body.contains("inv_sum(") && body.contains("collect"), "{body}");
    assert_eq!(run(TWO_ABORTS).trim(), "9");
}

/// A heap element through `enumerate` still runs correctly (its ownership is
/// decided by the receivers as before; only `Copy` elements short-cut).
const HEAP_ENUM: &str = r#"
fn joined(xs: List[String]) -> String = xs |> list.fold("", (a, s) => a + s)

fn h(v: List[String]) -> String = v |> list.enumerate |> list.map(((i, s)) => "${i}${s}") |> joined

fn main() -> Unit = println(h(["a", "b"]))
"#;

#[test]
fn a_heap_enumerate_chain_into_a_sink_runs() {
    assert_eq!(run(HEAP_ENUM).trim(), "0a1b");
}
