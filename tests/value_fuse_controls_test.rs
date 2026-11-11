//! Where the optimizer's Value fusion (almide-optimize `value_fuse.rs`) fires
//! and where it must not, and that firing never changes what a program
//! prints or how it exits.
//!
//! Each program is lowered twice — as built, and with the pass switched off
//! (`ALMIDE_VALUE_FUSE_OFF`) — and run on the wasm leg both ways and on the
//! native leg as built: the three runs must agree byte for byte, exit code
//! included. Whether the pass fired is read off the linked IR: a document
//! read off its text is a `json.__spans` call (one per document), a direct
//! writer a `__almd_json_text_<fn>` twin and the absence of the
//! `json.stringify` call it replaced.

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
        readers: root.matches("\"func\":\"__spans\"").count(),
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
    let native = almide(&["run", path], false);
    assert_eq!(
        (String::from_utf8_lossy(&native.stdout), native.status.code()),
        (String::from_utf8_lossy(&on.stdout), on.status.code()),
        "{name}: the native run differs from the wasm one\nnative stderr: {}",
        String::from_utf8_lossy(&native.stderr),
    );
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

/// One reader program: `body` is the body of `fn read(text: String) -> String`,
/// run over a document of every kind of value, a refused one and no document.
fn reader(name: &str, extra: &str, body: &str) -> Seen {
    check(
        name,
        &program(&format!(
            r#"{extra}
fn read(text: String) -> String = {{
{body}
}}

fn all() -> String = "{{\"i\":7,\"f\":1.5,\"b\":true,\"s\":\"x\\ty\",\"n\":null,\"a\":[1,{{\"i\":2}}],\"o\":{{\"i\":3,\"s\":\"in\"}},\"data\":[{{\"i\":1,\"s\":\"p\"}},{{\"i\":2.5}},[],{{\"i\":4,\"o\":{{\"i\":5}}}}],\"items\":[{{\"i\":8}}]}}"

fn main() -> Unit = {{
  println(read(all()))
  println(read("[{{\"i\":1}},{{\"i\":2}},3]"))
  println(read("{{\"i\":1,\"i\":2}}"))
  println(read("{{\"i\":12345678901234567890}}"))
  println(read("nope"))
}}
"#
        )),
    )
}

/// The `value.get_*` family against the leaf kinds. The completeness rule:
/// a member that answers a scalar is a leaf read (`get_int`, `get_float`,
/// `get_bool`, `get_string`); `get_array` is fused as the rows of a row
/// site and nowhere else; `get_object` and a bare `get` hand out a piece of
/// the tree, and the `as_*` family answers a Result that names the kind it
/// found, so a document read through one of them is built. Every program
/// reads the same keys of the same documents, one key per kind of value.
#[test]
fn the_accessor_family_is_fused_by_matrix() {
    let keys = ["i", "f", "b", "s", "n", "a", "o", "none"];
    let scalar = |acc: &str, show: &str| -> String {
        let cells: Vec<String> = keys
            .iter()
            .map(|k| format!("(match value.{acc}(d, \"{k}\") {{ some(x) => {show}, none => \"-\" }})"))
            .collect();
        format!("  let d = json.parse(text) ?? value.null()\n  {}", cells.join(" + \",\" + "))
    };
    let whole = |acc: &str, show: &str| -> String {
        let cells: Vec<String> =
            keys.iter().map(|k| format!("(match value.{acc} {{ ok(x) => {show}, err(e) => e }})").replace("KEY", k)).collect();
        format!("  let d = json.parse(text) ?? value.null()\n  {}", cells.join(" + \",\" + "))
    };
    // (accessor, how its answer is shown, documents read off the text)
    let leaves = [
        ("get_int", "\"${x}\"", 1),
        ("get_float", "\"${float.to_bits(x)}\"", 1),
        ("get_bool", "\"${x}\"", 1),
        ("get_string", "\"<${x}>\"", 1),
        ("get_array", "\"${list.len(x)}\"", 0),
        ("get_object", "\"${list.len(x)}\"", 0),
        ("get", "json.stringify(x)", 0),
    ];
    for (acc, show, fused) in leaves {
        let seen = reader(&format!("matrix_{acc}"), "", &scalar(acc, show));
        assert_eq!(seen.readers, fused, "value.{acc} as a read of the document");
    }
    let results = [
        ("as_int(value.get(d, \"KEY\") ?? value.null())", "\"${x}\""),
        ("as_float(value.get(d, \"KEY\") ?? value.null())", "\"${float.to_bits(x)}\""),
        ("as_bool(value.get(d, \"KEY\") ?? value.null())", "\"${x}\""),
        ("as_string(value.get(d, \"KEY\") ?? value.null())", "\"<${x}>\""),
    ];
    for (i, (acc, show)) in results.into_iter().enumerate() {
        let seen = reader(&format!("matrix_as_{i}"), "", &whole(acc, show));
        assert_eq!(seen.readers, 0, "value.{acc}: the Result names the kind found, the tree is built");
    }
    // get_array as the rows of a row site, one leaf of each kind under it.
    let seen = reader(
        "matrix_rows",
        "",
        r#"  let d = json.parse(text) ?? value.null()
  let rows = (value.get_array(d, "data") ?? []) |> list.map((x) => {
    let o = value.get(x, "o") ?? value.null()
    "${value.get_int(x, "i") ?? -1}/${value.get_float(x, "i") ?? -1.0}/${value.get_bool(x, "i") ?? false}/${value.get_string(x, "s") ?? "?"}/${value.get_int(o, "i") ?? -1}"
  })
  "${rows}""#,
    );
    assert_eq!(seen.readers, 1);
}

#[test]
fn the_shapes_a_document_is_read_through() {
    // (name, body, documents read off the text)
    let shapes = [
        // An object root read at several paths, through `??`, `match`, an
        // alias and a reader function.
        (
            "object_root",
            r#"  let d = json.parse(text) ?? value.null()
  let o = value.get(d, "o") ?? value.null()
  let s = match value.get_string(o, "s") {
    some(x) => x,
    none => "none",
  }
  "${value.get_int(d, "i") ?? 0} ${value.get_float(d, "f") ?? 0.0} ${s} ${score(d)} ${value.get_bool(d, "b") == none}""#,
            1,
        ),
        // The parse spelled as a match; a fold straight over the rows.
        (
            "match_and_fold",
            r#"  let d = match json.parse(text) {
    ok(v) => v,
    err(_) => value.null(),
  }
  let n = list.fold(value.get_array(d, "data") ?? [], 0, (acc, x) => acc + (value.get_int(x, "i") ?? 100))
  "${n} ${value.get_int(d, "i") ?? 0}""#,
            1,
        ),
        // Two sites over the same rows, one of them in a loop: the scan
        // runs once, where the parse ran.
        (
            "rows_twice_and_in_a_loop",
            r#"  let d = json.parse(text) ?? value.null()
  let a = (value.as_array(d) ?? []) |> list.map((x) => value.get_int(x, "i") ?? 0)
  var total = 0
  for k in 0..<3 {
    total = total + ((value.as_array(d) ?? []) |> list.filter_map((x) => value.get_int(x, "i")) |> list.sum) + k
  }
  "${a} ${total}""#,
            1,
        ),
        // The text is not a variable, and a variable reassigned after the parse.
        (
            "text_expression",
            r#"  var t = text
  let d = json.parse(string.trim(" " + t + " ")) ?? value.null()
  let e = json.parse(t) ?? value.null()
  t = "[]"
  "${value.get_int(d, "i") ?? 0} ${value.get_int(e, "i") ?? 0} ${t}""#,
            2,
        ),
        // An unbound parse under a row site.
        (
            "inline_rows",
            r#"  let col = (value.as_array(json.parse(text) ?? value.null()) ?? []) |> list.map((x) => value.get_int(x, "i") ?? -1)
  "${col}""#,
            1,
        ),
        // any / all / count over the rows: no list of them is built.
        (
            "predicates",
            r#"  let d = json.parse(text) ?? value.null()
  let big = (value.get_array(d, "data") ?? []) |> list.any((x) => (value.get_int(x, "i") ?? 0) > 3)
  let every = (value.get_array(d, "data") ?? []) |> list.all((x) => (value.get_int(x, "i") ?? 0) > 3)
  "${big} ${every}""#,
            1,
        ),
    ];
    for (name, body, fused) in shapes {
        assert_eq!(reader(name, "", body).readers, fused, "{name}");
    }
}

#[test]
fn a_parse_unwrapped_in_an_effect_function_keeps_the_parsers_error() {
    let seen = check(
        "bang",
        &program(
            r#"effect fn read(text: String) -> Result[Int, String] = {
  let d = json.parse(text)!
  ok(value.get_int(d, "i") ?? -1)
}

fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok ${n}",
  err(e) => "err ${e}",
}

effect fn main() -> Unit = {
  println(show(read("{\"i\":5}")))
  println(show(read("{\"i\":5,}")))
  println(show(read("{\"i\":5,\"i\":6}")))
  println(show(read("")))
}
"#,
        ),
    );
    assert_eq!(seen.readers, 1);
}

#[test]
fn a_document_used_as_a_tree_keeps_its_tree() {
    // (name, extra functions, body): each use here needs the tree, or a
    // second row array, and the whole document is built.
    let controls = [
        // The document itself is written out.
        (
            "escapes",
            "",
            r#"  let d = json.parse(text) ?? value.null()
  let col = (value.as_array(d) ?? []) |> list.map(score)
  "${col} ${json.stringify(d)}""#,
        ),
        // A key computed at run time.
        (
            "dynamic_key",
            "fn key_of(n: Int) -> String = if n > 0 then \"i\" else \"s\"\n",
            r#"  let d = json.parse(text) ?? value.null()
  "${value.get_int(d, key_of(1)) ?? 0}""#,
        ),
        // The rows are handed back as Values.
        (
            "rows_handed_back",
            "",
            r#"  let d = json.parse(text) ?? value.null()
  let kept = (value.get_array(d, "data") ?? []) |> list.filter((x) => (value.get_int(x, "i") ?? 0) > 1)
  "${list.len(kept)}""#,
        ),
        // The callback uses its element whole.
        (
            "element_whole",
            "",
            r#"  let d = json.parse(text) ?? value.null()
  let col = (value.get_array(d, "data") ?? []) |> list.map((x) => "${value.get_int(x, "i") ?? 0}" + json.stringify(x))
  "${col}""#,
        ),
        // Two different arrays read by rows.
        (
            "two_row_arrays",
            "",
            r#"  let d = json.parse(text) ?? value.null()
  let a = (value.get_array(d, "data") ?? []) |> list.map((x) => value.get_int(x, "i") ?? 0)
  let b = (value.get_array(d, "items") ?? []) |> list.map((x) => value.get_int(x, "i") ?? 0)
  "${a} ${b}""#,
        ),
        // A function that is not a reader: it hands its Value on.
        (
            "not_a_reader",
            "fn through(v: Value) -> Int = list.len(value.as_array(v) ?? [])\n",
            r#"  let d = json.parse(text) ?? value.null()
  "${through(d)} ${value.get_int(d, "i") ?? 0}""#,
        ),
        // A reassignable document.
        (
            "var_document",
            "",
            r#"  var d = json.parse(text) ?? value.null()
  let n = value.get_int(d, "i") ?? 0
  d = value.null()
  "${n} ${value.get_int(d, "i") ?? 0}""#,
        ),
        // A parse whose failure is something other than null.
        (
            "other_fallback",
            "",
            r#"  let d = json.parse(text) ?? value.int(3)
  "${value.get_int(d, "i") ?? 0} ${value.as_int(d) ?? 0}""#,
        ),
        // An alias that escapes.
        (
            "alias_escapes",
            "",
            r#"  let d = json.parse(text) ?? value.null()
  let o = value.get(d, "o") ?? value.null()
  "${value.get_int(o, "i") ?? 0} ${json.stringify(o)}""#,
        ),
    ];
    for (name, extra, body) in controls {
        assert_eq!(reader(name, extra, body).readers, 0, "{name}: the tree must be built");
    }
}

/// The switch reaches every command that compiles: `emit` (every test
/// here), `build` and `test` — the module each produces is the unfused one.
#[test]
fn the_switch_is_honoured_by_build_and_test() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("switch_test.almd");
    let src = program(
        r#"fn read(text: String) -> Int = {
  let d = json.parse(text) ?? value.null()
  value.get_int(d, "i") ?? 0
}

fn main() -> Unit = println("${read("{\"i\":5}")}")

test "reads" {
  assert_eq(read("{\"i\":5}"), 5)
}
"#,
    );
    std::fs::write(&path, src).unwrap();
    let path = path.to_str().unwrap();
    let built = |off: bool| -> Vec<u8> {
        let out = dir.path().join(if off { "off.wasm" } else { "on.wasm" });
        let mut args = vec!["build", path, "--target", "wasm", "-o", out.to_str().unwrap()];
        if off {
            args.extend(["--allow-switch", "ALMIDE_VALUE_FUSE_OFF"]);
        }
        let run = almide(&args, off);
        assert!(run.status.success(), "build: {}", String::from_utf8_lossy(&run.stderr));
        std::fs::read(out).unwrap()
    };
    let (on, off) = (built(false), built(true));
    assert!(on.len() > off.len(), "the fused module carries the reader ({} bytes) and the unfused one does not ({})", on.len(), off.len());
    let tested = |off: bool| -> String {
        let run = almide(&["test", path, "--target", "wasm"], off);
        assert!(run.status.success(), "test: {}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
        String::from_utf8_lossy(&run.stdout).into_owned() + &String::from_utf8_lossy(&run.stderr)
    };
    // `almide test` reports the size of the module it ran.
    assert_ne!(tested(false), tested(true), "`almide test` built the same module with the switch on and off");
}

/// The row-site rule leans on the `list` module being written over an
/// element type throughout: a function of it that named Value could tell
/// its elements apart without the callback.
#[test]
fn no_list_function_is_written_over_value() {
    let dir = format!("{}/stdlib", env!("CARGO_MANIFEST_DIR"));
    let mut seen = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // The module's declarations; `list_*.almd` are its runtime bodies,
        // one per element representation, and name theirs.
        if name != "list.almd" {
            continue;
        }
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            if line.starts_with("fn ") || line.starts_with("effect fn ") {
                seen += 1;
                assert!(!line.contains("Value"), "{name}: a list function over Value: {line}");
            }
        }
    }
    assert!(seen > 50, "the list module was not found ({seen} functions)");
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
