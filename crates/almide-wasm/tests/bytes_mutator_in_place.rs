//! The in-place bytes mutators cost O(range), not O(len) (#3641). Native
//! `copy_within` / `fill` / `clear` work through `&mut Vec<u8>`; the wasm
//! leg lowered them functionally — a fresh block of the receiver's length
//! and a full copy (or fill) per call, even for an empty range — so a loop
//! of `copy_within` over an n-byte buffer moved n bytes per call where
//! native moves none.
//!
//! The pin is RELATIVE: each shape is run once with the mutator and once
//! with `bytes.put_u8(b, 0, 1)` — a writer already in place — in its
//! position, and the two runs must allocate the same blocks and request
//! the same bytes. A unique receiver (a var, a `mut` parameter threaded
//! through a self tail call) is written in place; a SHARED one is still
//! copied once, so an alias keeps its bytes (value semantics) — the stdout
//! below is native's. What the position itself costs is not this pin's:
//! the self tail call boxes 8 bytes per call and `var a = acc` in a fold
//! callback copies the accumulator at its first write, for `set_u8` and
//! `list.set` alike.

mod harness;
use harness::run_wasm;

/// `(stdout, allocs, payload bytes requested)` of the armed run.
fn counted(src: &str) -> (String, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    let _guard = almide_wasm::alloc_count::CountGuard::set();
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.exit, 0, "the probe exits 0: {}", r.stdout);
    let c = r.alloc_count.expect("the armed module exports the counters");
    assert_eq!(c.live(), 0, "every block is released by exit");
    (r.stdout, c.allocs, c.bytes)
}

/// Each shape runs its mutator `{M}` 100 times over a 4096-byte buffer.
const SHAPES: &[(&str, &str, &str)] = &[
    (
        "copy_within on a var",
        "bytes.copy_within(b, 0, 2, 1)",
        r#"effect fn main() -> Unit = {
  var b = bytes.repeat(bytes.from_list([7, 9]), 2048)
  for i in 0..<100 {
    {M}
  }
  println("${bytes.len(b)}")
}
"#,
    ),
    (
        "an empty copy_within range",
        "bytes.copy_within(b, 3, 3, 0)",
        r#"effect fn main() -> Unit = {
  var b = bytes.repeat(bytes.from_list([7, 9]), 2048)
  for i in 0..<100 {
    {M}
  }
  println("${bytes.len(b)}")
}
"#,
    ),
    (
        "copy_within on a mut param through a self tail call",
        "bytes.copy_within(b, 0, 2, 1)",
        r#"fn go(mut b: Bytes, n: Int) -> Bytes =
  if n == 0 then b
  else {
    {M}
    go(b, n - 1)
  }

effect fn main() -> Unit = {
  var b = bytes.repeat(bytes.from_list([7, 9]), 2048)
  let r = go(b, 100)
  println("${bytes.len(r)}")
}
"#,
    ),
    (
        "copy_within on a fold accumulator",
        "bytes.copy_within(b, 0, 2, 1)",
        r#"fn step(acc: Bytes, x: Int) -> Bytes = {
  var b = acc
  {M}
  b
}

effect fn main() -> Unit = {
  let r = list.fold(list.range(0, 100), bytes.repeat(bytes.from_list([5]), 4096), (acc, x) => step(acc, x))
  println("${bytes.len(r)}")
}
"#,
    ),
    (
        "fill on a var",
        "bytes.fill(b, 3)",
        r#"effect fn main() -> Unit = {
  var b = bytes.repeat(bytes.from_list([7]), 4096)
  for i in 0..<100 {
    {M}
  }
  println("${bytes.get_or(b, 5, 0)}")
}
"#,
    ),
    (
        "clear then push on a var",
        "bytes.clear(b)\n    bytes.push(b, 1)",
        r#"effect fn main() -> Unit = {
  var b = bytes.repeat(bytes.from_list([7]), 16)
  for i in 0..<100 {
    {M}
  }
  println("${bytes.len(b)}")
}
"#,
    ),
];

#[test]
fn a_unique_receiver_is_mutated_in_place() {
    let mut failures = Vec::new();
    for (name, mutator, src) in SHAPES {
        let (_, a, b) = counted(&src.replace("{M}", mutator));
        let (_, ca, cb) = counted(&src.replace("{M}", "bytes.put_u8(b, 0, 1)"));
        if (a, b) != (ca, cb) {
            failures.push(format!(
                "{name}: {a} allocs / {b} bytes, against {ca} allocs / {cb} bytes for set_u8 in its place \
                 (an in-place mutator allocates nothing of its own)"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A shared receiver is copied (once — the copy is unique afterwards) and
/// the alias keeps its bytes; the values are native's.
const ALIASING: &str = r#"fn touch(mut b: Bytes) -> Unit = {
  bytes.copy_within(b, 1, 3, 0)
}

effect fn main() -> Unit = {
  var c = bytes.from_list([1, 2, 3, 4, 5, 6])
  let alias = c
  bytes.copy_within(c, 0, 3, 2)
  bytes.copy_within(c, 0, 1, 5)
  println("${bytes.to_list(alias)} ${bytes.to_list(c)}")
  var d = c
  bytes.fill(d, 9)
  println("${bytes.to_list(c)} ${bytes.to_list(d)}")
  var e = d
  bytes.clear(e)
  println("${bytes.to_list(d)} ${bytes.len(e)}")
  var g = c
  touch(g)
  println("${bytes.to_list(c)} ${bytes.to_list(g)}")
  var h = bytes.from_list([1, 2, 3, 4, 5, 6])
  bytes.copy_within(h, 0, 2, -1)
  bytes.copy_within(h, 4, 2, 0)
  bytes.copy_within(h, -3, 2, 0)
  bytes.copy_within(h, 0, 6, 1)
  bytes.copy_within(h, 1, 3, 9223372036854775807)
  println("${bytes.to_list(h)}")
  bytes.copy_within(h, 0, 100, 0)
  bytes.copy_within(h, 2, 6, 0)
  bytes.copy_within(h, 0, 4, 2)
  println("${bytes.to_list(h)}")
}
"#;

const ALIASING_NATIVE: &str = "\
[1, 2, 3, 4, 5, 6] [1, 2, 1, 2, 3, 1]
[1, 2, 1, 2, 3, 1] [9, 9, 9, 9, 9, 9]
[9, 9, 9, 9, 9, 9] 0
[1, 2, 1, 2, 3, 1] [2, 1, 1, 2, 3, 1]
[1, 2, 3, 4, 5, 6]
[3, 4, 3, 4, 5, 6]
";

#[test]
fn a_shared_receiver_is_copied_and_its_alias_keeps_its_bytes() {
    let (out, _, _) = counted(ALIASING);
    assert_eq!(out, ALIASING_NATIVE);
}
