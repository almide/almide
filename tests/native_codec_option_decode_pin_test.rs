//! NATIVE-leg regression pin for #1713 (0.60.0 regression, the "check green
//! must build" class).
//!
//! A derived `Codec` type living in an imported module, with an Option-of-heap
//! field (`Option[List[Record]] = none` is the reported shape), infers its
//! decode's `&Value` borrow only in fixed-point round 1 — round 0 runs before
//! the generated option workers' signatures exist. The MIRROR sig keys a
//! cross-module call site resolves through (`Thing.decode` et al.) were
//! `or_insert`-frozen at round 0's `Own`, so the definition emitted
//! `decode(_v: &Value)` while the call site kept passing `Value` by value —
//! rustc E0308 on a program `check` had accepted.
//!
//! The wasm leg never had the bug (`almide test` runs there — A/B-verified:
//! the pre-fix 0.61.0 binary passes a spec/ fixture of this same shape), so
//! per the tests/native_mut_param_pins_test.rs doctrine this pins at the
//! compiler level on the NATIVE target; spec/stdlib/codec_field_matrix_test
//! pins the cross-target field semantics.

use std::io::Write;
use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

/// Write `files` under a temp package dir, `almide run` (NATIVE target) the
/// first one, assert it prints `expected`.
fn run_prints(name: &str, files: &[(&str, &str)], expected: &str) {
    let dir = std::env::temp_dir().join(format!("almd_codec_opt_pin_{name}_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("almide.toml"),
        "[package]\nname = \"pins\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    for (rel, src) in files {
        let file = dir.join(rel);
        let mut f = std::fs::File::create(&file).unwrap();
        f.write_all(src.as_bytes()).unwrap();
    }
    let entry = dir.join(files[0].0);
    let out = Command::new(almide_bin())
        .args(["run", entry.to_str().unwrap()])
        .output()
        .expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        out.status.success(),
        "[{name}] almide run (native) failed — the pinned native codegen bug is back?\n{stderr}"
    );
    assert_eq!(stdout.trim_end(), expected, "[{name}] wrong output");
}

#[test]
fn cross_module_decode_with_option_list_record_field_builds_native() {
    run_prints(
        "opt_list_rec",
        &[
            (
                "src/main.almd",
                r#"import self.thing
import json

effect fn main() -> Unit = {
  match thing.Thing.decode(json.parse("{\"schema\":\"s1\"}") ?? value.null()) {
    Ok(t) => {
      println(t.schema)
      match t.extra {
        some(_) => println("some"),
        none => println("none"),
      }
    },
    Err(e) => println("bad: " + e),
  }
  match thing.Thing.decode(json.parse("{\"schema\":\"s2\",\"extra\":[{\"name\":\"a\"},{\"name\":\"b\"}]}") ?? value.null()) {
    Ok(t) => match t.extra {
      some(items) => println(int.to_string(list.len(items))),
      none => println("none"),
    },
    Err(e) => println("bad: " + e),
  }
}
"#,
            ),
            (
                "src/thing.almd",
                r#"type Item: Codec = {
  name: String,
}

type Thing: Codec = {
  schema: String,
  extra: Option[List[Item]] = none,
}
"#,
            ),
        ],
        "s1\nnone\n2",
    );
}

/// #2052 (0.62.0, the same "check green must build" class, single module).
/// The `T?` × {record with a default field, record without, list, primitive}
/// matrix on one derived decode, the outer record itself carrying a default
/// (so its own decode uses `__decode_default_*`), and a third level wrapping
/// the whole thing in an Option. Before the fix the record-with-default cell
/// emitted `decode_option_custom(_v, ..)` with `_v: &AlmideValue` — E0308.
#[test]
fn option_field_matrix_with_defaulted_records_builds_native() {
    run_prints(
        "opt_matrix",
        &[(
            "src/main.almd",
            r#"import json

type WithDefault: Codec = { city: String, zip: String = "0" }
type Plain: Codec = { city: String }
type Rec: Codec = { name: String, a: WithDefault?, b: Plain?, c: List[Int]?, d: Int?, e: List[WithDefault]?, f: String = "t" }
type Outer: Codec = { r: Rec?, n: Int = 1 }

fn show(r: Rec) -> String = {
  let a = match r.a { some(x) => x.city + "/" + x.zip, none => "-" }
  let b = match r.b { some(x) => x.city, none => "-" }
  let c = match r.c { some(xs) => int.to_string(list.len(xs)), none => "-" }
  let d = match r.d { some(x) => int.to_string(x), none => "-" }
  let e = match r.e { some(xs) => int.to_string(list.len(xs)), none => "-" }
  r.name + " " + a + " " + b + " " + c + " " + d + " " + e + " " + r.f
}

effect fn main() -> Unit = {
  let full = json.parse("{\"name\":\"x\",\"a\":{\"city\":\"p\"},\"b\":{\"city\":\"q\"},\"c\":[1,2],\"d\":7,\"e\":[{\"city\":\"r\",\"zip\":\"9\"}],\"f\":\"u\"}")!
  match Rec.decode(full) { ok(r) => println(show(r)), err(e) => println(e) }
  let bare = json.parse("{\"name\":\"y\"}")!
  match Rec.decode(bare) { ok(r) => println(show(r)), err(e) => println(e) }
  let outer = json.parse("{\"r\":{\"name\":\"z\",\"a\":{\"city\":\"s\",\"zip\":\"1\"}}}")!
  match Outer.decode(outer) {
    ok(o) => match o.r { some(r) => println(show(r) + " " + int.to_string(o.n)), none => println("none") },
    err(e) => println(e),
  }
}
"#,
        )],
        "x p/0 q 2 7 1 u\ny - - - - - t\nz s/1 - - - - t 1",
    );
}
