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
