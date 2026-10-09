//! #3397: a list op whose work is bounded by a range of its source must BORROW
//! the source on the native target, never clone it.
//!
//! `list.slice(cs, i, j)` was emitted as `almide_rt_list_slice(cs.clone(), i, j)`
//! (and `list.take` / `list.drop` the same), so a loop of three-element slices
//! of a 50 000-element list cloned the whole list per call: 4 s native against
//! 0 ms on wasm, and 24 s for `take` + `drop`. The op only reads its source, so
//! the runtime takes `&[T]` and copies just the elements it returns.
//!
//! Family rule (the matrix this gate holds):
//! - RANGE ops — the result is a sub-range, a single element or a scalar, or the
//!   op can stop early — borrow: no `@consume(xs)` in `stdlib/list.almd`, a `&`
//!   first parameter in `runtime/rs/src/list.rs`, and an emitted call with no
//!   `.clone()` / `.to_vec()` of a source that is used afterwards.
//! - An op may consume (`@consume(xs)`) only when its work already visits every
//!   source element, so one clone costs no more than the op itself (map, zip,
//!   enumerate, …), or when it reuses the buffer in place (insert, update, …).
//!   That set is `MAY_CONSUME`; a new `@consume` outside it fails here, so the
//!   choice is made on purpose rather than by copying a neighbour.

use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// Range ops, each with a call on the source `cs` (a `List[String]`).
const RANGE_OPS: &[(&str, &str)] = &[
    ("slice", "list.len(list.slice(cs, 1, 3))"),
    ("take", "list.len(list.take(cs, 2))"),
    ("drop", "list.len(list.drop(cs, 2))"),
    ("take_end", "list.len(list.take_end(cs, 2))"),
    ("drop_end", "list.len(list.drop_end(cs, 2))"),
    ("take_while", "list.len(list.take_while(cs, is_a))"),
    ("drop_while", "list.len(list.drop_while(cs, is_a))"),
    ("window", "list.len(list.window(cs, 2))"),
    ("windows", "list.len(list.windows(cs, 2))"),
    ("chunk", "list.len(list.chunk(cs, 2))"),
    ("get", "string.len(list.get(cs, 1) ?? \"\")"),
    ("get_or", "string.len(list.get_or(cs, 1, \"\"))"),
    ("first", "string.len(list.first(cs) ?? \"\")"),
    ("last", "string.len(list.last(cs) ?? \"\")"),
    ("find", "string.len(list.find(cs, is_a) ?? \"\")"),
    ("find_index", "list.find_index(cs, is_a) ?? 9"),
    ("index_of", "list.index_of(cs, \"b\") ?? 9"),
    ("contains", "if list.contains(cs, \"b\") then 1 else 0"),
    ("any", "if list.any(cs, is_a) then 1 else 0"),
    ("all", "if list.all(cs, is_a) then 1 else 0"),
    ("len", "list.len(cs)"),
    ("is_empty", "if list.is_empty(cs) then 1 else 0"),
    ("join", "string.len(list.join(cs, \",\"))"),
];

/// Expected value of each `RANGE_OPS` call on `["a", "a", "b", "c"]`.
const EXPECTED: &[i64] = &[2, 2, 2, 2, 2, 2, 2, 3, 3, 2, 1, 1, 1, 1, 1, 0, 2, 1, 1, 0, 4, 0, 7];

/// Callback ops stream fusion may turn into an iterator chain.
const FUSED: &[&str] = &["find", "find_index", "any", "all"];

/// Ops allowed to take their source by value: they visit every element (the
/// result holds them all, or folds over them all) or reuse the buffer.
const MAY_CONSUME: &[&str] = &[
    "map", "filter", "filter_map", "flat_map", "fold", "reduce", "scan", "partition", "group_by",
    "sort_by", "unique_by", "enumerate", "zip", "zip_with", "intersperse", "shuffle",
    "insert", "remove_at", "update",
];

fn root() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

/// `(runtime symbol suffix, has @consume(xs))` for every list intrinsic.
fn list_intrinsics() -> Vec<(String, bool)> {
    let src = std::fs::read_to_string(format!("{}/stdlib/list.almd", root())).unwrap();
    let mut out = Vec::new();
    let mut sym: Option<String> = None;
    let mut consume = false;
    for line in src.lines() {
        if let Some(rest) = line.strip_prefix("@intrinsic(\"almide_rt_list_") {
            sym = rest.split('"').next().map(str::to_string);
            consume = false;
        } else if line.starts_with("@consume(") {
            consume = consume || line.contains("xs");
        } else if line.starts_with("fn ") {
            if let Some(s) = sym.take() {
                out.push((s, consume));
            }
            consume = false;
        }
    }
    assert!(out.len() > 40, "stdlib/list.almd lost its intrinsics? found {}", out.len());
    out
}

fn runtime_first_param(op: &str) -> String {
    let src = std::fs::read_to_string(format!("{}/runtime/rs/src/list.rs", root())).unwrap();
    let head = format!("pub fn almide_rt_list_{op}");
    let line = src.lines().find(|l| l.starts_with(&head) && l[head.len()..].starts_with(['<', '(']))
        .unwrap_or_else(|| panic!("runtime/rs/src/list.rs has no `{head}`"));
    let params = &line[line.find('(').unwrap() + 1..];
    params.split(':').nth(1).unwrap_or("").trim().to_string()
}

#[test]
fn range_ops_borrow_and_only_whole_traversal_ops_consume() {
    let intrinsics = list_intrinsics();
    for &(op, _) in RANGE_OPS {
        let (_, consume) = intrinsics.iter().find(|(s, _)| s == op)
            .unwrap_or_else(|| panic!("list.{op} is not an intrinsic any more — update RANGE_OPS"));
        assert!(!consume, "list.{op} only reads a range of its source but is `@consume(xs)` (#3397)");
        let ty = runtime_first_param(op);
        assert!(ty.starts_with('&'), "almide_rt_list_{op} takes its source as `{ty}`, not a borrow (#3397)");
    }
    for (op, consume) in &intrinsics {
        if *consume {
            assert!(
                MAY_CONSUME.contains(&op.as_str()),
                "list.{op} is `@consume(xs)` but is not in MAY_CONSUME: an op that does not visit every \
                 source element must borrow it (a used-afterwards source is cloned whole per call, #3397)"
            );
        }
    }
}

fn program() -> String {
    let calls: Vec<String> = RANGE_OPS.iter().map(|(_, c)| format!("    {c},")).collect();
    format!(
        "fn is_a(s: String) -> Bool = s == \"a\"\n\n\
         fn probe(cs: List[String]) -> List[Int] =\n  [\n{calls}\n  ]\n\n\
         effect fn main() -> Unit = {{\n\
         \x20 let cs = [\"a\", \"a\", \"b\", \"c\"]\n\
         \x20 let here = [\n{here}\n  ]\n\
         \x20 println(list.join(list.map(probe(cs), (n) => int.to_string(n)), \" \"))\n\
         \x20 println(list.join(list.map(here, (n) => int.to_string(n)), \" \"))\n\
         \x20 for c in cs {{ println(c) }}\n\
         \x20 println(list.join(cs, \"\"))\n\
         }}\n",
        calls = calls.join("\n"),
        here = calls.join("\n"),
    )
}

#[test]
fn range_ops_emit_a_borrow_of_a_used_afterwards_source() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("range_ops.almd");
    std::fs::write(&src, program()).unwrap();

    let out = Command::new(almide()).args([src.to_str().unwrap(), "--target", "rust"]).output().unwrap();
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout);
    // The user fns: `probe` (cs is a param) and `main` (cs is a local read again after).
    let user: String = ["pub fn probe(", "pub fn __almide_main("]
        .iter()
        .filter_map(|h| rust.find(h).map(|i| rust[i..].split("\n}\n").next().unwrap().to_string()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        user.contains("pub fn probe(") && user.contains("pub fn __almide_main("),
        "the emitted Rust lost `probe` or `__almide_main`"
    );
    // Fused or not, nothing in the two bodies copies the whole source.
    for whole in ["cs.clone(", "cs.to_vec(", "(cs).clone(", "(cs).to_vec("] {
        assert!(!user.contains(whole), "a range op copies its whole source on native: `{whole}` (#3397)\n{user}");
    }
    for &(op, _) in RANGE_OPS {
        let call = format!("almide_rt_list_{op}(");
        let calls = user.match_indices(&call).count();
        // A fusable op may become an iterator chain over the source instead
        // (no runtime call — the chain borrows); every other op is a call.
        assert!(
            calls >= 2 || (calls == 0 && FUSED.contains(&op)),
            "list.{op}: expected one runtime call in `probe` and one in `__almide_main`, found {calls}"
        );
        for (i, _) in user.match_indices(&call) {
            let arg = user[i + call.len()..].split([',', ')']).next().unwrap();
            assert!(
                !arg.contains(".clone(") && !arg.contains(".to_vec(") && !arg.contains(".to_owned("),
                "list.{op} clones its source on native: `{call}{arg}` (#3397)"
            );
        }
    }

    let run = Command::new(almide()).args(["run", src.to_str().unwrap()]).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    let want: Vec<String> = EXPECTED.iter().map(i64::to_string).collect();
    let want = want.join(" ");
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        vec![want.as_str(), want.as_str(), "a", "a", "b", "c", "aabc"],
        "native run disagrees:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

// ── #3398: move the source at its last use ──────────────────────────────────
//
// Borrowing the source costs a clone of every element the op KEEPS. Where the
// source is not read again — its last use, or a temporary — that is pure loss:
// 2× on a `list.drop(xs, 1)` tail recursion over strings. Each op below has an
// `_owned` twin in the runtime that takes the source by value and moves the
// kept elements; the clone pass (and TailCallOpt, for its loop params) leaves
// the source a bare `Var` where its last-use analysis allows a move, and
// BorrowLowering calls the twin there (`crates/almide-codegen/src/owned_source.rs`).
//
// Family rule: every range op whose result is a sub-list of its source
// (`-> List[A]`) has a twin, and so does `find` (it may move its hit out).
// The single-element reads (`get`, `get_or`, `first`, `last`) copy one element
// at most and the nested-list ops (`window`, `windows`, `chunk`) copy elements
// into several results anyway: neither gains from a move, so neither has one.

use almide_codegen::owned_source::OWNED_SOURCE_TWINS;

/// Ops with a twin beyond the sub-list rule.
const TWIN_EXTRA: &[&str] = &["find"];

/// The declared return type of `list.<op>` in `stdlib/list.almd`.
fn stdlib_return_type(op: &str) -> String {
    let src = std::fs::read_to_string(format!("{}/stdlib/list.almd", root())).unwrap();
    let head = format!("fn {op}");
    let line = src.lines().find(|l| l.starts_with(&head) && l[head.len()..].starts_with(['[', '(']))
        .unwrap_or_else(|| panic!("stdlib/list.almd has no `{head}`"));
    line.rsplit("->").next().unwrap().trim_end_matches("= _").trim().to_string()
}

#[test]
fn every_sub_list_range_op_has_an_owned_twin_that_takes_the_source_by_value() {
    // The map twins (`almide_rt_map_*`) are gated by map_owned_twin_test.rs.
    let list_twins: Vec<&(&str, &str)> = OWNED_SOURCE_TWINS.iter()
        .filter(|(b, _)| b.starts_with("almide_rt_list_"))
        .collect();
    let twin_ops: Vec<&str> = list_twins.iter()
        .map(|(b, _)| b.strip_prefix("almide_rt_list_").unwrap())
        .collect();
    for &(op, _) in RANGE_OPS {
        let sub_list = stdlib_return_type(op) == "List[A]";
        let want = sub_list || TWIN_EXTRA.contains(&op);
        assert_eq!(
            twin_ops.contains(&op), want,
            "list.{op} (returns `{}`): an owned twin is {} by the family rule (#3398)",
            stdlib_return_type(op), if want { "required" } else { "not expected" }
        );
    }
    let src = std::fs::read_to_string(format!("{}/runtime/rs/src/list.rs", root())).unwrap();
    for (borrowed, owned) in list_twins {
        assert_eq!(*owned, format!("{borrowed}_owned"), "the twin of {borrowed} is named `{borrowed}_owned`");
        let op = borrowed.strip_prefix("almide_rt_list_").unwrap();
        assert!(RANGE_OPS.iter().any(|(o, _)| *o == op), "{borrowed} has a twin but is not a RANGE op");
        let head = format!("pub fn {owned}");
        let line = src.lines().find(|l| l.starts_with(&head) && l[head.len()..].starts_with(['<', '(']))
            .unwrap_or_else(|| panic!("runtime/rs/src/list.rs has no `{head}`"));
        let first = line[line.find('(').unwrap() + 1..].split(',').next().unwrap();
        assert!(
            first.contains(": Vec<") && !first.contains('&'),
            "{owned} must take its source by value (`mut xs: Vec<T>`), not `{first}`"
        );
    }
}

/// One local per twin op, read once before the op and never after: the op is
/// its last use. Plus a borrow-then-move pair, and the two tail-recursion
/// shapes (TCO loop params: `list.drop(xs, 1)` and a `[x, ..rest]` pattern).
fn last_use_program() -> String {
    "fn is_a(s: String) -> Bool = s == \"a\"\n\
     \n\
     fn mk() -> List[String] = [\"a\", \"a\", \"b\", \"c\"]\n\
     \n\
     fn by_drop(xs: List[String], acc: Int) -> Int =\n\
     \x20 match xs {\n\
     \x20   [] => acc,\n\
     \x20   [x, ..] => by_drop(list.drop(xs, 1), acc + string.len(x)),\n\
     \x20 }\n\
     \n\
     fn by_rest(xs: List[String], acc: Int) -> Int =\n\
     \x20 match xs {\n\
     \x20   [] => acc,\n\
     \x20   [x, ..rest] => by_rest(rest, acc + string.len(x)),\n\
     \x20 }\n\
     \n\
     effect fn main() -> Unit = {\n\
     \x20 let s_take = mk()\n  let s_drop = mk()\n  let s_take_while = mk()\n  let s_drop_while = mk()\n\
     \x20 let s_find = mk()\n  let s_slice = mk()\n  let s_take_end = mk()\n  let s_drop_end = mk()\n\
     \x20 let n = list.len(s_take) + list.len(s_drop) + list.len(s_take_while) + list.len(s_drop_while)\n\
     \x20   + list.len(s_find) + list.len(s_slice) + list.len(s_take_end) + list.len(s_drop_end)\n\
     \x20 let r = [\n\
     \x20   list.len(list.take(s_take, 2)),\n\
     \x20   list.len(list.drop(s_drop, 1)),\n\
     \x20   list.len(list.take_while(s_take_while, is_a)),\n\
     \x20   list.len(list.drop_while(s_drop_while, is_a)),\n\
     \x20   string.len(list.find(s_find, (s) => s == \"b\" or s == \"zz\") ?? \"\"),\n\
     \x20   list.len(list.slice(s_slice, 1, 3)),\n\
     \x20   list.len(list.take_end(s_take_end, 3)),\n\
     \x20   list.len(list.drop_end(s_drop_end, 3)),\n\
     \x20 ]\n\
     \x20 let both = mk()\n\
     \x20 let kept = list.take(both, 1)\n\
     \x20 let moved = list.drop(both, 1)\n\
     \x20 println(list.join(list.map(r, (k) => int.to_string(k)), \" \"))\n\
     \x20 println(\"${n} ${list.join(kept, \"\")} ${list.join(moved, \"\")} ${by_drop(mk(), 0)} ${by_rest(mk(), 0)}\")\n\
     }\n"
        .to_string()
}

fn emit_rust(name: &str, src: &str) -> (tempfile::TempDir, std::path::PathBuf, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, src).unwrap();
    let out = Command::new(almide()).args([path.to_str().unwrap(), "--target", "rust"]).output().unwrap();
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout).to_string();
    (dir, path, rust)
}

/// The body of `pub fn <name>(` in emitted Rust.
fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let head = format!("pub fn {name}(");
    let i = rust.find(&head).unwrap_or_else(|| panic!("the emitted Rust lost `{head}`"));
    rust[i..].split("\n}\n").next().unwrap()
}

#[test]
fn a_range_op_at_its_sources_last_use_moves_it_into_the_owned_twin() {
    let (_dir, src, rust) = emit_rust("last_use.almd", &last_use_program());
    let main = fn_body(&rust, "__almide_main");
    for (_, owned) in OWNED_SOURCE_TWINS.iter().filter(|(b, _)| b.starts_with("almide_rt_list_")) {
        let op = owned.strip_prefix("almide_rt_list_").unwrap().strip_suffix("_owned").unwrap();
        let moved = format!("{owned}(s_{op}");
        // `find` with a lambda may fuse into an iterator chain that consumes
        // the source instead (`(s_find).into_iter()`): a move all the same.
        let fused_move = op == "find" && main.contains(&format!("(s_{op}).into_iter()"));
        assert!(
            main.contains(&moved) || fused_move,
            "list.{op} at the last use of its source does not move it: expected `{moved}` (#3398)\n{main}"
        );
        for copy in [format!("s_{op}.clone("), format!("s_{op}.to_vec(")] {
            assert!(!main.contains(&copy), "list.{op} copies a source it could move: `{copy}`\n{main}");
        }
    }
    // Read again after the first op: borrowed there, moved at the second.
    assert!(main.contains("almide_rt_list_take(&both"), "a source read again must stay borrowed\n{main}");
    assert!(main.contains("almide_rt_list_drop_owned(both"), "the source's last use must move it\n{main}");
    // The tail-recursion shapes: the loop param moves into the drop that rebinds it.
    for f in ["by_drop", "by_rest"] {
        let body = fn_body(&rust, f);
        assert!(
            body.contains("almide_rt_list_drop_owned(xs, 1i64)"),
            "`{f}` clones the kept elements of its loop param per iteration instead of moving them (#3398)\n{body}"
        );
    }

    let run = Command::new(almide()).args(["run", src.to_str().unwrap()]).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).lines().collect::<Vec<_>>(),
        vec!["2 3 2 2 1 2 3 1", "32 a abc 4 4"],
        "native run disagrees:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

/// The tail recursion of #3398 over 4000 strings, timed against its own
/// control in the same binary: `moved` drops at the source's last use (the
/// owned twin, no element cloned), `kept` reads the source once more after the
/// drop, which keeps it borrowed and clones every kept element. Both are
/// O(n²) element moves; only `kept` allocates per element. Measured (release,
/// 5 × 4000): moved 10 ms, kept 700 ms; before #3398 both were the clone path.
/// The gate is a RATIO, so a slow machine moves both sides.
#[test]
fn perf_tail_recursion_moves_instead_of_cloning() {
    let src = "import env\n\
        \n\
        fn moved(xs: List[String], acc: Int) -> Int =\n\
        \x20 match xs {\n\
        \x20   [] => acc,\n\
        \x20   [x, ..] => moved(list.drop(xs, 1), acc + string.len(x)),\n\
        \x20 }\n\
        \n\
        fn kept(xs: List[String], acc: Int) -> Int =\n\
        \x20 match xs {\n\
        \x20   [] => acc,\n\
        \x20   [x, ..] => kept(list.drop(xs, 1), acc + string.len(x) + (if list.is_empty(xs) then 1 else 0)),\n\
        \x20 }\n\
        \n\
        effect fn main() -> Unit = {\n\
        \x20 let xs = list.map(list.range(0, 4000), (i) => \"s${i}\")\n\
        \x20 let t0 = env.millis()\n\
        \x20 let a = moved(xs, 0) + moved(xs, 0) + moved(xs, 0) + moved(xs, 0) + moved(xs, 0)\n\
        \x20 let t1 = env.millis()\n\
        \x20 let b = kept(xs, 0) + kept(xs, 0) + kept(xs, 0) + kept(xs, 0) + kept(xs, 0)\n\
        \x20 let t2 = env.millis()\n\
        \x20 println(\"${a} ${b} ${t1 - t0} ${t2 - t1}\")\n\
        }\n";
    let (_dir, path, rust) = emit_rust("tail_perf.almd", src);
    assert!(fn_body(&rust, "moved").contains("almide_rt_list_drop_owned(xs, 1i64)"), "the moved shape lost its move");
    assert!(fn_body(&rust, "kept").contains("almide_rt_list_drop(&xs, 1i64)"), "the control no longer borrows");
    let run = Command::new(almide()).args(["run", path.to_str().unwrap()]).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    let f: Vec<i64> = stdout.split_whitespace().map(|w| w.parse().unwrap()).collect();
    assert_eq!(f.len(), 4, "unexpected output: {stdout}\n{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!((f[0], f[1]), (94450, 94450), "the two shapes disagree on the sum");
    let (moved_ms, kept_ms) = (f[2], f[3]);
    // ~70× apart when measured; fail only well short of that.
    assert!(
        moved_ms * 5 <= kept_ms.max(1),
        "moving the source at its last use is not faster than cloning its elements: \
         moved {moved_ms} ms vs kept {kept_ms} ms (#3398)"
    );
}
