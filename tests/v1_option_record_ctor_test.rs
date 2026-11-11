//! `some(<constructor>)` where the constructor is spelled as a record literal
//! (`some(Named { kind: k, name: n })`). A variant case is constructed either
//! by a positional call (`Label(t)`) or by a record literal, and the Option
//! wrappers asked only for the call: the record spelling in a function tail,
//! in an `if` or `match` arm, or bound by a `let` was refused ("heap-result
//! `if` outside the executable subset", "heap-result OptionSome", "OptionSome
//! heap bind") where the positional one of the same type was built.
use std::process::Command;

const HEAD: &str = r#"type Shape =
  | Label(String)
  | Named { kind: String, name: String }
  | Dot { x: Int, y: Int }

fn show(s: Shape) -> String = match s {
  Label(t) => "label:" + t,
  Named { kind, name } => kind + "/" + name,
  Dot { x, y } => "dot:${x},${y}",
}

fn shown(o: Option[Shape]) -> String = match o {
  some(s) => show(s),
  none => "none",
}
"#;

const BODY: &str = r#"
fn tail_heap(k: String) -> Option[Shape] = some(Named { kind: k, name: "t" })

fn tail_scalar(n: Int) -> Option[Shape] = some(Dot { x: n, y: n + 1 })

fn if_arm(k: String) -> Option[Shape] =
  if k == "named" then some(Named { kind: k, name: "i" }) else none

fn match_arm(k: String) -> Option[Shape] = match k {
  "dot" => some(Dot { x: 1, y: string.len(k) }),
  "label" => some(Label(k)),
  "none" => none,
  _ => some(Named { kind: k, name: "m" }),
}

fn bound(k: String) -> Option[Shape] = {
  let o = some(Named { kind: k, name: "b" })
  o
}

effect fn main() -> Unit = {
  println(shown(tail_heap("a")))
  println(shown(tail_scalar(4)))
  println(shown(if_arm("named")))
  println(shown(if_arm("other")))
  for k in ["dot", "label", "none", "zed"] {
    println(shown(match_arm(k)))
  }
  println(shown(bound("c")))
}
"#;

const STDOUT: &str = "a/t\ndot:4,5\nnamed/i\nnone\ndot:1,3\nlabel:label\nnone\nzed/m\nc/b\n";

/// The positional spelling in the same positions: the route the record
/// spelling now shares, held to the same answers.
const POSITIONAL: &str = r#"
fn tail_heap(k: String) -> Option[Shape] = some(Label(k))

fn if_arm(k: String) -> Option[Shape] =
  if k == "named" then some(Label(k + "!")) else none

fn bound(k: String) -> Option[Shape] = {
  let o = some(Label(k))
  o
}

effect fn main() -> Unit = {
  println(shown(tail_heap("a")))
  println(shown(if_arm("named")))
  println(shown(if_arm("other")))
  println(shown(bound("c")))
}
"#;

const POSITIONAL_STDOUT: &str = "label:a\nlabel:named!\nnone\nlabel:c\n";

fn render(src: String) -> Result<String, String> {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| format!("{e:?}")))
        .expect("spawn")
        .join()
        .expect("render thread")
}

fn runs_unwalled(name: &str, src: &str, stdout: &str) {
    render(src.to_string()).unwrap_or_else(|e| panic!("{name} walls: {e}"));
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
fn an_option_of_a_record_spelled_constructor_is_built_in_every_position() {
    runs_unwalled("option_record_ctor", &format!("{HEAD}{BODY}"), STDOUT);
}

#[test]
fn the_positional_spelling_keeps_its_answers() {
    runs_unwalled("option_positional_ctor", &format!("{HEAD}{POSITIONAL}"), POSITIONAL_STDOUT);
}
