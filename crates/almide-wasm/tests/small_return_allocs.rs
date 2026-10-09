//! Per-shape allocation pins for the small values a call hands straight to
//! its consumer (the onebrc aggregate's per-line shapes). Each program runs
//! its shape 100 times; the pinned count is what the wasm leg allocates
//! for all of it, measured with the allocation counter (`alloc_count`).
//!
//!  - `string.split_once` destructured by its `match`: the two pieces only —
//!    no option cell, no pair (split_match.rs). Was 4 per call.
//!  - `map.upsert` on a receiver the call owns (a dying var, an owned param
//!    at its frame's tail, a fold accumulator): written in place — no copy
//!    of the map per call, and an inert `init` built only when stored
//!    (dying_move.rs, writeback_move.rs `lower_fold_body`). Was 3 per call
//!    (the map copy, the eager init, the callback's record).
//!  - `int.parse` of an already-trimmed text: the Result only — a text that
//!    starts and ends with a visible ASCII byte is parsed in place, not
//!    through a trimmed copy (stdlib/string_to_int.almd). Was 2.

mod harness;
use harness::run_wasm;

fn counted(src: &str) -> (String, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    let _guard = almide_wasm::alloc_count::CountGuard::set();
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.exit, 0, "the probe exits 0: {}", r.stdout);
    let c = r.alloc_count.expect("the armed module exports the counters");
    assert_eq!(c.live(), 0, "every block is released by exit");
    (r.stdout, c.allocs, c.frees)
}

const SPLIT: &str = r#"fn f(line: String) -> Int =
  match string.split_once(line, ";") {
    some((a, b)) => string.len(a) * 10 + string.len(b),
    none => 0,
  }

effect fn main() -> Unit = {
  var acc = 0
  for i in 0..<100 {
    acc = acc + f("Abha;12.3") + f("none")
  }
  println("${acc}")
}
"#;

const SPLIT_ONE_SIDE: &str = r#"fn f(line: String) -> Int =
  match string.split_once(line, "::") {
    none => 0,
    some((_, b)) => string.len(b),
  }

effect fn main() -> Unit = {
  var acc = 0
  for i in 0..<100 {
    acc = acc + f("k::value")
  }
  println("${acc}")
}
"#;

const UPSERT_PARAM: &str = r#"type Stats = { min: Int, max: Int, count: Int }

fn step(acc: Map[String, Stats], k: String, t: Int) -> Map[String, Stats] =
  map.upsert(acc, k, Stats { min: t, max: t, count: 1 }, (s) => Stats {
    min: if t < s.min then t else s.min,
    max: if t > s.max then t else s.max,
    count: s.count + 1,
  })

effect fn main() -> Unit = {
  var m: Map[String, Stats] = map.new()
  for i in 0..<100 {
    m = step(m, "k", i)
  }
  let s = map.get(m, "k") ?? Stats { min: 0, max: 0, count: 0 }
  println("${s.min} ${s.max} ${s.count}")
}
"#;

const UPSERT_FOLD: &str = r#"fn step(acc: Map[String, Int], line: String) -> Map[String, Int] =
  match string.split_once(line, ";") {
    some((k, _)) => map.upsert(acc, k, 1, (n) => n + 1),
    none => acc,
  }

effect fn main() -> Unit = {
  let lines = list.repeat("a;1", 100)
  let m = list.fold(lines, map.new(), (acc, line) => step(acc, line))
  println("${map.get(m, "a") ?? 0}")
}
"#;

const PARSE: &str = r#"fn f(s: String) -> Int = match int.parse(s) { ok(v) => v, err(_) => 0 }

effect fn main() -> Unit = {
  var acc = 0
  for i in 0..<100 {
    acc = acc + f("12")
  }
  println("${acc}")
}
"#;

#[test]
fn a_destructured_split_once_allocates_only_its_pieces() {
    assert_eq!(counted(SPLIT), ("4400\n".to_string(), 200, 200), "two pieces per hit, nothing on a miss");
    assert_eq!(counted(SPLIT_ONE_SIDE), ("500\n".to_string(), 100, 100), "a wildcard piece is never built");
}

#[test]
fn an_owned_upsert_receiver_is_written_in_place() {
    // The map's two blocks (empty, then grown by the first insert), the
    // first insert's init, the 99 callback records of the present-key
    // calls, and the final lookup's option: no copy of the map, no unused
    // init (develop: 3 per call).
    assert_eq!(counted(UPSERT_PARAM), ("0 99 100\n".to_string(), 103, 103));
}

#[test]
fn a_fold_accumulator_moves_into_its_step() {
    // The list, the map's two blocks, one `k` piece per line, the final
    // lookup's option — the accumulator never copied.
    assert_eq!(counted(UPSERT_FOLD), ("100\n".to_string(), 104, 104));
}

#[test]
fn parsing_a_trimmed_text_allocates_only_its_result() {
    assert_eq!(counted(PARSE), ("1200\n".to_string(), 100, 100));
}
