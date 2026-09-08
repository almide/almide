//! #2028: a param the callee only READS is borrowed on the structural wasm
//! leg (param_borrow.rs) — the call site shares nothing, the exit plan
//! releases nothing. The regression this pins: `check(t: Tree)` walking a
//! 2^20-node tree paid an inc/dec pair per node (+51 % wall clock) once
//! variants became droppable, and neither runtime gate saw it because the
//! output was right and the ratchet corpus has no sequential-allocation
//! row. Placement is the observable: the emitted `check` carries no RC
//! call at all, and `make` only the constructor's own stores.

fn render(source: &str) -> String {
    let modules = almide_mir::pipeline::bundled_self_modules(source);
    almide_mir::pipeline::try_render_wasm_source(source, &modules, false)
        .unwrap_or_else(|e| panic!("the structural leg walled the fixture: {e}"))
}

/// The WAT text of one named function.
fn function_body<'a>(wat: &'a str, name: &str) -> &'a str {
    let head = format!("(func ${name} ");
    let alt = format!("(func ${name}\n");
    let start = wat
        .find(&head)
        .or_else(|| wat.find(&alt))
        .unwrap_or_else(|| panic!("no function ${name} in the module"));
    let rest = &wat[start + 5..];
    let end = rest
        .find("\n (func ")
        .or_else(|| rest.find("\n  (func "))
        .unwrap_or(rest.len());
    &wat[start..start + 5 + end]
}

fn rc_lines(body: &str) -> Vec<&str> {
    body.lines()
        .map(str::trim)
        .filter(|l| l.contains("$rc_inc") || l.contains("$rc_dec") || l.contains("__drop"))
        .collect()
}

const TREE: &str = r#"
type Tree = Leaf | Node(Tree, Tree)
fn make(depth: Int) -> Tree =
  if depth == 0 then Leaf else Node(make(depth - 1), make(depth - 1))
fn check(t: Tree) -> Int = match t { Leaf => 1, Node(l, r) => check(l) + check(r) + 1 }
fn main() -> Unit = {
  var total = 0
  var i = 0
  while i < 12 { total = total + check(make(19)); i = i + 1 }
  println("${total}")
}
"#;

#[test]
fn a_read_only_variant_param_is_borrowed() {
    let wat = render(TREE);
    let check = function_body(&wat, "check");
    let lines = rc_lines(check);
    assert!(
        lines.is_empty(),
        "check(t) only reads t and passes its payloads to itself: no RC site expected, got:\n{}",
        lines.join("\n")
    );
}

/// The other half of the convention: a param the body CONSUMES stays
/// owned — the site shares, the exit plan releases — so a caller that
/// keeps its argument is never left with a freed block.
const CONSUMER: &str = r#"
type Tree = Leaf | Node(Tree, Tree)
fn keep(t: Tree) -> List[Tree] = [t]
fn main() -> Unit = {
  let t = Node(Leaf, Leaf)
  let xs = keep(t)
  let ys = keep(t)
  println("${list.len(xs) + list.len(ys)}")
}
"#;

#[test]
fn a_stored_param_keeps_the_owned_convention() {
    let wat = render(CONSUMER);
    let keep = function_body(&wat, "keep");
    let lines = rc_lines(keep);
    assert!(
        lines.iter().any(|l| l.contains("$rc_inc")),
        "keep(t) stores t into a list: the slot takes its share, got:\n{}",
        lines.join("\n")
    );
}
