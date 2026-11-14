//! v1 walls, round 23 (family B): shapes the native render refused, now
//! rendered, each checked against its expected output on the native render
//! and on wasm.

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

// A string-match command interpreter over `mut` params: a `!` call through a
// `mut` param, then a guard raising `err`, beside a `_ => err(…)` arm. The err
// carry pairs that raise with the buffers as `match err((msg, …)) { … }`, a
// known constructor whose payload allocates; it now reduces to the arm it
// selects, binding the payload once.
const MUT_PARAM_COMMANDS: &str = r#"
type Obj =
  | ONum(Int)
  | OName(String)
  | OList(List[Obj])
  | OGone
type St = { out: List[String], gens: Map[String, Int] }
local fn kind(o: Obj) -> String = match o { ONum(_) => "number", OName(_) => "name", OList(_) => "list", OGone => "gone" }
local fn pop(mut stack: List[Obj]) -> Obj! = match list.pop(stack) {
  some(o) => ok(o),
  none => err("the stack is empty"),
}
local fn pop_num(mut stack: List[Obj]) -> Int! = match pop(stack)! {
  ONum(n) => ok(n),
  o => err("expected a number, found " + kind(o)),
}
local fn command(mut st: St, mut stack: List[Obj], mut dict: Map[Int, Obj], cmd: String) -> Unit! = match cmd {
  "version" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "def" => {
    let k = pop_num(stack)!
    let x = pop(stack)!
    list.push(stack, x)
    dict = map.set(dict, k, x)
    ok(())
  },
  _ => err("unknown " + cmd),
}
local fn run(cmds: List[String], init: List[Obj]) -> String = {
  var st = St { out: [], gens: [:] }
  var s = init
  var d: Map[Int, Obj] = [:]
  var r = ""
  for c in cmds {
    r = r + (match command(st, s, d, c) { ok(_) => "ok;", err(e) => "err(${e});" })
  }
  "${r} stack=${list.len(s)} dict=${map.len(d)}"
}
effect fn main() -> Unit = {
  println(run(["version"], [ONum(6)]))
  println(run(["version"], [ONum(4)]))
  println(run(["version"], [OName("x")]))
  println(run(["nope"], [ONum(6)]))
  println(run(["def", "version"], [ONum(5), OName("a"), ONum(7)]))
}
"#;

#[test]
fn a_raise_beside_a_guarded_mut_param_arm_renders() {
    both(
        MUT_PARAM_COMMANDS,
        "ok; stack=0 dict=0\nerr(article version 4 is not supported); stack=0 dict=0\nerr(expected a number, found name); stack=0 dict=0\nerr(unknown nope); stack=1 dict=0\nok;err(expected a number, found name); stack=1 dict=1\n",
    );
}
