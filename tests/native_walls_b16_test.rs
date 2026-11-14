//! v1 walls, round 16 (family B): shapes the native render refused, now rendered,
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

// A heap branch as a statement match's subject, with effectful arms: the
// subject is bound once, then the arms dispatch on it.
const BRANCH_SUBJECT: &str = r#"
type Ctor = { name: String, nargs: Int }
fn ctor_of(cs: List[Ctor], c: String) -> Ctor? = list.find(cs, (ci) => ci.name == c)
fn go(xs: List[String], cs: List[Ctor]) -> List[String] = {
  var out: List[String] = []
  for x in xs {
    match (if x == "_" then none else ctor_of(cs, x)) {
      some(cj) => list.push(out, cj.name + "!"),
      none => list.push(out, x),
    }
  }
  out
}
effect fn main() -> Unit = {
  let cs = [Ctor { name: "A", nargs: 1 }, Ctor { name: "B", nargs: 0 }]
  println(list.join(go(["A", "_", "q", "B"], cs), ","))
}
"#;

#[test]
fn a_branch_subject_of_a_statement_match_is_bound_before_the_arms() {
    both(BRANCH_SUBJECT, "A!,_,q,B!\n");
}

// A `!` over a callee with a record err, in a loop of a String-err fn: the
// loop err slot stores the err's text, as the propagation does elsewhere.
const LOOP_RECORD_ERR: &str = r#"
type LexError = { at: Int, msg: String }
fn scan(s: String) -> Result[List[String], LexError] =
  if s == "" then err(LexError { at: 0, msg: "empty" }) else ok(string.split(s, " "))
fn count(xs: List[String]) -> Int! = {
  var n = 0
  for text in xs {
    let toks = scan(text)!
    n = n + list.len(toks)
  }
  ok(n)
}
effect fn main() -> Unit = {
  for xs in [["a b", "c"], ["a", "", "b"]] {
    match count(xs) {
      ok(n) => println(int.to_string(n)),
      err(e) => println("E " + e),
    }
  }
}
"#;

#[test]
fn a_record_err_bang_in_a_loop_stores_its_text_in_the_err_slot() {
    both(LOOP_RECORD_ERR, "3\nE LexError { at: 0, msg: \"empty\" }\n");
}

// The same `!` in a statement arm: the return rule builds this fn's err from
// the callee err's text.
const ARM_RECORD_ERR: &str = r#"
type LexError = { at: Int, msg: String }
fn scan(s: String) -> Result[List[String], LexError] =
  if s == "" then err(LexError { at: 0, msg: "empty" }) else ok(string.split(s, " "))
fn pick(k: Int, t: String) -> Int! = {
  var n = 1
  match k {
    0 => {
      let toks = scan(t)!
      n = n + list.len(toks)
    },
    _ => (),
  }
  ok(n * 10)
}
effect fn main() -> Unit = {
  for (k, t) in [(0, "x y"), (0, ""), (1, "")] {
    match pick(k, t) {
      ok(v) => println(int.to_string(v)),
      err(e) => println("E " + e),
    }
  }
}
"#;

#[test]
fn a_record_err_bang_in_an_arm_returns_its_text_as_this_fns_err() {
    both(ARM_RECORD_ERR, "30\nE LexError { at: 0, msg: \"empty\" }\n10\n");
}
