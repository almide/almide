//! #2944 / #2933 — a write through a record field, at any depth, balances:
//! every block the module allocates is freed (`allocs == frees`).
//!
//! Before #2944 the copy-on-write field write rebound the var to the fresh
//! copy without releasing the record it replaced: `h.n = i` in a loop leaked
//! one record per write (the spread rebind `h = { ...h, n: i }` balanced).
//! The field forms of `list.push` and `string.push` also leaked their
//! one-element list / fresh argument, which a synthesized concat left
//! unowned. #2933 takes the same write through deeper paths (`h.a.b`), so
//! each level's copy must settle the same way.
//!
//! Each probe's stdout is the native leg's, recorded from `almide run`.

mod harness;
use harness::run_wasm;

fn counts(src: &str) -> (String, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("field_write.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, c.allocs, c.frees)
}

/// The #2944 repro: a scalar field write in a loop.
const FIELD_WRITE: &str = r#"type H = { xs: List[Int], n: Int }

effect fn main() -> Unit = {
  var h = { xs: [1], n: 0 }
  for i in 0..<1000 {
    h.n = i
  }
  println(int.to_string(h.n))
}
"#;

/// One-level mut forms with fresh arguments: the push's one-element list
/// and string.push's argument are temporaries of the write.
const ONE_LEVEL_MUT: &str = r#"type H = { xs: List[Int], s: String, n: Int }

effect fn main() -> Unit = {
  var h = { xs: [1, 2, 3], s: "", n: 0 }
  for i in 0..<1000 {
    list.push(h.xs, i)
    let _ = list.pop(h.xs)
    string.push(h.s, int.to_string(i % 10))
    if string.len(h.s) > 8 then { string.clear(h.s) } else ()
  }
  println(int.to_string(list.len(h.xs)) + " " + h.s)
}
"#;

#[test]
fn a_field_write_releases_the_record_it_replaces() {
    let (out, allocs, frees) = counts(FIELD_WRITE);
    assert_eq!(out, "999\n");
    assert_eq!(allocs, frees, "h.n = i leaked {} block(s)", allocs - frees);
}

#[test]
fn one_level_mut_forms_release_their_temporaries() {
    let (out, allocs, frees) = counts(ONE_LEVEL_MUT);
    assert_eq!(out, "3 9\n");
    assert_eq!(allocs, frees, "the one-level mut forms leaked {} block(s)", allocs - frees);
}

/// The cross-target fixture: every mut form through a two- and three-level
/// path, on a var and a `mut` parameter, with copies bound before the ops.
#[test]
fn nested_field_path_mut_forms_balance() {
    let src = include_str!("../../../spec/wasm_cross/nested_field_path_mut.almd");
    let (out, allocs, frees) = counts(src);
    assert_eq!(
        out,
        "drained 3: 3,2,1\n\
         root.trace now 0, the copy bound before 3\n\
         n 1007752 xs 0,1,2,3\n\
         ss s0,s1,s2,s3 fs 499.5 s 7890123456789\n\
         ps p0,p1,p2,p3 m 7 trace 4\n\
         snaps 0/0/ 64252/4/1234567890123456789 253752/4/34567890123456789 568252/4/567890123456789\n"
    );
    assert_eq!(allocs, frees, "the nested field path leaked {} block(s)", allocs - frees);
}
