/// LICM must never hoist a call to an `@extern` fn out of a loop. The extern's
/// body is the `_` hole, so the purity fixpoint found nothing impure in it and
/// treated `gpu.push_u32(0)` — a host append to a staging buffer — as a pure
/// loop invariant: three per-element calls became three calls in total, and
/// every uploaded record after the first shifted (snaidhm's native renderer).

use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::canonicalize;
use almide::check::Checker;
use almide::lower::lower_program;
use almide::codegen::{self, pass::Target, CodegenOutput};

fn compile_to_rust(src: &str) -> String {
    let tokens = Lexer::tokenize(src);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    let diags = checker.infer_program(&mut prog);
    let errors: Vec<_> = diags.iter().filter(|d| d.level == almide::diagnostic::Level::Error).collect();
    assert!(errors.is_empty(), "Type errors: {:?}", errors.iter().map(|e| &e.message).collect::<Vec<_>>());
    let mut ir = lower_program(&prog, &checker.env, &checker.type_map);
    almide::optimize::optimize_program(&mut ir);
    almide::mono::monomorphize(&mut ir);
    match codegen::codegen(&mut ir, Target::Rust) {
        CodegenOutput::Source(s) => s,
        CodegenOutput::Binary(_) => unreachable!(),
    }
}

/// The user-code half of the emission (after the runtime preamble).
fn user_code(full: &str) -> String {
    full.rsplit("//__ALMIDE_RT_BOUNDARY__").next().unwrap().to_string()
}

/// The body of the emitted fn `name`, up to the next top-level fn.
fn fn_body<'a>(code: &'a str, name: &str) -> &'a str {
    let start = code.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("no fn {name} in:\n{code}"));
    let rest = &code[start..];
    let end = rest[1..].find("\nfn ").or_else(|| rest[1..].find("\npub fn ")).map_or(rest.len(), |i| i + 1);
    &rest[..end]
}

const SRC: &str = r#"
@extern(rust, "crate::host", "push")
fn push(value: Int) -> Unit = _

fn weight(x: Int) -> Int = x * 3 + 1

fn upload(items: List[Int]) -> Int = {
  var total = 0
  for item in items {
    push(item)
    push(0)
    total = total + item + weight(7)
  }
  total
}

effect fn main() -> Unit = {
  println(int.to_string(upload([1, 2, 3])))
}
"#;

#[test]
fn extern_call_with_invariant_args_stays_in_the_loop() {
    let out = user_code(&compile_to_rust(SRC));
    let body = fn_body(&out, "upload");
    let loop_at = body.find("for ").unwrap_or_else(|| panic!("no loop in:\n{body}"));
    let (before, inside) = body.split_at(loop_at);
    assert!(
        !before.contains("push(0"),
        "`push(0)` was hoisted out of the loop — an extern is not a pure invariant:\n{body}"
    );
    assert!(inside.contains("push(0"), "`push(0)` must run once per element:\n{body}");
}

#[test]
fn pure_call_with_invariant_args_is_still_hoisted() {
    // The control: the same shape against a pure fn is still an invariant, so
    // the test above fails for the extern's sake and not because LICM is off.
    let out = user_code(&compile_to_rust(SRC));
    let body = fn_body(&out, "upload");
    let loop_at = body.find("for ").unwrap_or_else(|| panic!("no loop in:\n{body}"));
    assert!(
        body[..loop_at].contains("__licm_") && body[..loop_at].contains("weight(7"),
        "`weight(7)` is pure and loop-invariant, and should be hoisted:\n{body}"
    );
}
