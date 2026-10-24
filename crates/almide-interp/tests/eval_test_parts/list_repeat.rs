// ── eval_test.rs, part 5: list.repeat over every element class ──
//
// include!-spliced into `tests/eval_test.rs`. `list.repeat` resolved to the
// Int-declared registry core (`list_repeat`, stdlib/list_make.almd), whose
// `prim.store64` refused a Float element as "a non-address" — so every fixture
// that built a zero row with `list.repeat(0.0, n)` left the oracle
// (matrix_dims_guard, module_var_alias_cow, matrix_packed_weight_family). The
// op is element-type-blind, so it is served value-level like `chunk`; the
// expected lines are native's (measured first, per crates/almide-interp/CLAUDE.md).

#[test]
fn list_repeat_over_every_element_class() {
    let src = r#"fn main() -> Unit = {
  println("${list.repeat(0.0, 3)}")
  println("${list.repeat(-1.5, 2)}")
  println("${list.repeat(7, 2)}")
  println("${list.repeat(true, 2)}")
  println("${list.repeat("ab", 2)}")
  println("${list.repeat([1, 2], 2)}")
  println("${list.len(list.repeat(0.0, 0))}")
  println("${list.len(list.repeat(0.0, -4))}")
  let row = list.repeat(0.0, 2) + [1.0]
  println("${row}")
}
"#;
    let (code, out, err) = run(src);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        out.trim(),
        "[0, 0, 0]\n[-1.5, -1.5]\n[7, 7]\n[true, true]\n[\"ab\", \"ab\"]\n[[1, 2], [1, 2]]\n0\n0\n[0, 0, 1]"
    );
}

#[test]
fn list_repeat_over_the_ceiling_aborts() {
    let src = r#"fn main() -> Unit = {
  println("${list.len(list.repeat(0.0, 268435457))}")
}
"#;
    let (code, _out, err) = run(src);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("repeat result too large"), "{err}");
}

// `bytes.repeat` joined the repeat family's ceiling (#4154): the interp runs
// the self-hosted `bytes_repeat` body, so this pins that its guard aborts with
// the native line rather than materializing a 2 GiB buffer.
#[test]
fn bytes_repeat_over_the_ceiling_aborts() {
    let src = r#"import bytes
fn main() -> Unit = {
  println("${bytes.len(bytes.repeat(bytes.from_list([1, 2]), 3))}")
  println("${bytes.len(bytes.repeat(bytes.from_list([1, 2]), 1073741825))}")
}
"#;
    let (code, out, err) = run(src);
    assert_eq!(code, 1, "{err}");
    assert_eq!(out.trim(), "6");
    assert!(err.contains("repeat result too large"), "{err}");
}
