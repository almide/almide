//! A narrow sized-int VALUE where `Int` is expected must be rejected by the
//! checker (#867). It used to be accepted — `compatible`'s literal-coercion
//! rule was symmetric — and the emitted Rust then failed with E0308
//! (`expected i64, found i32`), breaking the check-accepted-must-build
//! invariant for the whole `<narrow>.to_<narrower-than-i64>()` family.
//!
//! The rule is directional on purpose:
//! - INTO a sized slot stays open — that is how an `Int`-typed literal is
//!   contextually typed (`let x: Int32 = 42`).
//! - OUT of a sized value into `Int`/`Int64` is closed — no literal ever
//!   types that direction, so it is always a real value that needs the
//!   explicit, lossless `.to_int64()`.
//! - `Int` ↔ `Int64` (and `Float` ↔ `Float64`) bridge freely: same width,
//!   same runtime repr.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("narrow.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

const REJECTED: &[&str] = &[
    // The filed repro: an Int32-returning conversion passed to an Int slot.
    "effect fn main() -> Unit = {\n  let u: UInt8 = 200\n  println(int.to_string(u.to_int32()))\n}",
    // Same family, other widths.
    "effect fn main() -> Unit = {\n  let u: UInt16 = 9\n  println(int.to_string(u.to_int8()))\n}",
    // Float sibling: a Float32 value where Float is expected.
    "effect fn main() -> Unit = {\n  let f: Float32 = 1.5\n  println(float.to_string(f))\n}",
    // Annotation position — the same direction stated as a binding.
    "effect fn main() -> Unit = {\n  let u: UInt8 = 200\n  let a: Int = u.to_int32()\n  println(int.to_string(a))\n}",
    "effect fn main() -> Unit = {\n  let f: Float32 = 1.5\n  var g: Float = f\n  g = 2.0\n  println(float.to_string(g))\n}",
];

const ACCEPTED: &[&str] = &[
    // The documented idiom: widen explicitly first.
    "effect fn main() -> Unit = {\n  let u: UInt8 = 200\n  println(int.to_string(u.to_int64()))\n}",
    // Literal coercion into sized slots must keep working.
    "fn f(x: Int32) -> Int32 = x\n\neffect fn main() -> Unit = {\n  println(int32.to_string(f(5)))\n}",
    // Int ↔ Int64 same-width bridging.
    "effect fn main() -> Unit = {\n  let u: UInt8 = 7\n  let w: Int64 = u.to_int64()\n  println(int.to_string(w))\n}",
    // Sized comparisons against literals, both operand orders.
    "effect fn main() -> Unit = {\n  let u: UInt8 = 3\n  if u == 3 then println(\"a\") else println(\"b\")\n  assert_eq(u, 3)\n}",
    // Annotated literal bindings — the coercion direction that must stay open.
    "effect fn main() -> Unit = {\n  let a: Int32 = 42\n  let b: UInt64 = 100\n  let f: Float32 = 1.5\n  println(int32.to_string(a))\n}",
];

#[test]
fn narrow_sized_value_into_int_slot_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    for src in REJECTED {
        let out = check(dir.path(), src);
        assert!(
            out.contains("error["),
            "must be rejected by check (it cannot build):\n{src}\ngot:\n{out}"
        );
        assert!(
            out.contains("to_int64") || out.contains("to_float64"),
            "the hint must name the explicit widening idiom:\n{src}\ngot:\n{out}"
        );
    }
}

#[test]
fn literal_coercion_and_explicit_widening_still_check() {
    let dir = tempfile::tempdir().expect("tempdir");
    for src in ACCEPTED {
        let out = check(dir.path(), src);
        assert!(
            !out.contains("error["),
            "must stay accepted:\n{src}\ngot:\n{out}"
        );
    }
}
