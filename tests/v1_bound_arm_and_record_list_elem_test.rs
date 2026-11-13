//! Two shapes the certified native render refused (#4640 walls):
//!
//! - a heap-result `match`/`if` arm that is a value the arm lowering has no
//!   family for (`ok(ds) => (list.first(ds) ?? d).decl`). The arm is now
//!   lowered as the block `{ let t = <arm>; t }` the source could have
//!   spelled, which the bind lowering already carries;
//! - a list whose element is a list of records (`List[List[Tok]]`), appended
//!   to in a loop. The native render frees a list by its element type, so such
//!   an element has a drop there, as a heap tuple element already did.
//!
//! Each program prints one fixed text on the native render (ownership
//! certificate required, rendered by the certified render itself) and on wasm.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-bound-arm-{}-{tag}", std::process::id()));
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

const FIELD_OF_FALLBACK_ARM: &str = r#"type R = | RType | RName(String)
type D = | DCheck(R) | DSym(String)
type L = { line: Int, decl: D }

fn parse(s: String) -> Result[List[L], String] =
  if s == "" then err("empty") else ok([L { line: 1, decl: DSym(s) }, L { line: 2, decl: DCheck(RType) }])

fn first_decl(src: String) -> D = match parse(src) {
  ok(ds) => (list.first(ds) ?? L { line: 0, decl: DCheck(RType) }).decl,
  err(e) => DCheck(RName("parse error: " + e)),
}

fn last_name(src: String, fallback: String) -> String =
  if src == "" then fallback + "!" else (list.last(string.split(src, ",")) ?? fallback) + "?"

fn main() -> Unit = {
  println("${first_decl("ab")}")
  println("${first_decl("")}")
  println(last_name("a,b,c", "z"))
  println(last_name("", "z"))
}
"#;

#[test]
fn a_heap_arm_with_no_family_is_lowered_as_a_bound_local() {
    prints(
        "bound-arm",
        FIELD_OF_FALLBACK_ARM,
        "DSym(\"ab\")\nDCheck(RName(\"parse error: empty\"))\nc?\nz!\n",
    );
}

const LAYOUT_BLOCKS: &str = r#"type Tok = { text: String, line: Int, col: Int }

fn blocks(ts: List[Tok], base: Int) -> List[List[Tok]] = {
  var out: List[List[Tok]] = []
  var cur: List[Tok] = []
  var line = 0
  for t in ts {
    if t.line != line and t.col <= base and not list.is_empty(cur) then {
      list.push(out, cur)
      cur = []
    } else ()
    list.push(cur, t)
    line = t.line
  }
  if not list.is_empty(cur) then list.push(out, cur) else ()
  out
}

fn prefixes(ts: List[Tok]) -> List[List[Tok]] = {
  var out: List[List[Tok]] = []
  var cur: List[Tok] = []
  for t in ts {
    list.push(cur, t)
    list.push(out, cur)
  }
  out
}

fn show(bs: List[List[Tok]]) -> String =
  bs |> list.map((b) => b |> list.map((t) => t.text) |> list.join(",")) |> list.join(" | ")

fn main() -> Unit = {
  let ts = [Tok { text: "a", line: 1, col: 1 }, Tok { text: "b", line: 2, col: 3 }, Tok { text: "c", line: 3, col: 1 }, Tok { text: "d", line: 3, col: 5 }]
  println(show(blocks(ts, 1)))
  println(show(prefixes(ts)))
}
"#;

#[test]
fn a_list_of_record_lists_is_appended_in_a_loop() {
    prints("record-list-elem", LAYOUT_BLOCKS, "a,b | c,d\na | a,b | a,b,c | a,b,c,d\n");
}
