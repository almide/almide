//! Where the optimizer's Value fusion (almide-optimize `value_fuse.rs`) fires
//! and where it must not, and that firing never changes what a program
//! prints or how it exits.
//!
//! Each program is lowered twice — as built, and with the pass switched off
//! (`ALMIDE_VALUE_FUSE_OFF`) — and run on the wasm leg both ways: the two
//! runs must agree byte for byte, exit code included. Whether the pass fired
//! is read off the linked IR: a directed reader is a `__almd_json_ints_N`
//! helper, a direct writer a `__almd_json_text_<fn>` twin and the absence of
//! the `json.stringify` call it replaced.

use std::process::{Command, Output};

fn bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn almide(args: &[&str], off: bool) -> Output {
    let mut c = Command::new(bin());
    c.args(args);
    if off {
        c.env("ALMIDE_VALUE_FUSE_OFF", "1");
    } else {
        c.env_remove("ALMIDE_VALUE_FUSE_OFF");
    }
    c.output().expect("run the compiler")
}

struct Seen {
    readers: usize,
    twins: usize,
    stringify_calls: usize,
}

fn lowered(path: &str, off: bool) -> Seen {
    let out = almide(&["emit", path, "--emit-ir"], off);
    assert!(out.status.success(), "emit {path}: {}", String::from_utf8_lossy(&out.stderr));
    let ir = String::from_utf8_lossy(&out.stdout).replace("\": \"", "\":\"");
    let root = &ir[..ir.rfind("\"modules\"").unwrap_or(ir.len())];
    // A helper is named where it is defined and where it is called: count names.
    let distinct = |prefix: &str| -> usize {
        root.match_indices(prefix)
            .map(|(i, _)| root[i..].split('"').next().unwrap_or(""))
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    };
    Seen {
        readers: distinct("__almd_json_ints_"),
        twins: distinct("__almd_json_text_"),
        stringify_calls: root.matches("\"func\":\"stringify\"").count(),
    }
}

/// Runs `src` with and without the pass, requires the same output and exit
/// code, and returns what the pass built.
fn check(name: &str, src: &str) -> Seen {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("{name}.almd"));
    std::fs::write(&path, src).unwrap();
    let path = path.to_str().unwrap();
    let off = lowered(path, true);
    assert_eq!((off.readers, off.twins), (0, 0), "{name}: the switch must turn the pass off");
    let on = almide(&["run", path, "--target", "wasm"], false);
    let reference = almide(&["run", path, "--target", "wasm"], true);
    assert_eq!(
        (String::from_utf8_lossy(&on.stdout), on.status.code()),
        (String::from_utf8_lossy(&reference.stdout), reference.status.code()),
        "{name}: the fused run differs from the unfused one\nfused stderr: {}\nunfused stderr: {}",
        String::from_utf8_lossy(&on.stderr),
        String::from_utf8_lossy(&reference.stderr),
    );
    assert!(!on.stdout.is_empty() || on.status.code() != Some(0), "{name}: the program printed nothing");
    lowered(path, false)
}

const HEAD: &str = r#"import json

fn score(r: Value) -> Int = value.get_int(value.get(r, "meta") ?? value.null(), "score") ?? 0

fn note(i: Int, k: Int) -> Int = {
  println("field ${k} of ${i}")
  i * 10 + k
}

fn rec(i: Int) -> Value = value.object([
  ("id", value.int(i)),
  ("name", value.string("user_${i}")),
  ("meta", value.object([("score", value.int(i * 3)), ("on", value.bool(i % 2 == 0))])),
])

fn docs() -> String = "[{\"meta\":{\"score\":4}},{\"meta\":{\"score\":-9}},7,{\"meta\":{\"score\":1.5}}]"
"#;

fn program(body: &str) -> String {
    format!("{HEAD}\n{body}")
}

#[test]
fn the_round_trip_reads_and_writes_without_the_tree() {
    let seen = check(
        "round_trip",
        &program(
            r#"fn main() -> Unit = {
  let doc = value.array(list.range(0, 40) |> list.map(rec))
  let text = json.stringify(doc)
  let back = json.parse(text) ?? value.null()
  let sum = (value.as_array(back) ?? []) |> list.map(score) |> list.sum
  println("${sum} ${string.len(text)} ${text}")
}
"#,
        ),
    );
    assert_eq!((seen.readers, seen.twins), (1, 1));
    // The only stringify left is none: the array writer took the document.
    assert_eq!(seen.stringify_calls, 0);
}

#[test]
fn a_refused_document_reads_through_the_parser() {
    // Every text here is one the directed reader refuses or the parser
    // rejects; the answers are the tree's either way.
    let seen = check(
        "refusals",
        &program(
            r#"fn read(text: String) -> List[Int] = {
  let back = json.parse(text) ?? value.null()
  (value.as_array(back) ?? []) |> list.map(score)
}

fn main() -> Unit = {
  println("${read(docs())}")
  println("${read("[{\"m\\u0065ta\":{\"score\":2}}]")}")
  println("${read("[{\"meta\":{\"score\":2},\"meta\":{\"score\":3}}]")}")
  println("${read("[{\"meta\":{\"score\":2e0}},{\"meta\":{\"score\":12345678901234567890}}]")}")
  println("${read("[{\"meta\":{\"score\":2}},]")}")
  println("${read("{\"meta\":{\"score\":2}}")}")
  println("${read(string.repeat("[", 400) + string.repeat("]", 400))}")
  println("${read("")}")
}
"#,
        ),
    );
    assert_eq!(seen.readers, 1);
}

#[test]
fn a_parse_result_that_escapes_keeps_its_tree() {
    let seen = check(
        "escapes",
        &program(
            r#"fn main() -> Unit = {
  let back = json.parse(docs()) ?? value.null()
  let col = (value.as_array(back) ?? []) |> list.map(score)
  println("${col} ${json.stringify(back)}")
}
"#,
        ),
    );
    assert_eq!(seen.readers, 0);
}

#[test]
fn a_parse_result_read_twice_keeps_its_tree() {
    let seen = check(
        "read_twice",
        &program(
            r#"fn main() -> Unit = {
  let back = json.parse(docs()) ?? value.null()
  let a = (value.as_array(back) ?? []) |> list.map(score)
  let b = (value.as_array(back) ?? []) |> list.map(score)
  println("${a} ${b}")
}
"#,
        ),
    );
    assert_eq!(seen.readers, 0);
}

#[test]
fn a_dynamic_key_keeps_the_tree() {
    let seen = check(
        "dynamic_key",
        &program(
            r#"fn key_of(n: Int) -> String = if n > 0 then "score" else "id"

fn by_key(r: Value) -> Int = value.get_int(value.get(r, "meta") ?? value.null(), key_of(1)) ?? 0

fn main() -> Unit = {
  let back = json.parse(docs()) ?? value.null()
  let col = (value.as_array(back) ?? []) |> list.map(by_key)
  println("${col}")
}
"#,
        ),
    );
    assert_eq!(seen.readers, 0);
}

#[test]
fn a_read_inside_a_loop_or_of_a_reassigned_text_keeps_the_tree() {
    let seen = check(
        "loop_and_var",
        &program(
            r#"fn main() -> Unit = {
  let back = json.parse(docs()) ?? value.null()
  var total = 0
  for i in 0..<3 {
    total = total + ((value.as_array(back) ?? []) |> list.map(score) |> list.sum) + i
  }
  var text = docs()
  let again = json.parse(text) ?? value.null()
  text = "[]"
  let col = (value.as_array(again) ?? []) |> list.map(score)
  println("${total} ${col} ${text}")
}
"#,
        ),
    );
    assert_eq!(seen.readers, 0);
}

#[test]
fn a_tree_stringified_twice_or_handed_on_is_built() {
    let seen = check(
        "written_twice",
        &program(
            r#"fn size(v: Value) -> Int = list.len(value.as_array(v) ?? [])

fn main() -> Unit = {
  let doc = value.array(list.range(0, 5) |> list.map(rec))
  println(json.stringify(doc))
  println(json.stringify(doc))
  let other = value.array(list.range(0, 3) |> list.map(rec))
  println("${size(other)} ${json.stringify(other)}")
  println(json.stringify(value.int(3)))
}
"#,
        ),
    );
    assert_eq!(seen.twins, 0);
    assert_eq!(seen.stringify_calls, 4);
}

#[test]
fn field_effects_keep_their_order_and_an_aborting_field_aborts_where_it_did() {
    let seen = check(
        "effects",
        &program(
            r#"fn noisy(i: Int) -> Value = value.object([
  ("a", value.int(note(i, 1))),
  ("b", value.array([value.int(note(i, 2)), value.string("s${note(i, 3)}")])),
  ("c", value.int(note(i, 4))),
])

fn main() -> Unit = {
  println("start")
  let text = json.stringify(value.array(list.range(0, 3) |> list.map(noisy)))
  println(text)
}
"#,
        ),
    );
    assert_eq!(seen.twins, 1);
    let seen = check(
        "abort",
        &program(
            r#"let xs = [10, 20]

fn at(i: Int) -> Value = value.object([
  ("i", value.int(note(i, 1))),
  ("x", value.int(xs[i])),
  ("after", value.int(note(i, 2))),
])

fn main() -> Unit = {
  println("start")
  println(json.stringify(value.array([0, 1] |> list.map(at))))
  println(json.stringify(value.array([1, 2, 0] |> list.map(at))))
  println("unreached")
}
"#,
        ),
    );
    assert_eq!(seen.twins, 1);
}

#[test]
fn a_key_or_literal_that_needs_an_escape_and_a_repeated_key_go_to_the_runtime_writer() {
    let seen = check(
        "unclean",
        &program(
            r#"fn quoted(i: Int) -> Value = value.object([("a\"b", value.int(i))])

fn twice(i: Int) -> Value = value.object([("k", value.int(i)), ("k", value.int(i + 1))])

fn accent(i: Int) -> Value = value.object([("clé", value.int(i))])

fn mixed(i: Int) -> Value = {
  let s = if i % 2 == 0 then "q\"uo\\te\n" else "plain"
  value.object([
    ("lit", value.string("tab\there")),
    ("hole", value.string("v=${s}")),
    ("plain", value.string(s)),
    ("n", value.int(i)),
  ])
}

fn main() -> Unit = {
  let ns = [1, 2, 3]
  println(json.stringify(value.array(ns |> list.map(quoted))))
  println(json.stringify(value.array(ns |> list.map(twice))))
  println(json.stringify(value.array(ns |> list.map(accent))))
  println(json.stringify(value.array(ns |> list.map(mixed))))
}
"#,
        ),
    );
    // Only `mixed` has a twin: its unclean strings are holes the runtime
    // writer fills, its key spellings are clean.
    assert_eq!(seen.twins, 1);
}

#[test]
fn a_lone_tree_written_once_is_left_to_the_runtime_writer() {
    let seen = check(
        "lone",
        &program(
            r#"fn main() -> Unit = {
  println(json.stringify(rec(3)))
  println(json.stringify(value.object([("a", value.int(1)), ("b", value.array([value.null()]))])))
  let v = value.array([value.int(1), value.int(2)])
  println(json.stringify(v))
}
"#,
        ),
    );
    assert_eq!((seen.twins, seen.stringify_calls), (0, 3));
}
