//! E089 (#2496): a `${…}` segment whose value has no defined string form is
//! rejected at check time instead of at rustc.
//!
//! Each rejected shape below used to pass `almide check` and die in the
//! generated Rust with `E0277 … doesn't implement std::fmt::Display` (or,
//! through a container, `… : AlmideRepr is not satisfied`), while both wasm
//! legs walled it — so nothing here ever printed on any leg, and the
//! rejection cannot break a program that built.
//!
//! The accepted half is the other side of the family gate: a type that DOES
//! render must keep rendering. `Bytes`, `Unit` and `Matrix` render inside a
//! container (`[[1, 2]]`, `some(())`, `[matrix.from_lists([[0]])]`) — they are
//! gaps at the top level only — and a generic body that interpolates its
//! parameter stays legal for every instantiation that has a string form.
//!
//! The concrete one-liners are also fixture families under
//! `tests/diagnostics/e089-*`; what lives here is what a fixture cannot say:
//! the instantiation chain, and the accepted column.

use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::canonicalize;
use almide::check::Checker;
use almide::diagnostic::Level;

/// The checker's errors for `input`, as `(code, message)`.
fn errors(input: &str) -> Vec<(String, String)> {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("the fixture parses");
    assert!(
        parser.errors.iter().all(|d| d.level != Level::Error),
        "the fixture must parse cleanly: {:?}",
        parser.errors
    );
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    checker
        .infer_program(&mut prog)
        .into_iter()
        .filter(|d| d.level == Level::Error)
        .map(|d| (d.code.unwrap_or_default().to_string(), d.message.clone()))
        .collect()
}

#[track_caller]
fn assert_e089(src: &str, wanted: &str) {
    let errs = errors(src);
    assert!(
        errs.iter().any(|(c, m)| c == "E089" && m.contains(wanted)),
        "expected an E089 containing {wanted:?}, got {errs:?}\nsource:\n{src}"
    );
}

#[track_caller]
fn assert_accepted(src: &str) {
    let errs = errors(src);
    assert!(errs.is_empty(), "unexpected rejection for\n{src}\ngot {errs:?}");
}

const SHOW: &str = "fn show[T](v: T) -> String = \"v=${v}\"\n";

/// The scalar gaps: each renders nowhere, and each names its own fix.
#[test]
fn a_value_with_no_string_form_is_rejected_at_the_segment() {
    assert_e089(
        "effect fn main() -> Unit = {\n  let u = ()\n  println(\"u=${u}\")\n}\n",
        "a Unit value",
    );
    // A CALL segment inside an effect fn takes a different route through the
    // checker (the implicit-propagation check owns its Result); it used to
    // skip the string-form rule entirely.
    assert_e089(
        "effect fn main() -> Unit = {\n  println(\"b=${bytes.from_list([1, 2])}\")\n}\n",
        "a Bytes value",
    );
    assert_e089(
        "fn noop() -> Unit = ()\n\neffect fn main() -> Unit = {\n  println(\"u=${noop()}\")\n}\n",
        "a Unit value",
    );
    assert_e089(
        "effect fn main() -> Unit = {\n  let b = bytes.from_list([1, 2])\n  println(\"b=${b}\")\n}\n",
        "a Bytes value",
    );
    assert_e089(
        "effect fn main() -> Unit = {\n  let m = matrix.zeros(1, 2)\n  println(\"m=${m}\")\n}\n",
        "a Matrix value",
    );
    assert_e089(
        "effect fn main() -> Unit = {\n  let b = bytes.from_list([1, 2])\n  let p = bytes.as_ptr(b)\n  println(\"p=${p}\")\n}\n",
        "a raw pointer",
    );
    assert_e089(
        "effect fn main() -> Unit = {\n  let f = (x: Int) => x + 1\n  println(\"f=${f}\")\n}\n",
        "a function value",
    );
}

/// A function value reached THROUGH a container, tuple, anonymous record, or
/// a named record / variant field: the repr would have to render it.
#[test]
fn a_value_holding_a_function_is_rejected_and_the_message_names_it() {
    for (src, wanted) in [
        ("effect fn main() -> Unit = {\n  let f = (x: Int) => x + 1\n  println(\"xs=${[f]}\")\n}\n", "it holds a function value"),
        ("effect fn main() -> Unit = {\n  let f = (x: Int) => x + 1\n  println(\"o=${some(f)}\")\n}\n", "it holds a function value"),
        ("effect fn main() -> Unit = {\n  let f = (x: Int) => x + 1\n  println(\"t=${(1, f)}\")\n}\n", "it holds a function value"),
        ("effect fn main() -> Unit = {\n  let f = (x: Int) => x + 1\n  println(\"m=${[\"a\": f]}\")\n}\n", "it holds a function value"),
        ("effect fn main() -> Unit = {\n  let f = (x: Int) => x + 1\n  println(\"r=${{ g: f, n: 1 }}\")\n}\n", "field `g`"),
        ("type H = { f: (Int) -> Int }\n\neffect fn main() -> Unit = {\n  let h = H { f: (x: Int) => x + 1 }\n  println(\"h=${h}\")\n}\n", "field `f`"),
        ("type V = | A((Int) -> Int) | B\n\neffect fn main() -> Unit = {\n  let v = B\n  println(\"v=${v}\")\n}\n", "case `A`"),
    ] {
        assert_e089(src, wanted);
    }
}

/// The generic body is checked once with `T` rigid, so the rule fires at the
/// CALL — naming both ends.
#[test]
fn a_generic_instantiated_with_an_unprintable_type_is_rejected_at_the_call() {
    for (arg, wanted) in [
        ("bytes.from_list([1, 2])", "makes it `Bytes`"),
        ("()", "makes it `Unit`"),
        ("matrix.zeros(1, 1)", "makes it `Matrix`"),
    ] {
        let src = format!("{SHOW}\neffect fn main() -> Unit = {{\n  println(show({arg}))\n}}\n");
        assert_e089(&src, wanted);
        assert!(
            errors(&src).iter().any(|(_, m)| m.contains("`show` interpolates")),
            "the message must name the generic whose body interpolates: {src}"
        );
    }
}

/// The requirement travels through intermediate generic calls, and the
/// message lists the hops it took.
#[test]
fn the_requirement_propagates_through_intermediate_generic_calls() {
    let two = format!(
        "{SHOW}\nfn wrap[U](u: U) -> String = show(u)\n\neffect fn main() -> Unit = {{\n  println(wrap(1))\n  println(wrap(bytes.from_list([1])))\n}}\n"
    );
    assert_e089(&two, "makes it `Bytes`");
    assert!(
        errors(&two).iter().any(|(_, m)| m.contains("through `show`")),
        "the message must name the hop through show: {:?}",
        errors(&two)
    );

    let three = format!(
        "{SHOW}\nfn wrap[U](u: U) -> String = show(u)\n\nfn wrap2[W](w: W) -> String = wrap(w)\n\neffect fn main() -> Unit = {{\n  println(wrap2(()))\n}}\n"
    );
    assert_e089(&three, "makes it `Unit`");
}

/// The other column: everything that renders must still check.
#[test]
fn types_that_do_render_are_still_accepted() {
    // The three top-level gaps render INSIDE a container.
    assert_accepted("effect fn main() -> Unit = {\n  let b = bytes.from_list([1, 2])\n  println(\"xs=${[b]}\")\n  println(\"o=${some(b)}\")\n}\n");
    assert_accepted("effect fn main() -> Unit = {\n  println(\"xs=${[(), ()]}\")\n  println(\"o=${some(())}\")\n  println(\"t=${(1, ())}\")\n}\n");
    assert_accepted("effect fn main() -> Unit = {\n  println(\"xs=${[matrix.zeros(1, 1)]}\")\n}\n");
    assert_accepted("type RB = { b: Bytes, u: Unit }\n\neffect fn main() -> Unit = {\n  let r = RB { b: bytes.from_list([1]), u: () }\n  println(\"r=${r}\")\n}\n");
    // Scalars, containers, records, variants.
    assert_accepted("type P = { n: Int, s: String }\ntype C = | On | Off\n\neffect fn main() -> Unit = {\n  println(\"${1} ${1.5} ${true} ${\"s\"} ${[1, 2]} ${[\"a\": 1]} ${(1, \"x\")} ${some(1)} ${P { n: 1, s: \"a\" }} ${On}\")\n}\n");
    // A generic body interpolating its parameter is legal — for every
    // instantiation that has a string form.
    assert_accepted(&format!("{SHOW}\neffect fn main() -> Unit = {{\n  println(show(1))\n  println(show(\"a\"))\n  println(show([1, 2]))\n  println(show(some(()))) \n}}\n"));
    // A generic over a CONTAINER of the parameter: Bytes renders there.
    assert_accepted("fn showl[T](xs: List[T]) -> String = \"v=${xs}\"\n\neffect fn main() -> Unit = {\n  println(showl([bytes.from_list([1])]))\n}\n");
}

/// A segment whose type never resolved is E025's business — E089 must not
/// stack a second error under it.
#[test]
fn an_undecidable_segment_stays_e025_alone() {
    let src = "fn main() -> Unit = {\n  let xs = []\n  println(\"${xs}\")\n}\n";
    let errs = errors(src);
    assert!(
        !errs.iter().any(|(c, _)| c == "E089"),
        "an unresolved slot must not also be E089: {errs:?}"
    );
}
