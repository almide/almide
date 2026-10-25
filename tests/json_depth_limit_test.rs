//! #4209 / C-434 — `json.set_path` refuses to build a value nested deeper
//! than `json.parse` accepts, with parse's own text, on every leg.
//!
//! Before the fix set_path had no bound: a dotted key expanded into a path
//! built a value 40,000 deep, and every recursive walk of it (stringify,
//! `==`, the drop, set_path itself) overflowed the stack — natively past
//! ~40k, on stock wasmtime past ~5k. The bound is the one parse has
//! (#4020, C-391): a value's containers may nest JSON_MAX_DEPTH deep, and
//! set_path's result nests its path's steps plus the new value's own
//! nesting. Every ok result therefore round-trips through `json.parse`.
//!
//! The limit is declared once (crates/almide-base/src/json_limits.rs): the
//! native runtime splices that file, the structural wasm leg reads it, and
//! the self-hosted bodies (the wasm parse, the interp oracle's parse and
//! set_path) read stdlib/json_parse.almd's `__json_max_depth`, which the
//! last test pins to the constant.

use std::process::Command;

use almide_base::json_limits::{JSON_MAX_DEPTH, json_depth_error};

const SOURCE: &str = r#"import json

fn path_of(d: Int) -> JsonPath = list.range(0, d) |> list.fold(json.root(), (acc, _i) => json.field(acc, "k"))

fn nest(d: Int) -> Value = list.range(0, d) |> list.fold(value.int(1), (acc, _i) => value.array([acc]))

fn show(r: Result[Value, String]) -> String = match r {
  ok(v) => "ok ${string.len(json.stringify(v))} ${json.parse(json.stringify(v)) |> result.is_ok}",
  err(e) => "err ${e}",
}

effect fn main() -> Unit = {
  let doc = json.parse("{\"a\": [1, 2]}")!
  println(show(json.set_path(doc, path_of(1000), value.int(1))))
  println(show(json.set_path(doc, path_of(1001), value.int(1))))
  println(show(json.set_path(doc, path_of(999), value.array([]))))
  println(show(json.set_path(doc, path_of(1000), value.array([]))))
  println(show(json.set_path(doc, path_of(1000), value.object([]))))
  println(show(json.set_path(doc, json.root(), nest(1000))))
  println(show(json.set_path(doc, json.root(), nest(1001))))
  println(show(json.set_path(doc, path_of(500), nest(500))))
  println(show(json.set_path(doc, path_of(500), nest(501))))
  println(show(json.set_path(doc, path_of(500), value.object([("x", nest(499)), ("y", value.int(2))]))))
  println(show(json.set_path(doc, path_of(500), value.object([("y", value.int(2)), ("x", nest(500))]))))
  println(show(json.set_path(doc, json.index(json.field(json.root(), "a"), 0), value.int(9))))
  println(show(json.set_path(doc, path_of(40000), value.int(1))))
}
"#;

const EXPECTED: &str = "ok 6011 true
err json: nesting deeper than 1000
ok 6006 true
err json: nesting deeper than 1000
err json: nesting deeper than 1000
ok 2001 true
err json: nesting deeper than 1000
ok 4011 true
err json: nesting deeper than 1000
ok 4021 true
err json: nesting deeper than 1000
ok 11 true
err json: nesting deeper than 1000
";

fn write_source() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("json_depth.almd");
    std::fs::write(&file, SOURCE).expect("write source");
    (dir, file)
}

fn run(file: &std::path::Path, wasm: bool) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "wasm={wasm}: exit {:?}\n{}", out.status.code(), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn set_path_refuses_past_the_parse_bound_on_both_legs() {
    let (_dir, file) = write_source();
    for wasm in [false, true] {
        assert_eq!(run(&file, wasm), EXPECTED, "wasm={wasm}: set_path's depth bound moved");
    }
}

/// The interp oracle runs the self-hosted json_path.almd body. It abstains
/// on a Value hundreds of levels deep (its own walk limit, a resource
/// abstention), so its vote is cast on the refusals — which build nothing
/// deep — and on a shallow ok.
const INTERP_SOURCE: &str = r#"import json

fn path_of(d: Int) -> JsonPath = list.range(0, d) |> list.fold(json.root(), (acc, _i) => json.field(acc, "k"))

fn show(r: Result[Value, String]) -> String = match r {
  ok(v) => "ok ${json.stringify(v)}",
  err(e) => "err ${e}",
}

effect fn main() -> Unit = {
  let doc = json.parse("{\"a\": [1, 2]}")!
  println(show(json.set_path(doc, path_of(3), value.int(1))))
  println(show(json.set_path(doc, path_of(1001), value.int(1))))
  println(show(json.set_path(doc, path_of(40000), value.int(1))))
  println(show(json.set_path(doc, path_of(999), value.array([value.array([])]))))
  println(show(json.set_path(doc, path_of(1000), value.object([]))))
}
"#;

const INTERP_EXPECTED: &str = r#"ok {"a":[1,2],"k":{"k":{"k":1}}}
err json: nesting deeper than 1000
err json: nesting deeper than 1000
err json: nesting deeper than 1000
err json: nesting deeper than 1000
"#;

#[test]
fn the_interp_oracle_agrees() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("json_depth_interp.almd");
    std::fs::write(&file, INTERP_SOURCE).expect("write source");
    for wasm in [false, true] {
        assert_eq!(run(&file, wasm), INTERP_EXPECTED, "wasm={wasm}");
    }
    let path = file.to_str().unwrap().to_string();
    let out = std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(move || {
            let ir = almide::wasm_leg::lower_to_ir(&path, INTERP_SOURCE).expect("front");
            almide_interp::Interpreter::new(&ir).run_main()
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(out.status, almide_interp::RunStatus::Ok, "interp: {}", out.stderr);
    assert_eq!(out.stdout, INTERP_EXPECTED, "interp: set_path's depth bound moved");
}

/// The self-hosted bodies carry the bound as a literal (each self-hosted
/// source links on its own, so json_parse.almd and json_path.almd each read
/// it through one fn); both must be the one declaration's value, and the
/// err text the one declared text.
#[test]
fn the_self_hosted_bound_is_the_declared_one() {
    assert_eq!(json_depth_error(), format!("json: nesting deeper than {JSON_MAX_DEPTH}"));
    for (file, decl) in [("json_parse.almd", "fn __json_max_depth() -> Int = "), ("json_path.almd", "fn __jpath_max_depth() -> Int = ")] {
        let src = std::fs::read_to_string(format!("{}/stdlib/{file}", env!("CARGO_MANIFEST_DIR"))).expect("stdlib source");
        let line = src.lines().find(|l| l.starts_with(decl)).unwrap_or_else(|| panic!("{file}: no `{decl}…` declaration"));
        let lit: usize = line[decl.len()..].trim().parse().expect("a literal bound");
        assert_eq!(lit, JSON_MAX_DEPTH, "{file}'s bound drifted from crates/almide-base/src/json_limits.rs");
        let second = src.lines().filter(|l| !l.trim_start().starts_with("//") && !l.starts_with(decl)).any(|l| l.contains(&JSON_MAX_DEPTH.to_string()));
        assert!(!second, "{file}: a second literal bound — read the declaring fn");
    }
}
