//! #4261 — appending to a field of a rebuilt record, or to a fold
//! accumulator, extends the collection in place instead of copying it.
//!
//! `r = { ...r, xs: r.xs + [i], n: r.n + 1 }`, `r = { ...r, s: r.s + "x" }`,
//! `list.fold(xs, R { … }, (acc, i) => { ...acc, xs: acc.xs + [i] })` and
//! `list.fold(xs, "", (acc, i) => acc + "x")` copied the whole list or string
//! on every step (4.1 s for 160 000 list appends). A dying spread base now
//! moves the field the append reads out of its slot, and the append consumes
//! that owned value (`$cow` + `$list_push`, `$str_append`: in place at rc 1);
//! a dying String accumulator goes through `$str_append` the same way
//! (concat_dying.rs).
//!
//! Each row runs at N=250 and N=2000 under the allocation counter and pins
//! the growth of the bytes requested to the linear bound, as
//! record_map_field_update.rs does for #3406: 8x the appends may request at
//! most 12x the bytes. A copy per step is quadratic (64x).

mod harness;
use harness::run_wasm;

fn run(src: &str) -> (u64, String) {
    let ir = almide_spine::s5::lower_to_ir("builder_append_linear.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (out.alloc_count.expect("the armed module exports the counters").bytes, out.stdout)
}

const DECL: &str = "type R = { xs: List[Int], s: String, m: Map[Int, Int], n: Int }";

fn linear(name: &str, body: &str, show: &str, (expect_250, expect_2000): (&str, &str)) {
    let src = |n: u32| format!("{DECL}\nfn main() -> Unit = {{\n  let n = {n}\n  {body}\n  println({show})\n}}\n");
    let (h250, o250) = run(&src(250));
    let (h2000, o2000) = run(&src(2000));
    assert_eq!(o250, expect_250, "{name}: stdout at N=250");
    assert_eq!(o2000, expect_2000, "{name}: stdout at N=2000");
    assert!(
        h2000 < 12 * h250,
        "{name}: 8x the appends requested {}x the bytes ({h250} -> {h2000}) — the append copies the collection",
        h2000 / h250.max(1)
    );
}

const VAR_R: &str = "var r = R { xs: [], s: \"\", m: [:], n: 0 }";

#[test]
fn a_list_field_of_a_rebuilt_record_is_linear() {
    linear(
        "list field",
        &format!("{VAR_R}\n  for i in 0..<n {{\n    r = {{ ...r, xs: r.xs + [i], n: r.n + 1 }}\n  }}"),
        r#""${list.len(r.xs)} ${r.n}""#,
        ("250 250\n", "2000 2000\n"),
    );
}

#[test]
fn a_string_field_of_a_rebuilt_record_is_linear() {
    linear(
        "string field",
        &format!("{VAR_R}\n  for i in 0..<n {{\n    r = {{ ...r, s: r.s + \"xy\", n: r.n + 1 }}\n  }}"),
        r#""${string.len(r.s)} ${r.n}""#,
        ("500 250\n", "4000 2000\n"),
    );
}

#[test]
fn a_map_field_of_a_rebuilt_record_is_linear() {
    linear(
        "map field",
        &format!("{VAR_R}\n  for i in 0..<n {{\n    r = {{ ...r, m: map.set(r.m, i, i), n: r.n + 1 }}\n  }}"),
        r#""${map.len(r.m)} ${r.n}""#,
        ("250 250\n", "2000 2000\n"),
    );
}

#[test]
fn a_fold_record_accumulator_is_linear() {
    linear(
        "fold record",
        "let r = list.fold(list.range(0, n), R { xs: [], s: \"\", m: [:], n: 0 }, (acc, i) => { ...acc, xs: acc.xs + [i], s: acc.s + \"z\" })",
        r#""${list.len(r.xs)} ${string.len(r.s)}""#,
        ("250 250\n", "2000 2000\n"),
    );
}

#[test]
fn a_fold_string_accumulator_is_linear() {
    linear(
        "fold string",
        "let s = list.fold(list.range(0, n), \"\", (acc, i) => acc + \"ab\")",
        r#""${string.len(s)}""#,
        ("500\n", "4000\n"),
    );
}
