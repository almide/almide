//! `ok(..)` / `some(..)` / `err(..)` bound to a name with a record literal, a
//! record spread or a field projection as its payload (`let r = ok(Comment {
//! .. })` — the expected value of an equality assertion is this bind). The
//! bind built call, tuple and branch payloads through a `let` of their own
//! first; a record or projection payload had no such route and refused with
//! "ResultOk heap bind cannot be faithfully materialized". They take the same
//! route now.
use std::process::Command;

/// Record, spread and projection payloads under ok, some and err, compared
/// with a decoded value, and built in a loop.
const SHAPE: &str = r##"type Src = | A | B
type Comment = { id: String, color: Option[String], source: Src }

fn dec(s: String) -> Result[Comment, String] =
  if s == "" then err("empty") else ok(Comment { id: s, color: none, source: A })

fn show(r: Result[Comment, String]) -> String = match r {
  ok(c) => "ok " + c.id + " " + (c.color ?? "-"),
  err(e) => "err " + e,
}

fn main() -> Unit = {
  let c = Comment { id: "k", color: some("red"), source: B }
  let r: Result[Comment, String] = ok(Comment { id: "x", color: none, source: A })
  println(show(r))
  let q: Result[String, String] = ok(c.id)
  println(q ?? "none")
  let o: Option[Comment] = some(Comment { id: "y", color: some("blue"), source: B })
  println(match o { some(v) => v.id + " " + (v.color ?? "-"), none => "none" })
  let w: Option[Comment] = some({ ...c, id: "spread" })
  println(match w { some(v) => v.id + " " + (v.color ?? "-"), none => "none" })
  let e: Result[String, Comment] = err(Comment { id: "bad", color: none, source: A })
  println(match e { ok(s) => s, err(v) => "err " + v.id })
  if dec("x") == r then println("same") else println("different")
  if dec("z") == r then println("same") else println("different")
  if dec("") == r then println("same") else println("different")
  var n = 0
  for i in 0..<2000 {
    let t: Result[Comment, String] = ok(Comment { id: "i" + int.to_string(i), color: none, source: A })
    n = n + string.len(show(t))
  }
  println(int.to_string(n))
}
"##;

const SHAPE_STDOUT: &str = "ok x -\nk\ny blue\nspread red\nerr bad\nsame\ndifferent\ndifferent\n18890\n";

/// Payloads that never needed the route: a bound record, a string literal,
/// a scalar, a list literal and a concatenation.
const CONTROL: &str = r##"type Comment = { id: String, color: Option[String] }

fn show(r: Result[Comment, String]) -> String = match r {
  ok(c) => "ok " + c.id + " " + (c.color ?? "-"),
  err(e) => "err " + e,
}

fn main() -> Unit = {
  let c = Comment { id: "k", color: some("red") }
  let r: Result[Comment, String] = ok(c)
  println(show(r))
  println(c.id)
  let s: Result[String, String] = ok("lit")
  println(s ?? "none")
  let n: Option[Int] = some(c.id |> string.len)
  println(int.to_string(n ?? 0))
  let l: Option[List[String]] = some([c.id, "z"])
  println(list.join(l ?? [], ","))
  let e: Result[Comment, String] = err("no " + c.id)
  println(show(e))
}
"##;

const CONTROL_STDOUT: &str = "ok k red\nk\nlit\n1\nk,z\nerr no k\n";

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| format!("{e:?}")))
        .expect("spawn")
        .join()
        .expect("render thread")
}

fn runs_unwalled(name: &str, src: &str, stdout: &str) {
    render(src).unwrap_or_else(|e| panic!("{name} walls: {e}"));
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{name} {extra:?}");
    }
}

#[test]
fn a_record_or_projection_payload_under_ok_some_and_err_is_built_and_read_back() {
    runs_unwalled("ctor_record_payload", SHAPE, SHAPE_STDOUT);
}

#[test]
fn variable_literal_and_list_payloads_are_built_as_before() {
    runs_unwalled("ctor_record_payload_control", CONTROL, CONTROL_STDOUT);
}
