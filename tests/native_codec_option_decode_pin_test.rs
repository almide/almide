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
