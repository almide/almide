//! v1 walls, round 18 (family B): shapes the native render refused, now rendered,
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

// A list of (name, check) pairs filtered through its checks in a `let`: the
// bind position takes the value position's route for a tuple whose closure
// is a direct slot.
const TUPLE_CLOSURE_LIST_BIND: &str = r#"
type Config = { entries: List[(String, String)], problems: List[String] }
fn get(c: Config, k: String) -> Option[String] = list.find(c.entries, (p) => p.0 == k) |> option.map((p) => p.1)
fn unknown(c: Config) -> List[String] = c.entries |> list.filter((p) => p.0 != "size" and p.0 != "mode") |> list.map((p) => p.0)
fn checks() -> List[(String, (String) -> Bool)] = [
  ("size", (v) => string.len(v) < 3),
  ("mode", (v) => v == "on" or v == "off"),
]
fn bad(key: String, v: String) -> String = "${key}: can't use \"${v}\""
fn problems_in(c: Config, bells: List[String]) -> List[String] = {
  let unk = unknown(c) |> list.map((k) => "unknown key: ${k}")
  let bad_values = checks() |> list.filter_map(((key, valid)) => match get(c, key) {
    some(v) => if valid(v) then none else some(bad(key, v)),
    none => none,
  })
  let bad_bells = bells |> list.filter((b) => b != "" and b != "flash") |> list.map((b) => bad("bell-features", b))
  c.problems + unk + bad_values + bad_bells
}
effect fn main() -> Unit = {
  println(list.join(problems_in({ entries: [("size", "1234"), ("mode", "on"), ("zz", "1")], problems: ["p1"] }, ["flash", "boom"]), "|"))
}
"#;

// The same shape through map, fold, filter and find, each bound before use.
const TUPLE_CLOSURE_LIST_HOFS: &str = r#"
fn checks(lim: Int) -> List[(String, (String) -> Bool)] = [
  ("size", (v) => string.len(v) < lim),
  ("mode", (v) => v == "on" or v == "off"),
  ("any", (v) => v != ""),
]
fn mapped(xs: List[String]) -> List[String] = {
  let r = checks(3) |> list.map(((k, f)) => "${k}=${f("on")}")
  let first = r[0]
  [first + "|" + list.join(r, ",")] + xs
}
fn folded(v: String) -> Int = {
  let n = checks(5) |> list.fold(0, (acc, (k, f)) => if f(v) then acc + string.len(k) else acc)
  n * 2
}
fn kept(v: String) -> List[String] = {
  let ks = checks(2) |> list.filter(((k, f)) => f(v))
  let names = ks |> list.map((p) => p.0)
  names + [int.to_string(list.len(ks))]
}
fn found(v: String) -> String = {
  let hit = checks(4) |> list.find(((k, f)) => k == "mode")
  match hit {
    some((k, f)) => "${k}:${f(v)}",
    none => "none",
  }
}
effect fn main() -> Unit = {
  println(list.join(mapped(["z"]), ";"))
  println(int.to_string(folded("on")))
  println(list.join(kept("on"), ";"))
  println(found("off"))
  println(found("maybe"))
}
"#;

// Closures that capture a String, built and freed once per call.
const TUPLE_CLOSURE_LIST_HEAP_CAPTURE: &str = r#"
fn checks(tag: String) -> List[(String, (String) -> Bool)] = [
  ("size", (v) => string.len(v) < string.len(tag)),
  ("pre", (v) => string.starts_with(v, tag)),
]
fn bad(tag: String, v: String) -> List[String] = {
  let out = checks(tag) |> list.filter_map(((k, good)) => if good(v) then none else some("${k}:${v}"))
  out + [tag]
}
fn spin(i: Int, n: Int, acc: Int) -> Int =
  if i >= n then acc
  else spin(i + 1, n, acc + list.len(bad("tag-${int.to_string(i)}", "tag-1")))
effect fn main() -> Unit = {
  println(list.join(bad("tag-12", "tag-1"), ";"))
  println(int.to_string(spin(0, 30, 0)))
}
"#;

#[test]
fn a_tuple_closure_list_filtered_in_a_let() {
    both(TUPLE_CLOSURE_LIST_BIND, "p1|unknown key: zz|size: can't use \"1234\"|bell-features: can't use \"boom\"\n");
}

#[test]
fn a_tuple_closure_list_through_bound_hofs() {
    both(TUPLE_CLOSURE_LIST_HOFS, "size=true|size=true,mode=true,any=true;z\n22\nmode;any;2\nmode:true\nmode:false\n");
}

#[test]
fn a_tuple_closure_list_with_heap_captures() {
    both(TUPLE_CLOSURE_LIST_HEAP_CAPTURE, "pre:tag-1;tag-12\n69\n");
}
