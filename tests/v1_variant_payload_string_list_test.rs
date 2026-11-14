//! A `List[String]` built from a variant's String payload
//! (`args |> list.map((a) => match a { RName(x) => x, _ => "" })`) used inline
//! as a tuple element. The payload read is a slot of a variant whose cases
//! disagree at that slot, so the native block typing had no kind for it, the
//! mapped list stayed a plain `Vec`, and storing the String into it was refused
//! (#4640). The bound payload's checked String declaration now seeds that kind.
//!
//! Each program prints one fixed text on the native render (ownership
//! certificate required, rendered by the certified render itself) and on wasm.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-payload-strlist-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of `almide run <src>` on `target`, the certificate required.
fn run(src: &Path, target: Option<&str>) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// `source` prints `expected` on the native render and on wasm, and the
/// native build is the certified render's own.
fn prints(tag: &str, source: &str, expected: &str) {
    if let Err(e) = almide_mir::pipeline::try_render_rust_source(source) {
        panic!("{tag} left the certified render: {e:?}");
    }
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    for target in [None, Some("wasm")] {
        let (stdout, stderr) = run(&src, target);
        assert_eq!(stdout, expected, "{tag} on {target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

const MAPPED_PAYLOAD_IN_TUPLE: &str = r##"type Raw =
  | RName(String)
  | RApp(Raw, Raw)
  | RType

fn build(items: List[String], args: List[Raw]) -> String = {
  var calls: List[(String, List[String])] = []
  for name in items {
    list.push(calls, (name, args |> list.map((a) => match a {
      RName(x) => x,
      _ => "",
    })))
  }
  "${calls}"
}

fn main() -> Unit = {
  println(build(["f", "g"], [RName("x"), RType, RName("y")]))
}
"##;

#[test]
fn a_mapped_string_payload_list_inside_a_pushed_tuple_renders() {
    prints("payload-tuple", MAPPED_PAYLOAD_IN_TUPLE, "[(\"f\", [\"x\", \"\", \"y\"]), (\"g\", [\"x\", \"\", \"y\"])]\n");
}

const NESTED_DEFS_SHAPE: &str = r##"type Raw =
  | RName(String)
  | RApp(Raw, Raw)
  | RType

fn spine(r: Raw) -> List[Raw] = match r {
  RApp(f, a) => spine(f) + [a],
  _ => [r],
}

fn build(items: List[(String, Int)], n: Int, compiled: Raw) -> String = {
  var motives: List[Raw] = list.repeat(RType, n)
  var minors: List[List[Raw]] = list.repeat([], n)
  var calls: List[(Raw, String, List[String])] = []
  for (name, kd) in items {
    let args = spine(compiled)
    motives = list.set(motives, kd, list.first(args) ?? RType)
    minors = list.set(minors, kd, list.take(list.drop(args, 1), 2))
    list.push(calls, (RName(name), name, list.drop(args, 2) |> list.map((a) => match a {
      RName(x) => x,
      _ => "",
    })))
  }
  "${motives} ${minors} ${calls}"
}

fn main() -> Unit = {
  println(build([("f", 0), ("g", 1)], 2, RApp(RApp(RApp(RName("h"), RName("x")), RType), RName("y"))))
}
"##;

#[test]
fn the_nested_defs_accumulators_render() {
    prints(
        "nested-defs",
        NESTED_DEFS_SHAPE,
        "[RName(\"h\"), RName(\"h\")] [[RName(\"x\"), RType], [RName(\"x\"), RType]] [(RName(\"f\"), \"f\", [\"\", \"y\"]), (RName(\"g\"), \"g\", [\"\", \"y\"])]\n",
    );
}
