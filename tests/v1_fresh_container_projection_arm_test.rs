//! A branch arm that projects a heap field or component off a call's result
//! (`ok(ds) => check_all(k, ds).error`, `if loud then halves(s).0 else s`).
//! The arm lowering borrowed a field only from a parameter or a local, so a
//! projection off a fresh value had nothing to borrow from and the whole
//! branch fell to "heap-result `match` outside the executable subset". The
//! arm now binds the call's result and projects from that binding, as the
//! block arm `{ let r = f(..); r.error }` always did.
use std::process::Command;

/// Record-field and tuple-component projections off a call, as match arms and
/// as if arms, with a loop that runs the arms often enough to show a leak.
const SHAPE: &str = r##"type Report = { count: Int, error: Option[String], names: List[String] }

fn parse(src: String) -> Result[List[String], String] =
  if src == "" then err("empty") else ok(string.split(src, ","))

fn check_all(base: Int, ds: List[String]) -> Report =
  if list.len(ds) > 2 then Report { count: base + list.len(ds), error: some("too many: " + int.to_string(list.len(ds))), names: ds }
  else Report { count: base, error: none, names: ds }

fn halves(s: String) -> (String, List[String]) = (string.to_upper(s), string.split(s, "-"))

fn check(src: String) -> Option[String] = match parse(src) {
  err(e) => some("parse: " + e),
  ok(ds) => check_all(1, ds).error,
}

fn names(src: String) -> List[String] = match parse(src) {
  err(e) => [e],
  ok(ds) => check_all(1, ds).names,
}

fn upper(s: String, loud: Bool) -> String =
  if loud then halves(s).0 else s

fn pieces(s: String, split: Bool) -> List[String] =
  if split then halves(s).1 else [s]

fn main() -> Unit = {
  for s in ["", "a,b", "a,b,c"] {
    println(check(s) ?? "fine")
    println(list.join(names(s), "+"))
  }
  println(upper("x-y", true) + " " + upper("x-y", false))
  println(list.join(pieces("x-y", true), "|") + " " + list.join(pieces("x-y", false), "|"))
  var total = 0
  for _i in 0..<2000 {
    total = total + string.len(check("a,b,c") ?? "") + list.len(pieces("p-q-r", true))
  }
  println(int.to_string(total))
}
"##;

const SHAPE_STDOUT: &str = "parse: empty\nempty\nfine\na+b\ntoo many: 3\na+b+c\nX-Y x-y\nx|y x-y\n28000\n";

/// The projections the arm already handled: off a parameter, off a local,
/// off a tuple parameter, and a scalar field off a call.
const CONTROL: &str = r##"type Report = { count: Int, error: Option[String], names: List[String] }

fn make(n: Int) -> Report =
  Report { count: n, error: if n > 1 then some("over " + int.to_string(n)) else none, names: ["n" + int.to_string(n)] }

fn of_param(r: Report, pick: Bool) -> Option[String] =
  if pick then r.error else none

fn of_local(n: Int) -> List[String] = {
  let r = make(n)
  if n > 0 then r.names else []
}

fn of_pair(p: (String, List[String]), first: Bool) -> String =
  if first then p.0 else list.join(p.1, ",")

fn count_of(n: Int, twice: Bool) -> Int =
  if twice then make(n).count * 2 else make(n).count

fn main() -> Unit = {
  println(of_param(make(2), true) ?? "none")
  println(of_param(make(2), false) ?? "none")
  println(of_param(make(1), true) ?? "none")
  println(list.join(of_local(3), ",") + "|" + list.join(of_local(0), ","))
  println(of_pair(("a", ["b", "c"]), true) + " " + of_pair(("a", ["b", "c"]), false))
  println(int.to_string(count_of(4, true)) + " " + int.to_string(count_of(4, false)))
}
"##;

const CONTROL_STDOUT: &str = "over 2\nnone\nnone\nn3|\na b,c\n8 4\n";

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
fn an_arm_that_projects_a_heap_field_off_a_call_returns_that_field() {
    runs_unwalled("fresh_projection_arm", SHAPE, SHAPE_STDOUT);
}

#[test]
fn projections_off_parameters_and_locals_are_borrowed_as_before() {
    runs_unwalled("fresh_projection_arm_control", CONTROL, CONTROL_STDOUT);
}
