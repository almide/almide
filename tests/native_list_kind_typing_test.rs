//! #4006: two ways an untyped list block takes its generated list kind on the
//! certified native render (crates/almide-mir/src/render_native_slots.rs
//! `across_concats`, render_native_addr.rs `note_signature_recs` with
//! native_floor_kinds.rs `generated_list_of`), each pinned byte-identical to
//! the v0 native pipeline:
//!
//! - an empty, never-stored list joined by a concatenation takes the kind of
//!   the generated list on the other side (`var xs: List[List[Int]] = []`
//!   before `xs = xs + [[…]]`), where it used to default to a String list;
//! - a list handed to a floor body whose param declares a list of `Value`s,
//!   matrices or `(String, Value)` pairs takes that list's kind
//!   (`value.array([])`, `value.object([])`), which no store can give it.
//!
//! And the no-perf-loss rule for the lists the first rule admits: a
//! one-element generated list that exists only to be appended has its element
//! pushed (render_native_addr.rs `single_rec`), and the append itself is in
//! place whatever typed drop releases the old accumulator
//! (render_native_append.rs `list_drop_of`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almd_native_list_kind_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// stdout + exit code of the v0 native pipeline (the oracle).
fn run_v0(name: &str, src: &str) -> (String, i32) {
    let file = scratch(name).join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(almide())
        .args(["run", "--no-verified", file.to_str().unwrap()])
        .env("ALMIDE_NO_VERIFIED_OK", "1")
        .output()
        .expect("almide run");
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

/// The v1 render, and its stdout + exit code once rustc compiled it.
fn run_v1(name: &str, src: &str) -> (String, String, i32) {
    let rust = almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("v1 native walled on `{name}`: {e:?}"));
    let dir = scratch(name);
    let (rs, bin) = (dir.join("prog.rs"), dir.join("prog_bin"));
    std::fs::write(&rs, &rust).unwrap();
    // The edition the product's native build compiles with (src/cli/build.rs).
    let rc = Command::new("rustc")
        .args(["--edition", "2021", "-O", "-o", bin.to_str().unwrap(), rs.to_str().unwrap()]).output().expect("rustc");
    assert!(rc.status.success(), "rustc rejected `{name}`:\n{}", String::from_utf8_lossy(&rc.stderr));
    let out = Command::new(&bin).output().expect("run v1 binary");
    (rust, String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

fn same(name: &str, src: &str) -> String {
    let (rust, out, code) = run_v1(name, src);
    assert_eq!((out, code), run_v0(name, src), "`{name}`: v1 native differs from v0");
    rust
}

fn in_a_big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e));
}

#[test]
fn an_empty_list_joined_by_a_concatenation_takes_the_generated_list_kind() {
    in_a_big_stack(|| {
        let rust = same(
            "concat_nested",
            "fn build() -> String = {\n  var outer: List[List[Int]] = []\n  for i in 0..<4 {\n    outer = outer + [[i * 10, i * 10 + 1]]\n  }\n  let v = outer[2]\n  int.to_string(v[1]) + \" \" + int.to_string(list.len(outer))\n}\n\nfn main() -> Unit = println(build())\n",
        );
        assert!(
            !rust.contains("Vec<AlmideString> = Vec::new()") && !rust.contains("Vec<AlmideString> = Vec::with_capacity"),
            "the empty List[List[Int]] still renders as a String list:\n{rust}"
        );
        // The one-element list `[[…]]` exists only to be appended: its element
        // is pushed, with no one-element list allocated per append.
        assert!(
            rust.lines().any(|l| l.contains(".push(v") && l.contains("_e0);")) && !rust.contains("_e0];"),
            "the appended one-element list of lists is allocated instead of its element pushed:\n{rust}"
        );
        // A list of String lists starting empty, the same way.
        same(
            "concat_str_rows",
            "fn main() -> Unit = {\n  var rows: List[List[String]] = []\n  for i in 0..<3 {\n    rows = rows + [[\"a${i}\", \"b\"]]\n  }\n  println(int.to_string(list.len(rows)))\n}\n",
        );
    });
}

#[test]
fn a_list_handed_to_a_floor_body_takes_the_declared_list_kind() {
    in_a_big_stack(|| {
        same(
            "floor_value_lists",
            "import json\n\nfn main() -> Unit = {\n  let v = value.object([\n    (\"a\", value.array([])),\n    (\"b\", value.object([])),\n    (\"c\", value.array([value.int(1), value.int(2)])),\n  ])\n  println(json.stringify(v))\n}\n",
        );
    });
}

/// The programs of spec/wasm_cross these two rules put on the native floor,
/// each byte-identical to v0.
#[test]
fn the_corpus_programs_the_rules_admit_match_v0() {
    in_a_big_stack(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for fx in [
            "cross_module_spread",
            "nested_three_level_loop_build",
            "json_stringify_pretty",
            "matrix_shape_family",
            "value_as_array_roundtrip",
            "value_merge",
            "value_merge_duplicate_keys",
        ] {
            let src = std::fs::read_to_string(root.join(format!("spec/wasm_cross/{fx}.almd"))).expect("fixture");
            let rust = same(fx, &src);
            // The tower's accumulators are released by a list-of-lists drop:
            // the append is still in place, no fresh list copying the
            // accumulator per append.
            if fx == "nested_three_level_loop_build" {
                assert!(!rust.contains("Vec::with_capacity(&v"), "`{fx}`: the accumulator is copied into a fresh list per append:\n{rust}");
            }
        }
    });
}
