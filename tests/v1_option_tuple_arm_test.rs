//! A statement `match` over an Option whose `some` arm destructures a tuple
//! payload (`some((k, v)) => …`). The match that executes only the taken arm
//! asked for the payload bound whole (`some(p)`), so the destructuring
//! spelling fell to the both-arms form, which refuses any arm that calls or
//! assigns ("match over an UNTRACKED subject", and "if over an unresolvable
//! condition" when it sits in an `if` arm). The arm is now read as `some(p)`
//! with `let (k, v) = p` first in its body.
use std::process::Command;

/// A heap pair from a stdlib-forwarding fn inside an `if` inside a `for`, and
/// a scalar pair from a user fn driving a `while` through assignments.
const DESTRUCTURED: &str = r##"fn cut(s: String) -> Option[(String, String)] = string.split_once(s, "=")

fn step(xs: List[Int], at: Int) -> Option[(Int, Int)] =
  if at < list.len(xs) then some((list.get(xs, at) ?? 0, at + 1)) else none

fn pairs(src: String) -> List[String] = {
  var out: List[String] = []
  var n = 0
  for raw in string.lines(src) {
    n = n + 1
    let line = string.trim(raw)
    if line != "" and not string.starts_with(line, "#") then {
      match cut(line) {
        some((k, v)) => {
          let key = string.trim(k)
          if key == "" then list.push(out, "line ${n}: no key")
          else list.push(out, key + "=" + string.trim(v))
        },
        none => list.push(out, "line ${n}: no ="),
      }
    }
  }
  out
}

fn walk(xs: List[Int]) -> Int = {
  var total = 0
  var pos = 0
  var fine = true
  while fine {
    match step(xs, pos) {
      some((x, next)) => {
        total = total + x
        pos = next
      },
      none => { fine = false },
    }
  }
  total
}

effect fn main() -> Unit = {
  for p in pairs("a = 1\n# c\n\n = 2\nzz") { println(p) }
  println(int.to_string(walk([3, 4, 5])))
}
"##;

const DESTRUCTURED_STDOUT: &str = "a=1\nline 4: no key\nline 5: no =\n12\n";

/// Arms the reading does not apply to: a literal component, a guard that
/// reads the bound names, and a wildcard component beside them. Each is
/// selected as before.
const REFUTABLE: &str = r#"fn step(xs: List[Int], at: Int) -> Option[(Int, Int)] =
  if at < list.len(xs) then some((list.get(xs, at) ?? 0, at + 1)) else none

fn zeros(xs: List[Int]) -> List[String] = {
  var out: List[String] = []
  for i in 0..<list.len(xs) {
    match step(xs, i) {
      some((0, next)) => list.push(out, "zero before ${next}"),
      some((x, next)) if x > 5 => list.push(out, "big ${x}"),
      some((x, _)) => list.push(out, "plain ${x}"),
      none => list.push(out, "none"),
    }
  }
  out
}

effect fn main() -> Unit = {
  for p in zeros([0, 7, 2]) { println(p) }
}
"#;

const REFUTABLE_STDOUT: &str = "zero before 1\nbig 7\nplain 2\n";

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
fn a_some_arm_that_destructures_its_tuple_payload_runs_only_when_taken() {
    runs_unwalled("option_tuple_arm", DESTRUCTURED, DESTRUCTURED_STDOUT);
}

#[test]
fn literal_and_guarded_tuple_arms_are_selected_as_before() {
    runs_unwalled("option_tuple_arm_refutable", REFUTABLE, REFUTABLE_STDOUT);
}
