//! #3501 — the growing accumulators that native extends in place since
//! #3454 grow LINEARLY on the wasm leg too. Each shape runs at two sizes with
//! the allocation counter armed (`almide_wasm::alloc_count`); the bytes the
//! run requested must about double when `n` doubles. Before the fix every
//! one of them copied the whole accumulated value per step, so the bytes
//! went up four times per doubling (measured n = 4000 → 8000: the field
//! interpolation 36.1M → 151.7M, the list field 64M → 256M).
//!
//! The aliasing shapes that must NOT be extended in place — a copy of the
//! record or of the field taken before the loop, a right side that reads the
//! place again, the place not first, a nested path with an alias at the
//! middle level — print the same bytes as the native leg (recorded from
//! `almide run`), and every module balances (`allocs == frees`). The
//! certificates of the window frames are accepted by the portable checker.

mod harness;
use harness::run_wasm;

fn counts(src: &str) -> (String, u64, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("accumulator_growth.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, c.bytes, c.allocs, c.frees)
}

const PRELUDE: &str = r#"type Buf = { text: String, xs: List[Int], n: Int }
type Outer = { tag: Int, buf: Buf }
"#;

/// One shape's loop body over `b` / `s`, the value printed, at size `n`.
fn program(body: &str, n: u32) -> String {
    format!(
        "{PRELUDE}
effect fn main() -> Unit = {{
  var s = \"\"
  var b = Buf {{ text: \"\", xs: [], n: 0 }}
  for i in 0..<{n} {{
    {body}
  }}
  println(\"${{string.len(s)}} ${{string.len(b.text)}} ${{list.len(b.xs)}}\")
}}
"
    )
}

const SHAPES: [(&str, &str); 4] = [
    ("interpolation into a var", "s = \"${s}-${i}\""),
    ("concat into a string field", "b.text = b.text + \"ab\""),
    ("interpolation into a string field", "b.text = \"${b.text}-${i}\""),
    ("concat into a list field", "b.xs = b.xs + [i]"),
];

#[test]
fn field_and_interpolation_accumulators_grow_linearly() {
    let mut failures = Vec::new();
    for (name, body) in SHAPES {
        let (_, small, a1, f1) = counts(&program(body, 4000));
        let (_, large, a2, f2) = counts(&program(body, 8000));
        assert_eq!(a1, f1, "{name}: n=4000 leaked {} block(s)", a1 - f1);
        assert_eq!(a2, f2, "{name}: n=8000 leaked {} block(s)", a2 - f2);
        eprintln!("{name}: {small} -> {large} bytes");
        // Linear doubles (plus the amortized growth's slack); quadratic
        // quadruples. 2.5x separates the two with room on both sides.
        if large * 2 > small * 5 {
            failures.push(format!("{name}: {small} -> {large} bytes ({:.2}x per doubling)", large as f64 / small as f64));
        }
    }
    assert!(failures.is_empty(), "quadratic accumulators on wasm:\n{}", failures.join("\n"));
}

/// The shapes that keep (or must reach) the copy: each row's stdout is the
/// native leg's, recorded with `almide run`.
const ALIASING: &str = r#"type Buf = { text: String, xs: List[Int], n: Int }
type Outer = { tag: Int, buf: Buf }
type Names = { all: List[String], keep: String }

fn record_copy(n: Int) -> String = {
  var b = Buf { text: "r", xs: [7], n: 0 }
  let old = b
  for i in 0..<n {
    b.text = b.text + "+"
    b.xs = b.xs + [i]
    b.text = "${b.text}${i}"
  }
  "${old.text} ${old.xs} ${b.text} ${b.xs}"
}

fn field_copy(n: Int) -> String = {
  var b = Buf { text: "f", xs: [1], n: 0 }
  let t = b.text
  let ys = b.xs
  for i in 0..<n {
    b.text = "${b.text}${i}"
    b.xs = b.xs + [i]
  }
  "${t} ${ys} ${b.text} ${b.xs}"
}

fn copy_inside(n: Int) -> String = {
  var b = Buf { text: "c", xs: [], n: 0 }
  var log = ""
  for i in 0..<n {
    let snap = b.text
    b.text = b.text + "${i}"
    log = "${log}${snap}|"
  }
  "${log} ${b.text}"
}

fn rereads(n: Int) -> String = {
  var b = Buf { text: "x", xs: [2], n: 0 }
  for _ in 0..<n {
    b.text = b.text + b.text
    b.xs = b.xs + b.xs
    b.text = "${b.text}${b.n}${b.text}"
    b.text = "<${b.text}>"
    b.n = b.n + 1
  }
  "${b.text} ${b.xs}"
}

fn nested(n: Int) -> String = {
  var o = Outer { tag: 1, buf: Buf { text: "n", xs: [], n: 0 } }
  let mid = o.buf
  for i in 0..<n {
    o.buf.text = "${o.buf.text}${i}"
    o.buf.xs = o.buf.xs + [i]
    o.buf.text = o.buf.text + "."
  }
  "${mid.text} ${mid.xs} ${o.buf.text} ${o.buf.xs} ${o.tag}"
}

fn var_interp(n: Int) -> String = {
  var s = "v"
  let keep = s
  var t = "w"
  for i in 0..<n {
    s = "${s}${i}"
    t = "${t}${s}"
  }
  "${keep} ${s} ${t}"
}

fn tag(i: Int) -> String = "<${i}>"

fn produced_parts(n: Int) -> String = {
  var s = "s"
  var b = Buf { text: "b", xs: [], n: 0 }
  var nm = Names { all: [], keep: "k" }
  let first = nm
  for i in 0..<n {
    s = "${s}${tag(i)}-${string.repeat("z", i)}"
    b.text = "${b.text}${tag(i)}"
    b.text = b.text + tag(i + 10)
    nm.all = nm.all + [tag(i)]
  }
  "${s} ${b.text} ${nm.all} ${first.all}"
}

effect fn main() -> Unit = {
  println(produced_parts(3))
  println(record_copy(3))
  println(field_copy(3))
  println(copy_inside(3))
  println(rereads(2))
  println(nested(3))
  println(var_interp(3))
}
"#;

const ALIASING_NATIVE: &str = "s<0>-<1>-z<2>-zz b<0><10><1><11><2><12> [\"<0>\", \"<1>\", \"<2>\"] []
r [7] r+0+1+2 [7, 0, 1, 2]
f [1] f012 [1, 0, 1, 2]
c|c0|c01| c012
<<xx0xx><xx0xx>1<xx0xx><xx0xx>> [2, 2, 2, 2]
n [] n0.1.2. [0, 1, 2] 1
v v012 wv0v01v012
";

#[test]
fn aliasing_shapes_keep_their_copies_and_match_native() {
    let (out, _, allocs, frees) = counts(ALIASING);
    assert_eq!(out, ALIASING_NATIVE);
    assert_eq!(allocs, frees, "the aliasing probe leaked {} block(s)", allocs - frees);
}

#[test]
fn the_window_frames_certify() {
    // ONE test collects: the witness sink is process-global.
    let ir = almide_spine::s5::lower_to_ir("accumulator_growth.almd", ALIASING).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let w: std::collections::BTreeMap<String, String> = almide_wasm::witness::take().into_iter().collect();
    for name in ["produced_parts", "record_copy", "field_copy", "copy_inside", "rereads", "nested", "var_interp"] {
        let cert = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed; got {:?}", w.keys()));
        assert!(
            almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes()),
            "{name}'s certificate is refused:\n{cert}"
        );
    }
}
