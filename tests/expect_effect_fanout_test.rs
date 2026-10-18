//! The expected-effect search (`holds_effect_fn`) runs on every call
//! argument's slot type. It followed every named type it met to a depth of
//! eight without remembering one it had already searched, so a variant whose
//! cases name the same types several times multiplied per level: a parser's
//! tree type with a dozen self-references took the check from milliseconds
//! to hours. A named type is now searched once per query (unless reached
//! again shallower), so the cost is the size of the type graph.
use almide::canonicalize;
use almide::check::Checker;
use almide::diagnostic::Level;
use almide::lexer::Lexer;
use almide::parser::Parser;
use std::sync::mpsc;
use std::time::Duration;

/// Check `src` on a worker thread; the errors, or `None` past `limit`.
fn errors_within(src: &'static str, limit: Duration) -> Option<Vec<String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut prog = Parser::new(Lexer::tokenize(src)).parse().expect("parse failed");
        let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
        let mut checker = Checker::from_env(canon.env);
        checker.diagnostics = canon.diagnostics;
        let errs: Vec<String> = checker
            .infer_program(&mut prog)
            .into_iter()
            .filter(|d| d.level == Level::Error)
            .map(|d| d.message)
            .collect();
        let _ = tx.send(errs);
    });
    rx.recv_timeout(limit).ok()
}

// Twelve self-references across the cases: 12^8 expansions for the old
// search on every call that passes a `Node`.
const FANOUT: &str = r#"
type Node =
  | A(Node, Node, Node, Node)
  | B(Node, Node, Node, Node)
  | C(List[Node], Node, Option[Node], Node)
  | Leaf(Int)

fn weight(n: Node) -> Int = match n {
  Leaf(v) => v,
  A(a, _, _, _) => weight(a) + 1,
  B(b, _, _, _) => weight(b) + 2,
  C(_, c, _, _) => weight(c) + 3,
}

effect fn main() -> Unit = {
  let n = A(Leaf(1), Leaf(2), Leaf(3), Leaf(4))
  println(int.to_string(weight(n) + weight(B(n, n, n, n))))
}
"#;

#[test]
fn a_self_referencing_variant_checks_in_linear_time() {
    let errs = errors_within(FANOUT, Duration::from_secs(60)).expect("the check did not finish in 60 s: the expected-effect search is exponential in the type's fan-out again");
    assert!(errs.is_empty(), "expected no errors, got: {errs:?}");
}

// The search still finds an effect fn behind repeated names: `Handlers`
// is reached through two cases before the effect field is.
const DEEP_EFFECT: &str = r#"
type Handlers = { on: effect (String) -> Int }

type Route =
  | Plain(Route, Route)
  | Hooked(Route, Handlers)
  | End

fn count(r: Route) -> Int = match r {
  Plain(a, b) => count(a) + count(b),
  Hooked(a, _) => count(a) + 1,
  End => 0,
}

effect fn main() -> Unit = {
  let h = Handlers { on: (s) => string.len(s) }
  let r = Hooked(Plain(End, End), h)
  println(int.to_string(count(r)))
  let n = (h.on)("abc")!
  println(int.to_string(n))
}
"#;

#[test]
fn an_effect_fn_behind_repeated_names_is_still_found() {
    let errs = errors_within(DEEP_EFFECT, Duration::from_secs(60)).expect("the check did not finish in 60 s");
    assert!(errs.is_empty(), "expected no errors, got: {errs:?}");
}
