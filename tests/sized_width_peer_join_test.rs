//! Peer joins and return position obey the same sized-width rule (#880).
//!
//! `compatible` is DIRECTIONAL for the numeric widths (#867) and the operator
//! sites consult the AST for the literal exemption (#902) — but two families
//! were left joining symmetrically, so `check` accepted programs the native
//! build rejects:
//!
//!   1. A PEER JOIN (list elements, `if` / `match` arms, `assert_eq` args) took
//!      the FIRST peer's type, so `[1, u8v]` typed `List[Int]` while `[u8v, 1]`
//!      typed `List[UInt8]` — the same list, elements swapped. The `Int`
//!      reading emitted `vec![1i64, 3u8]` (rustc E0308).
//!   2. RETURN POSITION had no directional rule at all:
//!      `fn f(u: UInt8) -> Int = u` type-checked, ran on wasm (one i64 lane per
//!      scalar) and failed the native build.
//!
//! The rule both now follow: the SIZED peer wins the join, a canonical peer may
//! coerce into it only when it is a LITERAL, and a sized value never reaches a
//! canonical slot without an explicit widening.

use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::canonicalize;
use almide::check::Checker;
use almide::diagnostic::Level;

fn errors(input: &str) -> Vec<String> {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    checker
        .infer_program(&mut prog)
        .into_iter()
        .filter(|d| d.level == Level::Error)
        .map(|d| d.message)
        .collect()
}

fn assert_rejected(src: &str, needle: &str) {
    let errs = errors(src);
    assert!(
        errs.iter().any(|e| e.contains(needle)),
        "expected a rejection containing {needle:?}, got {errs:?}\nsource: {src}"
    );
}

fn assert_accepted(src: &str) {
    let errs = errors(src);
    assert!(errs.is_empty(), "unexpected rejection for {src}: {errs:?}");
}

/// The peer sets, with a canonical VALUE (not a literal) as one member.
#[test]
fn a_canonical_value_may_not_join_a_sized_peer() {
    assert_rejected(
        "fn main() -> Unit = {\n  let u: UInt8 = 3\n  let n = 5\n  let xs = [n, u]\n  println(int.to_string(list.len(xs)))\n}",
        "mixes sized numeric type UInt8",
    );
    // Either order: the join is symmetric in the members, not in their types.
    assert_rejected(
        "fn main() -> Unit = {\n  let u: UInt8 = 3\n  let n = 5\n  let xs = [u, n]\n  println(int.to_string(list.len(xs)))\n}",
        "mixes sized numeric type UInt8",
    );
    assert_rejected(
        "fn main() -> Unit = {\n  let u: UInt8 = 3\n  let n = 5\n  let v = if true then n else u\n  println(uint8.to_string(v))\n}",
        "mixes sized numeric type UInt8",
    );
    assert_rejected(
        "fn main() -> Unit = {\n  let u: UInt8 = 3\n  let n = 5\n  assert_eq(n, u)\n}",
        "mixes sized numeric type UInt8",
    );
}

/// A LITERAL peer is the case the coercion exists for: it adopts the sized
/// width at lowering, so the join settles on the sized member.
#[test]
fn a_literal_peer_still_joins_and_the_sized_width_wins() {
    assert_accepted(
        "fn main() -> Unit = {\n  let u: UInt8 = 3\n  let xs = [1, u]\n  println(int.to_string(list.len(xs)))\n}",
    );
    assert_accepted(
        "fn main() -> Unit = {\n  let u: UInt8 = 3\n  let v = if true then 1 else u\n  println(uint8.to_string(v))\n}",
    );
    assert_accepted(
        "fn main() -> Unit = {\n  let u: UInt8 = 3\n  assert_eq(3, u)\n}",
    );
}

/// `Int64` / `Float64` are the canonical types under another spelling — they
/// share the runtime representation, so mixing them with `Int` / `Float` is not
/// a width mistake and must not be rejected here.
#[test]
fn the_sixty_four_bit_spellings_are_not_a_mixed_join() {
    assert_accepted(
        "fn main() -> Unit = {\n  let a: Int64 = 3\n  let n = 5\n  let xs = [n, a]\n  println(int.to_string(list.len(xs)))\n}",
    );
}

/// Return position, the direction #867 already enforces on call args and
/// annotations: a sized VALUE never reaches a canonical `Int` slot.
#[test]
fn a_sized_value_may_not_be_returned_into_a_canonical_slot() {
    assert_rejected(
        "fn f(u: UInt8) -> Int = u",
        "explicit widening required",
    );
    assert_rejected(
        "fn f(x: Int32) -> Int = {\n  let y = x\n  y\n}",
        "explicit widening required",
    );
    // The explicit widening is the fix, and it type-checks.
    assert_accepted("fn f(u: UInt8) -> Int = int.from_uint8(u)");
    // A literal body has no width of its own — it adopts the declared one.
    assert_accepted("fn f() -> Int8 = 5");
    assert_accepted("fn f(u: UInt8) -> UInt8 = u");
    // `Int64` bridges the canonical slot freely (same representation).
    assert_accepted("fn f(x: Int64) -> Int = x");
}
