//! `fan.map(xs, <callback whose body propagates>)!` on the native render
//! (C-004, ADR-0024 D1: every element runs, the lowest-index `Err` is the
//! result). The lowering refused the consumed spelling fn-wide (#1865) while
//! the un-consumed ones (`?? fb`, a `match` over the Result) lowered; the
//! native render now lowers it through the same route — the unwrap ladder's
//! hoisted `match` over the call, the callback on its own Result channel.
//! `spec/embedded_cross/fan_mapper_effect_callback_forms.almd` is the corpus
//! program this unwalls.
use std::process::Command;

/// Record, list and scalar elements, a captured value, a `for` iterable, the
/// `?` consumer, 2000 elements, and a propagation out of `main`.
const CONSUMED: &str = r#"effect fn helper(n: Int) -> Result[String, String] = if n > 3 then err("big ${n}") else ok("v${n}")
effect fn nums(n: Int) -> Result[List[Int], String] = if n < 0 then err("neg ${n}") else ok([n, n * 2])
effect fn unit_ok(n: Int) -> Result[Int, String] = if n == 13 then err("unlucky") else ok(n + 1)

type P = { name: String, n: Int }

effect fn recs(xs: List[Int]) -> Result[List[P], String] = {
  let ps = fan.map(xs, (x) => {
    let s = helper(x)!
    P { name: s, n: x }
  })!
  ok(ps)
}

effect fn lists(xs: List[Int]) -> Result[Int, String] = {
  let ls = fan.map(xs, (x) => nums(x)!)!
  ok(list.len(list.flatten(ls)))
}

effect fn ints(xs: List[Int]) -> Result[Int, String] = {
  var total = 0
  for v in fan.map(xs, (x) => {
    println("  ran ${x}")
    unit_ok(x)!
  })! {
    total = total + v
  }
  ok(total)
}

effect fn captured(prefix: String, xs: List[Int]) -> Result[String, String] = {
  let out = fan.map(xs, (x) => prefix + helper(x)!)!
  ok(list.join(out, " "))
}

effect fn main() -> Unit = {
  let r1: Result[List[P], String] = recs([1, 2, 3])
  println(match r1 { ok(ps) => list.join(ps |> list.map((p) => "${p.name}:${p.n}"), ","), err(e) => "err " + e })
  let r2: Result[List[P], String] = recs([1, 9, 8])
  println(match r2 { ok(ps) => "ok ${list.len(ps)}", err(e) => "err " + e })
  let l1: Result[Int, String] = lists([1, 2, 3])
  println("${l1}")
  let l2: Result[Int, String] = lists([1, 0 - 2, 0 - 3])
  println("${l2}")
  let i1: Result[Int, String] = ints([1, 2, 3])
  println("${i1}")
  let i2: Result[Int, String] = ints([1, 13, 3, 13])
  println("${i2}")
  let c1: Result[String, String] = captured("p-", [1, 2])
  println("${c1}")
  let c2: Result[String, String] = captured("p-", [5, 6])
  println("${c2}")
  let o1: Option[List[String]] = fan.map([1, 2], (x) => helper(x)!)?
  println("${o1}")
  let o2: Option[List[String]] = fan.map([1, 8], (x) => helper(x)!)?
  println("${o2}")
  let big = fan.map(list.range(0, 2000), (x) => helper(x % 3)!)!
  println(int.to_string(list.len(big)))
  let dies = fan.map([1, 2, 7], (x) => helper(x)!)!
  println("unreachable ${list.len(dies)}")
}
"#;

const CONSUMED_STDOUT: &str = "v1:1,v2:2,v3:3\nerr big 9\nok(6)\nerr(\"neg -2\")\n  ran 1\n  ran 2\n  ran 3\nok(9)\n  ran 1\n  ran 13\n  ran 3\n  ran 13\nerr(\"unlucky\")\nok(\"p-v1 p-v2\")\nerr(\"big 5\")\nsome([\"v1\", \"v2\"])\nnone\n2000\n";

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| format!("{e:?}")))
        .expect("spawn")
        .join()
        .expect("render thread")
}

#[test]
fn a_consumed_fan_map_with_a_propagating_callback_runs_every_element_on_the_native_render() {
    render(CONSUMED).unwrap_or_else(|e| panic!("the consumed fan.map walls: {e}"));
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("consumed.almd");
    std::fs::write(&file, CONSUMED).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra).output().expect("almide run");
        assert_eq!(String::from_utf8_lossy(&out.stdout), CONSUMED_STDOUT, "{extra:?}");
        assert_eq!(out.status.code(), Some(1), "{extra:?}: the last `!` ends main");
        assert!(String::from_utf8_lossy(&out.stderr).contains("Error: big 7"), "{extra:?}");
    }
}

#[test]
fn the_callback_forms_fixture_renders_on_the_native_route() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/embedded_cross/fan_mapper_effect_callback_forms.almd");
    let src = std::fs::read_to_string(path).expect("fixture");
    render(&src).unwrap_or_else(|e| panic!("the fixture walls: {e}"));
}

/// Off the native render the refusal stands: the shared lowering lowers the
/// helpers and none of the fns that hold the consumed spelling.
#[test]
fn the_shared_lowering_still_refuses_the_consumed_spelling() {
    let lowered = almide_mir::pipeline::lowered_functions(CONSUMED).expect("the helpers lower");
    let names: Vec<&str> = lowered.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"helper"), "{names:?}");
    for refused in ["recs", "lists", "ints", "captured", "main"] {
        assert!(!names.contains(&refused), "`{refused}` lowered off the native render: {names:?}");
    }
}
