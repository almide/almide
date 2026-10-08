/// LICM must treat a variable that a call in the loop writes through a `mut`
/// parameter as loop-variant, whatever the call's shape — and must keep
/// hoisting what no such call touches. #3452: a bare call to a same-module fn
/// (`bump(s)`) was not looked up, so `s.count` was hoisted above the loop and
/// every iteration read the value from before it.

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

/// The body of the emitted fn `name`, split at its loop: (before, inside).
fn split_at_loop<'a>(code: &'a str, name: &str) -> (&'a str, &'a str) {
    let start = code.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("no fn {name} in:\n{code}"));
    let rest = &code[start..];
    let end = rest[1..].find("\nfn ").or_else(|| rest[1..].find("\npub fn ")).map_or(rest.len(), |i| i + 1);
    let body = &rest[..end];
    let loop_at = body.find("for ").or_else(|| body.find("while ")).unwrap_or_else(|| panic!("no loop in:\n{body}"));
    body.split_at(loop_at)
}

const SRC: &str = r#"
type St = { count: Int }

type Cfg = { scale: Int, base: Int }

fn bump(mut s: St) -> Unit = {
  s.count = s.count + 1
}

fn St.inc(mut self) -> Unit = {
  self.count = self.count + 1
}

fn bump_get(mut s: St) -> Int = {
  s.count = s.count + 1
  s.count
}

fn bare(cfg: Cfg) -> Int = {
  var s = St { count: 0 }
  var acc = 0
  for _k in [1, 2, 3] {
    bump(s)
    acc = acc + s.count + cfg.scale * cfg.base
  }
  acc
}

fn method(cfg: Cfg) -> Int = {
  var s = St { count: 0 }
  var acc = 0
  for _k in [1, 2, 3] {
    s.inc()
    acc = acc + s.count + cfg.scale * cfg.base
  }
  acc
}

fn let_bound(cfg: Cfg) -> Int = {
  var s = St { count: 0 }
  var acc = 0
  for _k in [1, 2, 3] {
    let r = bump_get(s)
    acc = acc + r + s.count + cfg.scale * cfg.base
  }
  acc
}

fn while_cond(cfg: Cfg) -> Int = {
  var s = St { count: 0 }
  var acc = 0
  while bump_get(s) < 4 {
    acc = acc + s.count * 10 + cfg.scale * cfg.base
  }
  acc
}

effect fn main() -> Unit = {
  let cfg = Cfg { scale: 2, base: 5 }
  println(int.to_string(bare(cfg) + method(cfg) + let_bound(cfg) + while_cond(cfg)))
}
"#;

#[test]
fn a_read_a_mut_param_call_writes_stays_in_the_loop() {
    let out = user_code(&compile_to_rust(SRC));
    for name in ["bare", "method", "let_bound", "while_cond"] {
        let (before, inside) = split_at_loop(&out, name);
        assert!(
            !before.contains("s.count"),
            "{name}: `s.count` was hoisted above a loop whose call writes `s`:\n{before}{inside}"
        );
        assert!(inside.contains("s.count"), "{name}: `s.count` must be read in the loop:\n{inside}");
    }
}

#[test]
fn an_invariant_read_no_mut_call_touches_is_still_hoisted() {
    let out = user_code(&compile_to_rust(SRC));
    for name in ["bare", "method", "let_bound", "while_cond"] {
        let (before, inside) = split_at_loop(&out, name);
        assert!(
            before.contains("__almide_ir2_licm_") && before.contains("cfg.scale"),
            "{name}: `cfg.scale * cfg.base` is loop-invariant and must still be hoisted:\n{before}{inside}"
        );
    }
}
