//! Lowering shapes the certificate producer still serves, driven through it.
//!
//! `almide verify` hands each program to `almide_mir::pipeline::program_witnesses`,
//! which lowers the program's own functions to MIR and emits their ownership,
//! name and capability witnesses. The specialized lowerings below live in
//! `crates/almide-mir/src/lower/` and are reached only by this producer (and the
//! corpus classifier) now. The native rung walls on every one of these programs
//! at its signature subset check, and the structural wasm emitter does not use
//! MIR lowering.
//!
//! Until #2935 these shapes were exercised by the incumbent renderer's tests,
//! which lowered and ran the same programs. When that renderer and its tests
//! were deleted, the lowerings lost every test that reached them (#3007). This
//! file drives them through the product entry instead. Each case asserts:
//!
//! - every named function LOWERS. A wall would send it to the bundle as
//!   `uncertified`, and the specialized path would silently stop being reached;
//! - every line of its ownership certificate passes the kernel pre-flight
//!   (`common::reject_reason`, the mirror of the Coq checker `corpus-wall.sh`
//!   runs), so the lowering is balanced and not merely present;
//! - no other function is emitted, so every HOF callback was defunctionalized
//!   inline rather than lifted (the fallback when a specialized lowering
//!   declines), and the tail-recursive functions named in `loops` certify as
//!   loops.
//!
//! Each assertion was checked against its lowering switched off: with the
//! tuple-fold lowerings declining, `best` and `find_at` lift their callbacks;
//! with the tail-call rewrite declining, `pfield` / `prows` lose their loops;
//! with the result-constructor lowering declining, `parse` walls.
//!
//! The binary's name starts with `lower`, so `proofs/coverage.sh` runs it.

mod common;
use common::reject_reason;

struct Case {
    name: &'static str,
    /// The lowering this program is here to reach (for the failure message).
    shape: &'static str,
    src: &'static str,
    /// Functions that must lower and certify. They are also the ONLY functions
    /// the producer may emit: a lifted `__lambda_*` means a HOF callback was
    /// lifted instead of defunctionalized inline, i.e. the specialized fold /
    /// map lowering declined and a generic route took over.
    fns: &'static [&'static str],
    /// Functions whose certificate must contain a loop `(..)`: the tail-call
    /// rewrite turned their self-recursion into a loop.
    loops: &'static [&'static str],
}

const CASES: &[Case] = &[
    Case {
        name: "opt_tuple_fold_scanner",
        shape: "(scalar, Option[scalar]) fold accumulator (defunc_tuple_fold_b.rs)",
        src: r#"fn find_at(sizes: List[Int], target: Int, pos: Int) -> Option[Int] = {
  let positions = list.range(0, 10)
  list.fold(positions, (pos, none), (state, i) => {
    let (p, found) = state
    match found {
      some(_) => state
      none =>
        if p > 100 then (p, none)
        else {
          let size = list.get(sizes, i) |> option.unwrap_or(999)
          if p == target then (p, some(p))
          else (p + size, none)
        }
    }
  }).1
}
effect fn main() -> Unit = {
  match find_at([4, 4, 4, 4], 8, 0) {
    some(p) => println("found:" + int.to_string(p))
    none => println("none")
  }
  match find_at([4, 4], 99, 0) {
    some(p) => println("?")
    none => println("none")
  }
}
"#,
        fns: &["find_at", "main"],
        loops: &[],
    },
    Case {
        name: "scalar_tuple_fold_with_preamble",
        shape: "(scalar, scalar) argmax fold with per-iteration lets (defunc_tuple_fold.rs)",
        src: r#"fn best(tokens: List[String], ranks: List[Int]) -> (Int, Int) = {
  let last = list.len(tokens) - 1
  list.fold(list.range(0, last), (0 - 1, 99999999), (acc, i) => {
    let (best_i, best_rank) = acc
    let a = list.get(tokens, i) |> option.unwrap_or("")
    let rank = if string.len(a) > 1 then list.get(ranks, i) |> option.unwrap_or(99999999) else 99999999
    if rank < best_rank then (i, rank) else (best_i, best_rank)
  })
}
effect fn main() -> Unit = {
  let r = best(["ab", "c", "def"], [5, 9, 2])
  println(int.to_string(r.0) + ":" + int.to_string(r.1))
}
"#,
        fns: &["best", "main"],
        loops: &[],
    },
    Case {
        name: "result_record_tuple_ctor",
        shape: "ok((Record, Int)) / err constructors of an effect fn (result_ctors.rs)",
        src: r#"type Hdr = { version: Int, count: Int }
effect fn parse(x: Int) -> Result[(Hdr, Int), String] =
  if x != 7 then err("bad magic") else ok((Hdr { version: x, count: x * 2 }, 24))
effect fn main() -> Unit = {
  match parse(7) {
    ok(pair) => {
      let (h, off) = pair
      println(int.to_string(h.version) + ":" + int.to_string(h.count) + ":" + int.to_string(off))
    }
    err(e) => println("E:" + e)
  }
  match parse(1) {
    ok(p) => println("?")
    err(e) => println("E:" + e)
  }
}
"#,
        fns: &["parse", "main"],
        loops: &[],
    },
    Case {
        name: "mutual_tail_csv_rows",
        shape: "tail-recursive append accumulators across mutual recursion (mod_p5_b.rs)",
        src: r#"fn pfield(text: String, pos: Int, acc: String) -> (String, Int) =
  if pos >= string.len(text) then (acc, pos)
  else {
    let c = string.get(text, pos) ?? ""
    if c == "," or c == "\n" then (acc, pos) else pfield(text, pos + 1, acc + c)
  }
fn pafter(text: String, pos: Int, rows: List[List[String]], cur: List[String]) -> List[List[String]] =
  if pos >= string.len(text) then rows + [cur]
  else {
    let c = string.get(text, pos) ?? ""
    if c == "," then prows(text, pos + 1, rows, cur)
    else if c == "\n" then prows(text, pos + 1, rows + [cur], [])
    else prows(text, pos, rows, cur)
  }
fn prows(text: String, pos: Int, rows: List[List[String]], cur: List[String]) -> List[List[String]] =
  if pos >= string.len(text) then rows + [cur]
  else {
    let c = string.get(text, pos) ?? ""
    if c == "," then prows(text, pos + 1, rows, cur + [""])
    else if c == "\n" then prows(text, pos + 1, rows + [cur], [])
    else {
      let (f, np) = pfield(text, pos, "")
      pafter(text, np, rows, cur + [f])
    }
  }
fn pick(rows: List[List[String]]) -> Int = {
  let header = list.get(rows, 0) ?? []
  let data = list.drop(rows, 1)
  list.len(header) + list.len(data)
}
effect fn main() -> Unit = {
  let rows = prows("a,b\nc\nd,e,f", 0, [], [])
  println(int.to_string(pick(rows)))
}
"#,
        fns: &["pfield", "pafter", "prows", "pick", "main"],
        loops: &["pfield", "prows"],
    },
    Case {
        name: "enumerate_map_to_value_pairs",
        shape: "enumerate |> map building (String, Value) pairs (defunc_hof_inner.rs)",
        src: r#"import json
effect fn main() -> Unit = {
  let row = ["x", "y"]
  let header = ["a", "b"]
  let pairs = header |> list.enumerate |> list.map((entry) => {
    let (i, key) = entry
    (key, value.str(list.get_or(row, i, "")))
  })
  println(value.stringify(value.object(pairs)))
  let strs = ["p", "q"]
  let objs = strs |> list.map((s) => value.object([(s, value.int(1))]))
  println(value.stringify(value.array(objs)))
}
"#,
        fns: &["main"],
        loops: &[],
    },
    Case {
        name: "zip_map_flatten_rows",
        shape: "zip |> map whose body flattens a nested heap list (defunc_hof_inner.rs)",
        src: r#"fn concat_cols(a: List[List[Float]], b: List[List[Float]]) -> List[List[Float]] =
  list.zip(a, b) |> list.map((pair) => list.flatten([pair.0, pair.1]))
fn main() -> Unit = {
  let c = concat_cols([[1.0, 2.0], [3.0, 4.0]], [[5.0], [6.0]])
  for row in c {
    println(row |> list.map((v) => float.to_string(v)) |> list.join(","))
  }
}
"#,
        fns: &["concat_cols", "main"],
        loops: &[],
    },
    Case {
        name: "fold_string_acc_over_tuples",
        shape: "fold with a String accumulator over a List[(String, Int)] (defunc_hof_inner.rs)",
        src: r#"fn lookup(pairs: List[(String, Int)], target: Int) -> String =
  list.fold(pairs, "", (acc, pair) => if pair.1 == target then pair.0 else acc)
fn main() -> Unit = {
  let ps = [("alpha", 1), ("beta", 2)]
  println(lookup(ps, 2))
  println(lookup(ps, 9))
}
"#,
        fns: &["lookup", "main"],
        loops: &[],
    },
    Case {
        name: "matrix_heads_map_and_flat_map",
        shape: "map / flat_map over List[Matrix] heads (defunc_hof_inner.rs)",
        src: r#"fn per_head_rms_norm(x: Matrix, gamma: List[Float], n_heads: Int, eps: Float) -> Matrix = {
  let heads = matrix.split_cols_even(x, n_heads)
  let normed = heads |> list.map((h) => matrix.rms_norm_rows(h, gamma, eps))
  matrix.concat_cols(normed)
}
fn repeat_kv(kv: Matrix, n_kv_heads: Int, n_rep: Int) -> Matrix = {
  if n_rep == 1 then kv
  else {
    let heads = matrix.split_cols_even(kv, n_kv_heads)
    let repeated = heads |> list.flat_map((h) => list.repeat(h, n_rep))
    matrix.concat_cols(repeated)
  }
}
effect fn main() -> Unit = {
  let x = matrix.from_lists([[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]])
  let p = per_head_rms_norm(x, [0.5, 1.5], 2, 0.00001)
  println(float.to_string(matrix.get(p, 0, 0)))
  let r2 = repeat_kv(x, 2, 2)
  println(int.to_string(matrix.cols(r2)))
}
"#,
        fns: &["per_head_rms_norm", "repeat_kv", "main"],
        loops: &[],
    },
    Case {
        name: "host_env_ops",
        shape: "env.set / env.sleep_ms as ordinary host calls (host_ops.rs, #2739)",
        src: r#"import env
effect fn main() -> Unit = {
  env.set("ALMIDE_LIVE_SHAPE_KEY", "v")
  env.sleep_ms(0)!
  println(env.get("ALMIDE_LIVE_SHAPE_KEY") ?? "unset")
}
"#,
        fns: &["main"],
        loops: &[],
    },
    Case {
        name: "host_http_result_match",
        shape: "a match over a host op's Result, err payload bound (host_ops.rs, #2739)",
        src: r#"import http
effect fn main() -> Unit = {
  match http.get("http://127.0.0.1:9/") {
    ok(_) => println("unexpected"),
    err(e) => println("get: ${e}"),
  }
}
"#,
        fns: &["main"],
        loops: &[],
    },
    Case {
        name: "literal_heap_qq_elem",
        shape: "a heap `??` element binds before its list/map literal (literal_elems.rs)",
        src: r#"fn header(o: Option[String]) -> Map[String, String] = ["X-Echo": o ?? "none", "K": "v"]
fn count(o: Option[String]) -> Int = map.len(["X-Echo": o ?? "none"])
fn row(o: Option[String]) -> List[String] = ["a", o ?? "none"]
"#,
        fns: &["header", "count", "row"],
        loops: &[],
    },
    Case {
        name: "closure_from_ok_payload",
        shape: "an `ok(h)` closure payload is dispatched through (control_p2_e.rs, the router value)",
        src: r#"fn add1(x: Int) -> Result[List[String], String] = ok(["a", int.to_string(x)])
fn mk(n: Int) -> Result[(Int) -> Result[List[String], String], String] = if n > 0 then ok(add1) else err("no")
effect fn go() -> Unit = {
  let h = mk(1)!
  let r = h(1)!
  if list.len(r) == 2 then () else println("bad")
}
effect fn main() -> Unit = go()!
"#,
        // `__lambda_mk_0` is the closure block `ok(add1)` builds, not a declined callback.
        fns: &["add1", "mk", "__lambda_mk_0", "go", "main"],
        loops: &[],
    },
];

#[test]
fn every_live_shape_lowers_and_certifies_through_the_witness_producer() {
    let mut failures = Vec::new();
    for case in CASES {
        let w = match almide_mir::pipeline::program_witnesses(case.src, &[], None) {
            Ok(w) => w,
            Err(e) => {
                failures.push(format!(
                    "{} [{}]: the program does not lower as a whole: {e}",
                    case.name, case.shape
                ));
                continue;
            }
        };
        let extra: Vec<&str> = w
            .functions
            .iter()
            .map(|fw| fw.name.as_str())
            .filter(|n| !case.fns.contains(n))
            .collect();
        if !extra.is_empty() {
            failures.push(format!(
                "{} [{}]: unexpected lifted function(s) {extra:?}",
                case.name, case.shape
            ));
        }
        for f in case.loops {
            if !w
                .functions
                .iter()
                .any(|fw| fw.name == *f && fw.ownership.contains('('))
            {
                failures.push(format!(
                    "{} [{}]: `{f}` certificate has no loop (tail call not rewritten)",
                    case.name, case.shape
                ));
            }
        }
        for f in case.fns {
            if let Some((_, why)) = w.walled.iter().find(|(n, _)| n == f) {
                failures.push(format!(
                    "{} [{}]: `{f}` walled: {why}",
                    case.name, case.shape
                ));
                continue;
            }
            let Some(fw) = w.functions.iter().find(|fw| fw.name == *f) else {
                failures.push(format!(
                    "{} [{}]: `{f}` produced no witness",
                    case.name, case.shape
                ));
                continue;
            };
            for line in fw.ownership.lines() {
                if let Some(r) = reject_reason(line) {
                    failures.push(format!(
                        "{} [{}]: `{f}` certificate {line:?} rejected: {r}",
                        case.name, case.shape
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n  {}", failures.join("\n  "));
}
