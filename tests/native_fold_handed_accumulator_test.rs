//! A fold reached through a function value hands its owned accumulator on to
//! the callback on the native render (#4762).
//!
//! `bytes.fold(b, init, f)` forwards to `list.fold`, so the callback is a
//! closure VALUE there and the native adapter calls it through a dispatch
//! table. The runtime gives the adapter each accumulator owned, but the
//! adapter lent it to the table (`&__p0`), whose arm copied it back into the
//! owned param the lambda takes over (`AlmideString::from(a0)`): a whole
//! accumulator copy per element, O(n) per step where wasm is O(1). Found by
//! the 2026-11 stdlib re-inspection (`bytes.fold`, `deque.fold`). The adapter
//! now hands the owned value through the handing-over table.

fn shipped_v1(src: &str) -> String {
    almide_mir::pipeline::try_render_rust_source(src).unwrap_or_else(|e| panic!("the v1 render walled: {e:?}\n{src}"))
}

const PROGRAM: &str = r#"fn a0() -> Bytes = bytes.zeros(1)
effect fn main() -> Unit = {
  var x: String = string.repeat("ab", 8)
  var i = 0
  while i < 3 {
    x = bytes.fold(a0(), x, (p0, p1) => if p1 >= 0 then p0 else "c")
    i = i + 1
  }
  println("${string.len(x)}")
}
"#;

fn fold_calls(rust: &str) -> Vec<&str> {
    rust.lines().filter(|l| l.contains("almide_rt_list_fold") && l.contains("__almd_ci_")).collect()
}

/// The arms of the dispatch table `name`.
fn table<'a>(rust: &'a str, name: &str) -> &'a str {
    let at = rust.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("no table {name} in\n{rust}"));
    let rest = &rust[at..];
    &rest[..rest.find("\n}\n").map_or(rest.len(), |e| e + 2)]
}

#[test]
fn a_table_dispatched_fold_hands_its_accumulator_over() {
    let rust = shipped_v1(PROGRAM);
    let calls = fold_calls(&rust);
    assert!(!calls.is_empty(), "no table-dispatched fold in the render:\n{rust}");
    for call in calls {
        assert!(call.contains("__p0, __p1)"), "the adapter lends the owned accumulator to the table:\n{call}");
        let name = call.split("__almd_ci_").nth(1).and_then(|s| s.split('(').next()).expect("table name");
        let arms = table(&rust, &format!("__almd_ci_{name}"));
        assert!(!arms.contains("AlmideString::from(a0)"), "the table copies the accumulator back:\n{arms}");
    }
}

#[test]
fn a_lambda_that_only_reads_a_handed_value_is_lent_it() {
    // The negative side of the same table: an element the lambda only reads
    // is lent from the handed value (`&a1`), never copied.
    let src = PROGRAM.replace("bytes.fold(a0(), x, (p0, p1) => if p1 >= 0 then p0 else \"c\")", "string.fold(\"a\", x, (p0, p1) => if p1 >= \"\" then p0 else \"c\")");
    let rust = shipped_v1(&src);
    let calls = fold_calls(&rust);
    assert!(!calls.is_empty(), "no table-dispatched fold in the render:\n{rust}");
    for call in calls {
        let name = call.split("__almd_ci_").nth(1).and_then(|s| s.split('(').next()).expect("table name");
        let arms = table(&rust, &format!("__almd_ci_{name}"));
        assert!(arms.contains(", a0, &a1)"), "the handed element is not lent to the reading lambda:\n{arms}");
    }
}
