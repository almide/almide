//! A `mut` buffer parameter whose function also RETURNS a heap value stays
//! linear on the wasm leg.
//!
//! The C-132 write-back lowers `let (r, s) = f(out, …)` to `let t = f(out, …);
//! let (ret, buf) = t; out = buf`, with `ret` and `buf` views of the result
//! tuple `t`. The write-back shares `buf` into `out`, and `t` keeps its own
//! credit on the buffer until the frame or the next rebind releases it. When
//! every other slot is a scalar the block walk releases `t` right after the
//! write-back (#3337, `settle_subject`); a heap `ret` (a tuple, a String, a
//! record) is a view `t` must keep alive, so `t` lived on — and the next call
//! met the buffer at rc 2 and copied all of it. One copy of the whole output
//! per call: the self-hosted zlib inflater (one call per DEFLATE block)
//! requested 7 bytes per output byte at 5 blocks.
//!
//! The bytes requested must be linear in the number of calls for every
//! return shape, every module must balance, and an alias of the buffer taken
//! before a call must keep its content.

mod harness;
use harness::run_wasm;

fn counts(src: &str) -> (String, u64, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("mut_writeback_heap_return.almd", src).expect("front");
    let bytes = {
        let _count = almide_wasm::alloc_count::CountGuard::set();
        let _canonical = almide_wasm::canonical_alloc::CanonicalGuard::set();
        almide_wasm::emit_program(&ir).expect("emit")
    };
    let r = run_wasm(&bytes).expect("run");
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, c.bytes, c.allocs, c.frees)
}

/// `fill` pushes `n` bytes into its `mut` buffer and answers `ret`.
const SHAPES: &[(&str, &str, &str, &str)] = &[
    // name, return type, return value, how the caller binds it
    ("Int", "Int", "k", "let r = fill(out, 4096)\n    acc = acc + r"),
    ("Unit", "Unit", "()", "fill(out, 4096)\n    acc = acc + 1"),
    ("tuple", "(Int, Int)", "(k, 0)", "let (r, _) = fill(out, 4096)\n    acc = acc + r"),
    ("String", "String", "\"x${k}\"", "let r = fill(out, 4096)\n    acc = acc + string.len(r)"),
    ("record", "Rec", "Rec { n: k, s: \"s${k}\" }", "let r = fill(out, 4096)\n    acc = acc + r.n + string.len(r.s)"),
];

fn program(ret_ty: &str, ret: &str, call: &str, blocks: u32) -> String {
    format!(
        "type Rec = {{ n: Int, s: String }}

fn fill(mut out: Bytes, n: Int) -> {ret_ty} = {{
  var k = 0
  while k < n {{
    bytes.push(out, 7)
    k = k + 1
  }}
  {ret}
}}

fn run(nb: Int) -> Int = {{
  var out = bytes.zeros(0)
  var acc = 0
  var i = 0
  while i < nb {{
    {call}
    i = i + 1
  }}
  bytes.len(out) + acc * 0
}}

fn main() -> Unit = {{
  println(\"${{run({blocks})}}\")
}}
"
    )
}

#[test]
fn a_mut_buffer_call_is_linear_in_bytes_whatever_it_returns() {
    let mut bad = Vec::new();
    for &(name, ret_ty, ret, call) in SHAPES {
        let mut series = Vec::new();
        for blocks in [1u32, 2, 4, 8] {
            let (out, bytes, allocs, frees) = counts(&program(ret_ty, ret, call, blocks));
            assert_eq!(out, format!("{}\n", 4096 * blocks), "{name} blocks={blocks}");
            assert_eq!(allocs, frees, "{name} blocks={blocks}: leaked {} block(s)", allocs.abs_diff(frees));
            series.push(bytes);
        }
        // A doubling buffer requests ~2x per doubling of the output; a copy
        // of the whole buffer per call adds Σ len — 15x at 8 blocks.
        if series[3] * 4 > series[0] * 8 * 5 {
            bad.push(format!("{name}: {series:?} ({:.1}x at 8x the calls)", series[3] as f64 / series[0] as f64));
        }
    }
    assert!(bad.is_empty(), "a `mut` buffer call copies the buffer per call:\n{}", bad.join("\n"));
}

/// Value semantics: an alias taken before each call keeps the old content,
/// in the caller and in the callee, whatever the call returns.
const ALIAS: &str = r#"fn fill(mut out: Bytes, n: Int) -> (Int, String) = {
  let before = out
  var k = 0
  while k < n {
    bytes.push(out, 7)
    k = k + 1
  }
  (bytes.len(before), "s")
}

fn main() -> Unit = {
  var out = bytes.zeros(0)
  var log = ""
  var i = 0
  while i < 3 {
    let old = out
    let (r, s) = fill(out, 4)
    log = log + "${bytes.len(old)}/${bytes.len(out)}/${r}${s} "
    i = i + 1
  }
  println(log)
}
"#;

#[test]
fn an_alias_taken_before_the_call_keeps_its_content() {
    let (out, _, allocs, frees) = counts(ALIAS);
    assert_eq!(out, "0/4/0s 4/8/4s 8/12/8s \n");
    assert_eq!(allocs, frees, "leaked {} block(s)", allocs.abs_diff(frees));
}

/// The returned heap value may itself hold the buffer (a tuple or a record
/// carrying `out`): it keeps the content it had when it was returned, and the
/// buffer's next push copies rather than writing under it.
const RETURNED_ALIAS: &str = r#"type Box = { b: Bytes, n: Int }

fn grab(mut out: Bytes, v: Int) -> (Bytes, Int) = {
  bytes.push(out, v)
  (out, bytes.len(out))
}

fn boxed(mut out: Bytes, v: Int) -> Box = {
  bytes.push(out, v)
  Box { b: out, n: v }
}

fn main() -> Unit = {
  var out = bytes.zeros(0)
  var keep: List[Bytes] = []
  var i = 0
  while i < 3 {
    let (snap, _) = grab(out, i)
    keep = keep + [snap]
    let bx = boxed(out, 10 + i)
    keep = keep + [bx.b]
    i = i + 1
  }
  println("${keep |> list.map((k) => bytes.len(k))} ${bytes.to_list(out)}")
}
"#;

#[test]
fn a_returned_value_holding_the_buffer_keeps_its_content() {
    let (out, _, allocs, frees) = counts(RETURNED_ALIAS);
    assert_eq!(out, "[1, 2, 3, 4, 5, 6] [0, 10, 1, 11, 2, 12]\n");
    assert_eq!(allocs, frees, "leaked {} block(s)", allocs.abs_diff(frees));
}
