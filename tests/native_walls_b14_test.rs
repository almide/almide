//! v1 walls, round 14 (family B): shapes the native render refused, now rendered,
//! each checked against its expected output on the native render and on wasm.

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| e.to_string()))
        .unwrap()
        .join()
        .unwrap()
}

fn run(src: &str, target: Option<&str>) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("p.almd");
    std::fs::write(&file, src).expect("source");
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(&file).env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1");
    if let Some(t) = target {
        cmd.arg("--target").arg(t);
    }
    let out = cmd.output().expect("almide run");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("walled"), "{err}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn both(src: &str, want: &str) {
    render(src).expect("renders on the native render");
    assert_eq!(run(src, None), want);
    assert_eq!(run(src, Some("wasm")), want);
}

// A lifted effect fn types its `if` at the Ok payload while each arm is the
// declared Result: the Err an arm's `!` builds is that Result's carrier.
const LIFTED_ARM_BANG: &str = r#"
import json
effect fn click(args: List[String]) -> Result[String, String] =
  if list.len(args) > 1 then err("too many") else ok("clicked")
effect fn res(s: String) -> Result[Value, String] =
  ok(value.array([value.string(s)]))
effect fn act(a: String) -> Result[Value, String] = if a == "one" then { let _ = click([a, a])! ; res(a) } else res(a)
effect fn main() -> Unit = {
  for a in ["one", "two", "zz"] {
    match act(a) {
      ok(v) => println(json.stringify(v)),
      err(e) => println("err ${e}"),
    }
  }
}
"#;

#[test]
fn a_bang_inside_a_lifted_effect_fn_arm_builds_the_declared_carrier() {
    both(LIFTED_ARM_BANG, "err too many\n[\"two\"]\n[\"zz\"]\n");
}

// A module `var` of a scalar Option lives in a heap slot whose native value is
// the Option itself.
const OPTION_SLOT: &str = r#"
var sock: Option[Int] = none
var serial = 0

fn fd() -> Int = match sock {
  some(s) => s,
  none => 0 - 1,
}

fn connect(n: Int) -> Unit = {
  sock = some(n * 3)
  serial = serial + 1
}

effect fn main() -> Unit = {
  println(int.to_string(fd()))
  connect(7)
  println(int.to_string(fd()))
  if option.is_none(sock) then println("none") else println("some")
  sock = none
  println(int.to_string(fd()))
  var t = 0
  for k in list.range(0, 6) {
    if k % 2 == 0 then connect(k) else { sock = none }
    t = t + fd()
  }
  println(int.to_string(t) + " " + int.to_string(serial))
}
"#;

#[test]
fn a_scalar_option_module_slot_is_read_and_written_by_value() {
    both(OPTION_SLOT, "-1\n21\nsome\n-1\n15 4\n");
}

// A `let`-bound match whose other arms are `err(..)` leaves the fn there; on the
// fn-tail chain the rest of the block continues inside the one arm that binds.
const ERR_ARM_BINDS: &str = r#"
fn pick(xs: List[Int], i: Int) -> Result[String, String] = {
  let a = match list.get(xs, i) {
    some(v) => v,
    none => err("no a at ${int.to_string(i)}"),
  }
  let s = "v${int.to_string(a)}"
  var b = match list.get(xs, a % 7) {
    some(v) => v * 2,
    none => err("no b"),
  }
  b = b + 1
  if b > 10 then ok(s + "/" + int.to_string(b)) else err("small ${s}")
}
fn rd(i: Int) -> Result[List[String], String] = if i % 3 == 0 then err("bad ${int.to_string(i)}") else ok(["x", "y"])
effect fn load(i: Int) -> Result[Int, String] = {
  let data = match rd(i) {
    ok(d) => d,
    err(e) => err(e),
  }
  ok(list.len(data) + i)
}
fn nested(o: Option[String], k: Int) -> Result[String, String] =
  if k > 0 then {
    let s = match o { some(x) => x, none => err("none here") }
    ok(s + "!")
  } else err("k")
effect fn main() -> Unit = {
  let xs = [3, 9, 12, 0, 5]
  for i in [0, 1, 2, 3, 4, 7] {
    match pick(xs, i) { ok(s) => println(s), err(e) => println("E " + e) }
    match load(i) { ok(n) => println(int.to_string(n)), err(e) => println("E " + e) }
  }
  match nested(some("a"), 1) { ok(s) => println(s), err(e) => println(e) }
  match nested(none, 1) { ok(s) => println(s), err(e) => println(e) }
  match nested(none, 0) { ok(s) => println(s), err(e) => println(e) }
}
"#;

#[test]
fn an_err_arm_bind_on_the_fn_tail_continues_in_its_binding_arm() {
    both(
        ERR_ARM_BINDS,
        "E small v3\nE bad 0\nv9/25\n3\nE no b\n4\nE small v0\nE bad 3\nE no b\n6\nE no a at 7\n9\na!\nnone here\nk\n",
    );
}
