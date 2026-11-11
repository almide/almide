//! `list.get(ts, p) ?? "lit"` over a borrowed `List[String]` parameter, the
//! result only compared or measured (a parser's token test) and the result
//! escaping (returned, stored, appended to): the certified render reads the
//! first through a borrow and shares the element's handle for the second.
//! Both must print what the previous release printed, on both legs, under the
//! ownership certifier — present, empty, absent and negative positions.
use std::process::Command;

const SHAPE: &str = r#"fn eq_at(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn bound_eq(ts: List[String], p: Int, s: String) -> Int = {
  let t = list.get(ts, p) ?? "?"
  if t == s or t == "(" then 1 else 2
}

fn guard_eq(ts: List[String], p: Int, s: String) -> Int = {
  let t = list.get(ts, p) ?? ""
  match t {
    "(" => 1,
    x if x == s => 3,
    _ => 2,
  }
}

fn len_at(ts: List[String], p: Int) -> Int = string.len(list.get(ts, p) ?? "none")

fn main() -> Unit = {
  let ts = ["A", "(", ")", "héllo", ""]
  for p in [0, 1, 3, 4, 5, -1] {
    println("${eq_at(ts, p, "(")} ${eq_at(ts, p, "")} ${bound_eq(ts, p, "?")} ${guard_eq(ts, p, "A")} ${len_at(ts, p)}")
  }
  var total = 0
  for i in 0..<100000 {
    total = total + (if eq_at(ts, i % 7, "(") then 1 else 2) + len_at(ts, i % 6)
  }
  println(int.to_string(total))
  println(list.join(ts, "|"))
}
"#;

const SHAPE_STDOUT: &str = "false false 2 3 1\ntrue false 1 1 1\nfalse false 2 2 5\nfalse true 2 2 0\nfalse true 1 2 4\nfalse true 1 2 4\n385714\nA|(|)|héllo|\n";

/// The element leaves the function: it must be an owned value the caller
/// keeps, and the list it came from must be unchanged.
const CONTROL: &str = r#"fn returned(ts: List[String], p: Int) -> String = list.get(ts, p) ?? "<end>"

fn stored(ts: List[String], p: Int) -> List[String] = {
  let t = list.get(ts, p) ?? "<end>"
  [t, t]
}

fn appended(ts: List[String], p: Int) -> String = {
  var t = list.get(ts, p) ?? "<end>"
  t = t + "!"
  t
}

fn main() -> Unit = {
  let ts = ["A", "(", ")", "héllo", ""]
  for p in [0, 1, 3, 4, 5, -1] {
    println("${returned(ts, p)} ${list.join(stored(ts, p), ",")} ${appended(ts, p)}")
  }
  var kept: List[String] = []
  for i in 0..<1000 {
    kept = kept + [returned(ts, i % 6)]
  }
  println("${list.len(kept)} ${list.get(kept, 3) ?? "?"} ${list.get(kept, 5) ?? "?"}")
  println(list.join(ts, "|"))
}
"#;

const CONTROL_STDOUT: &str = "A A,A A!\n( (,( (!\nhéllo héllo,héllo héllo!\n , !\n<end> <end>,<end> <end>!\n<end> <end>,<end> <end>!\n1000 héllo <end>\nA|(|)|héllo|\n";

fn runs_unwalled(name: &str, src: &str, stdout: &str) {
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
fn a_defaulted_element_only_inspected_prints_as_the_previous_release() {
    runs_unwalled("lent_element_fallback", SHAPE, SHAPE_STDOUT);
}

#[test]
fn a_defaulted_element_that_escapes_is_kept_by_its_holder() {
    runs_unwalled("lent_element_fallback_control", CONTROL, CONTROL_STDOUT);
}
