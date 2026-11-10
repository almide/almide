//! `some((s, r))` as the value of a branch arm, where the pair holds a record
//! (or a list beside a record): an Option of a heap pair with no in-place
//! producer. The arm refused with "heap-result `if` outside the executable
//! subset" although the bound spelling `{ let t = (s, r); some(t) }` lowered;
//! the arm now binds the payload first, as `ok(..)` and `err(..)` already did.
use std::process::Command;

/// A pair of a string and a record, and of a list and a record, under `some`
/// in an `if` arm and a `match` arm, read back by destructuring, and in a loop.
const SHAPE: &str = r##"type Raw = { name: String, n: Int }

fn mk(s: String, i: Int) -> Raw = { name: s + "!", n: i }

fn pick(s: String, i: Int) -> Option[(String, Raw)] =
  if i < 0 then none
  else if string.len(s) > i then some((s, mk(s, i)))
  else none

fn held(s: String, i: Int) -> Option[(String, Raw)] = {
  let r: Raw = mk(s, i * 2)
  if i < 0 then none else some((s + "?", r))
}

fn names(xs: List[String], i: Int) -> Option[(List[String], Raw)] =
  match list.get(xs, i) {
    some(x) => some((list.take(xs, i), mk(x, i))),
    none => none,
  }

fn show(o: Option[(String, Raw)]) -> String =
  match o {
    some((s, r)) => "${s} ${r.name} ${int.to_string(r.n)}",
    none => "none",
  }

fn main() -> Unit = {
  println(show(pick("hello", 2)))
  println(show(pick("hello", 9)))
  println(show(pick("hello", -1)))
  println(show(held("hey", 3)))
  println(show(held("hey", -3)))
  match names(["a", "b", "c"], 2) {
    some((h, r)) => println("${list.join(h, ",")} ${r.name} ${int.to_string(r.n)}"),
    none => println("none"),
  }
  match names(["a"], 2) {
    some((h, r)) => println(list.join(h, ",") + r.name),
    none => println("none"),
  }
  var total = 0
  for i in 0..<200000 {
    total = total + (match pick("abcdef", i % 9) {
      some((s, r)) => string.len(s) + r.n,
      none => 1,
    })
  }
  println(int.to_string(total))
}
"##;

const SHAPE_STDOUT: &str = "hello hello! 2\nnone\nnone\nhey? hey! 6\nnone\na,b c! 2\nnone\n1200001\n";

/// Payloads that never needed the route: a string-and-int pair, a record from
/// a call, and a pair of strings.
const CONTROL: &str = r##"type Raw = { name: String, n: Int }

fn mk(s: String, i: Int) -> Raw = { name: s + "!", n: i }

fn pair(s: String, i: Int) -> Option[(String, Int)] =
  if i < 0 then none else some((s, i))

fn rec(s: String, i: Int) -> Option[Raw] =
  if i < 0 then none else some(mk(s, i))

fn both(s: String, i: Int) -> Option[(String, String)] =
  if i < 0 then none else some((s, s + "+"))

fn main() -> Unit = {
  match pair("p", 4) {
    some((s, n)) => println(s + int.to_string(n)),
    none => println("none"),
  }
  match pair("p", -4) {
    some((s, n)) => println(s + int.to_string(n)),
    none => println("none"),
  }
  match rec("r", 5) {
    some(r) => println(r.name + int.to_string(r.n)),
    none => println("none"),
  }
  match both("b", 1) {
    some((a, b)) => println(a + b),
    none => println("none"),
  }
}
"##;

const CONTROL_STDOUT: &str = "p4\nnone\nr!5\nbb+\n";

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
fn some_of_a_pair_holding_a_record_in_a_branch_arm_is_built_and_destructured() {
    runs_unwalled("option_pair_record_arm", SHAPE, SHAPE_STDOUT);
}

#[test]
fn scalar_pair_record_and_string_pair_payloads_are_built_as_before() {
    runs_unwalled("option_pair_record_arm_control", CONTROL, CONTROL_STDOUT);
}
