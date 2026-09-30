//! #2992 — a top-level `var` releases the value it is rebound away from.
//!
//! A global holds ONE credit on its occupant for the program's life. Before
//! this change the structural leg never spent it: `g = …` rebound the
//! global and kept the old block, and every copy-on-write field write
//! (`g.n = …`, `list.push(g.xs, …)`, `string.push(g.s, …)`) did the same
//! with the old record, so a loop that rewrote a global grew by a block or
//! more per iteration. Field writes on a LOCAL record leaked the old record
//! too; `list.push(h.f, v)` also leaked its one-element operand.
//!
//! Each row runs the loop at N=10 and N=1000 under the allocation counter
//! and pins the blocks still live at exit (`allocs - frees`) to the SAME
//! number at both sizes — what a global still holds at exit, never a count
//! that grows with N. The expected stdout is native's, measured on the same
//! programs (`almide run` vs `almide run --target wasm`).

mod harness;
use harness::run_wasm;

fn live_and_stdout(src: &str) -> (u64, String) {
    let ir = almide_spine::s5::lower_to_ir("global_reassign.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    let c = out.alloc_count.expect("the armed module exports the counters");
    (c.allocs - c.frees, out.stdout)
}

/// `program(n)` at two sizes: the same stdout as native, and a live count
/// that does not move with N.
fn flat(name: &str, program: impl Fn(u32) -> String, expect_10: &str, expect_1000: &str) {
    let (l10, o10) = live_and_stdout(&program(10));
    let (l1000, o1000) = live_and_stdout(&program(1000));
    assert_eq!(o10, expect_10, "{name}: stdout at N=10");
    assert_eq!(o1000, expect_1000, "{name}: stdout at N=1000");
    assert_eq!(l10, l1000, "{name}: blocks live at exit must not grow with N (N=10: {l10}, N=1000: {l1000})");
}

const REC: &str = "type H = { n: Int, s: String, xs: List[Int] }\n";

fn global_loop(n: u32, decl: &str, body: &str, show: &str) -> String {
    format!("{REC}{decl}\neffect fn main() -> Unit = {{\n  for i in 0..<{n} {{\n{body}\n  }}\n  println({show})\n}}\n")
}

fn local_loop(n: u32, decl: &str, body: &str, show: &str) -> String {
    format!("{REC}effect fn main() -> Unit = {{\n  {decl}\n  for i in 0..<{n} {{\n{body}\n  }}\n  println({show})\n}}\n")
}

const G_REC: &str = r#"var g: H = { n: 0, s: "a", xs: [] }"#;
const SHOW_REC: &str = r#""${g.n} ${g.s} ${list.len(g.xs)}""#;

#[test]
fn reassigning_a_global_list_releases_the_old_one() {
    flat(
        "global List",
        |n| global_loop(n, "var g: List[Int] = [0]", "    g = [i, i + 1]", r#""${list.len(g)} ${list.get(g, 0) ?? -1}""#),
        "2 9\n",
        "2 999\n",
    );
}

#[test]
fn reassigning_a_global_string_releases_the_old_one() {
    flat(
        "global String",
        |n| global_loop(n, r#"var g: String = "start""#, r#"    g = "k" + int.to_string(i)"#, "g"),
        "k9\n",
        "k999\n",
    );
}

#[test]
fn reassigning_a_global_record_releases_the_old_one_and_its_fields() {
    flat(
        "global record",
        |n| global_loop(n, G_REC, r#"    g = { n: i, s: "s" + int.to_string(i), xs: [i] }"#, SHOW_REC),
        "9 s9 1\n",
        "999 s999 1\n",
    );
    flat("global record spread", |n| global_loop(n, G_REC, "    g = { ...g, n: i }", SHOW_REC), "9 a 0\n", "999 a 0\n");
}

/// The share guard: an rhs that aliases the old occupant keeps it alive —
/// a local still holding it, the same block handed back by a call, a
/// branch that keeps it.
#[test]
fn an_aliased_occupant_survives_its_replacement() {
    flat(
        "alias kept by a local",
        |n| {
            global_loop(
                n,
                "var g: List[Int] = [0]\nvar keep: List[Int] = []",
                "    let old = g\n    g = old + [i]\n    keep = old",
                r#""${list.len(g)} ${list.len(keep)}""#,
            )
        },
        "11 10\n",
        "1001 1000\n",
    );
    flat(
        "same block back",
        |n| {
            global_loop(
                n,
                "var g: List[Int] = [0]\nfn same(xs: List[Int]) -> List[Int] = xs",
                "    g = same(g)\n    g = if i % 2 == 0 then g else [i]",
                r#""${list.len(g)}""#,
            )
        },
        "1\n",
        "1\n",
    );
}

/// A global initialized from another global takes its own credit, so
/// replacing it releases only its share.
#[test]
fn a_global_initialized_from_another_owns_its_share() {
    flat(
        "init from a global",
        |n| global_loop(n, "var a: List[Int] = [1, 2]\nvar g: List[Int] = a", "    g = [i]", r#""${list.len(a)} ${list.len(g)}""#),
        "2 1\n",
        "2 1\n",
    );
}

/// Every copy-on-write field write releases the record it replaces, on a
/// global and on a local alike.
#[test]
fn a_field_write_releases_the_record_it_replaces() {
    for (stmt, e10, e1000) in [
        ("    g.n = i", "9 a 0\n", "999 a 0\n"),
        (r#"    g.s = "t" + int.to_string(i)"#, "0 t9 0\n", "0 t999 0\n"),
        ("    g.xs = [i]", "0 a 1\n", "0 a 1\n"),
    ] {
        flat(&format!("global {stmt}"), |n| global_loop(n, G_REC, stmt, SHOW_REC), e10, e1000);
        flat(
            &format!("local {stmt}"),
            |n| local_loop(n, G_REC, stmt, SHOW_REC),
            e10,
            e1000,
        );
    }
}

/// `list.push` / `string.push` on a record FIELD — the copy-on-write
/// rebind, which also leaked `list.push`'s one-element operand. (Native
/// drops these writes on a GLOBAL's field — a separate defect — so the
/// global rows pin the local twin's answer, which native gives.)
#[test]
fn a_mut_op_on_a_field_releases_the_record_and_its_operand() {
    let push = "    list.push(g.xs, i)";
    flat("local list.push field", |n| local_loop(n, G_REC, push, SHOW_REC), "0 a 10\n", "0 a 1000\n");
    flat("global list.push field", |n| global_loop(n, G_REC, push, SHOW_REC), "0 a 10\n", "0 a 1000\n");
    let spush = r#"    string.push(g.s, "b")"#;
    let (b10, b1000) = (format!("0 a{} 0\n", "b".repeat(10)), format!("0 a{} 0\n", "b".repeat(1000)));
    flat("local string.push field", |n| local_loop(n, G_REC, spush, SHOW_REC), &b10, &b1000);
    flat("global string.push field", |n| global_loop(n, G_REC, spush, SHOW_REC), &b10, &b1000);
}
