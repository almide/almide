//! `list.flat_map` whose callback ends in a list literal of Int / Float
//! (#2980, src/list_flat.rs): the literal's elements are pushed straight into
//! the accumulator instead of through a chunk list. Each case runs on the wasm
//! leg and on the interpreter (the definition), and pins the expected text,
//! so a bug in both legs cannot agree silently.

mod harness;
use harness::run_wasm;

fn check(name: &str, src: &str, expected: &str) {
    let file = format!("{name}.almd");
    let ir = almide_spine::s5::lower_to_ir(&file, src).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("emits");
    let run = run_wasm(&bytes).expect("runs");
    assert_eq!(run.exit, 0, "{name}: wasm exited {}: {}", run.exit, run.stderr);
    let interp = almide_spine::s5::run_file(&file, src).expect("interp runs");
    assert_eq!(interp.exit, 0, "{name}: oracle run failed: {}", interp.stderr);
    assert_eq!(run.stdout, interp.stdout, "{name}: wasm and the interpreter disagree");
    assert_eq!(run.stdout, expected, "{name}: both legs agree on the wrong text");
}

/// A block body: its statements run, then the literal's elements, in order.
#[test]
fn a_block_body_with_a_float_literal_tail() {
    check(
        "fm_block_float",
        r#"fn main() -> Unit = {
  let k = 0.5
  let xs = list.range(0, 4) |> list.flat_map((i) => {
    let f = float.from_int(i)
    let g = f * k
    [f, g, f + g]
  })
  println("${xs}")
}
"#,
        "[0, 0, 0, 1, 0.5, 1.5, 2, 1, 3, 3, 1.5, 4.5]\n",
    );
}

/// A bare literal body of Int, an empty source, and a one-element literal.
#[test]
fn bare_int_literals_and_an_empty_source() {
    check(
        "fm_bare_int",
        r#"fn main() -> Unit = {
  let a = [1, 2, 3] |> list.flat_map((x) => [x, x * 10])
  let empty: List[Int] = []
  let b = empty |> list.flat_map((x) => [x, x])
  let c = [7, 8] |> list.flat_map((x) => [x + 1])
  println("${a}")
  println("${b}")
  println("${c}")
}
"#,
        "[1, 10, 2, 20, 3, 30]\n[]\n[8, 9]\n",
    );
}

/// The element expressions keep their order around an effect in the body,
/// and a non-literal tail (a captured list) still takes the chunk route.
#[test]
fn element_order_and_a_non_literal_tail() {
    check(
        "fm_order",
        r#"fn main() -> Unit = {
  let pair = [100, 200]
  let a = [1, 2] |> list.flat_map((x) => {
    let y = x * 3
    [y, y + 1, x]
  })
  let b = [1, 2] |> list.flat_map((_x) => pair)
  println("${a}")
  println("${b}")
}
"#,
        "[3, 4, 1, 6, 7, 2]\n[100, 200, 100, 200]\n",
    );
}
