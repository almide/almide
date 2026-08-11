/// SharedCellBorrowPass emission pins (#1143): a closure-captured Map read
/// borrows the cell in place when its statement proves safe, keeps the
/// owned `.get()` snapshot when it does not, and a branch-lift helper's
/// snapshot param is never treated as a cell.

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

/// The user-code half of the emission (after the runtime preamble), so the
/// assertions can't accidentally match runtime library text.
fn user_code(full: &str) -> String {
    full.rsplit("//__ALMIDE_RT_BOUNDARY__").next().unwrap().to_string()
}

const COUNTER_LOOP: &str = r#"
effect fn main() -> Unit = {
  var stats: Map[String, Int] = map.new()
  let keys = ["a", "b"]
  let bump = (k: String) => {
    let cur = map.get(stats, k) ?? 0
    map.insert(stats, k, cur + 1)
  }
  for i in 0..<10 {
    bump(keys[i % 2])
  }
  println("a=${map.get(stats, "a") ?? 0}")
}
"#;

#[test]
fn closure_map_read_borrows_the_cell_in_place() {
    let out = user_code(&compile_to_rust(COUNTER_LOOP));
    // The read statement (`let cur = ...`) holds only shared borrows, so it
    // borrows instead of snapshotting the whole Map per call.
    assert!(
        out.contains(".borrow(),") || out.contains(".borrow()"),
        "expected an in-place cell borrow in the closure read:\n{out}"
    );
    // The closure body must not deep-clone the cell per read any more.
    let closure = out.split("Rc::new(move").nth(1).expect("closure body present");
    let closure_body = closure.split("})").next().unwrap();
    assert!(
        !closure_body.contains(".get()"),
        "closure read still snapshots the cell:\n{closure_body}"
    );
}

const MATCH_SUBJECT: &str = r#"
type Acc = { sum: Int, count: Int }

effect fn main() -> Unit = {
  var stats: Map[String, Acc] = map.new()
  let feed = (k: String, t: Int) => {
    match map.get(stats, k) {
      some(s) => map.insert(stats, k, Acc { sum: s.sum + t, count: s.count + 1 }),
      none => map.insert(stats, k, Acc { sum: t, count: 1 }),
    }
  }
  feed("x", 3)
  println("done")
}
"#;

#[test]
fn match_subject_read_is_hoisted_before_the_arms() {
    let out = user_code(&compile_to_rust(MATCH_SUBJECT));
    // The subject read is hoisted to its own bind (guard dies at the `;`),
    // then the arms take their mut borrows without overlap.
    assert!(
        out.contains("__scb_subj"),
        "expected the hoisted match subject bind:\n{out}"
    );
    assert!(
        out.contains(".borrow(),") || out.contains(".borrow()"),
        "expected the hoisted subject to borrow, not snapshot:\n{out}"
    );
}

const FORIN_HEAD_READ: &str = r#"
effect fn main() -> Unit = {
  var m: Map[String, Int] = map.new()
  map.insert(m, "seed", 1)
  let sweep = () => {
    for k in map.keys(m) {
      map.insert(m, k, 0)
    }
  }
  sweep()
  println("n=${map.len(m)}")
}
"#;

#[test]
fn forin_head_read_keeps_the_owned_snapshot() {
    let out = user_code(&compile_to_rust(FORIN_HEAD_READ));
    // A for-loop head's temporaries live for the whole loop, and the body
    // mutates the same cell — the head read must stay on the `.get()`
    // snapshot (a borrow would panic on the body's borrow_mut).
    let closure = out.split("Rc::new(move").nth(1).expect("closure body present");
    let head = closure.split("{").collect::<Vec<_>>().join("{");
    assert!(
        head.contains(".get()"),
        "for-head read must keep the owned snapshot:\n{closure}"
    );
}
