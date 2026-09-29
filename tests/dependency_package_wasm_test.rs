//! A DEPENDENCY package's modules must lower for wasm exactly as the same
//! source does when it is local to the consumer (#904).
//!
//! The v1 pipeline re-runs the frontend from source, so it re-derives what the
//! CLI resolver knows. What it did not re-derive was package identity: a
//! dependency's modules are registered under package-qualified names
//! (`ceangal.render`), and inside such a module `import self.layout` has to
//! resolve against the PACKAGE. Without that scope the import canonicalized to
//! `ceangal.render.layout`, found nothing, fell back to the bare leaf `layout`,
//! and the sibling call's signature was never found — so the call typed
//! `Unknown`, the function walled with "Unknown type reached MIR lowering", and
//! the CONSUMER saw an unlinked `almide_rt_ceangal_render_render_at`.
//!
//! The package's own build never hit this: standalone, its modules are named
//! bare, so the leaf fallback happened to be right. That is exactly the A/B in
//! the report — identical source, local ✅ / dependency ❌.
//!
//! These drive the wasm route (`render_wasm_routed`, `ModuleSource::Provided`)
//! with the tuples the resolver produces (dotted names, `is_self = false`) —
//! no package on disk, no CLI — and run the module. (The fix itself was in the
//! retired incumbent pipeline; the structural leg keys packages its own way,
//! #2865, and these hold it to the same A/B.)

use almide::lexer::Lexer;
use almide::parser::Parser;

fn parse(src: &str) -> almide::ast::Program {
    let mut p = Parser::new(Lexer::tokenize(src));
    let prog = p.parse().expect("fixture parses");
    assert!(p.errors.is_empty(), "fixture parse errors: {:?}", p.errors);
    prog
}

const GEOM: &str = r#"
type Rect = { x: Float, y: Float, w: Float, h: Float }

fn nums(n: Int) -> List[Float] = list.map(list.range(0, n), (i) => int.to_float(i))

fn boxes(n: Int, w: Float, h: Float) -> List[Rect] =
  list.map(list.range(0, n), (i) => Rect { x: int.to_float(i), y: 0.0, w: w, h: h })
"#;

const VIEW: &str = r#"
type View = { kind: String, text: String }

fn text(s: String) -> View = View { kind: "text", text: s }
"#;

/// The shape that walled: a dependency submodule calling a SIBLING submodule of
/// the same package through `import self.<name>`.
const SHAPE: &str = r#"
import self.view as view
import self.geom as geom

fn count(n: Int) -> Int = list.len(geom.nums(n))

fn described(v: view.View, n: Int) -> String =
  view.text(v.kind).text + int.to_string(list.len(geom.boxes(n, 1.0, 2.0)))
"#;

fn dependency_modules() -> Vec<(String, almide::ast::Program, bool)> {
    vec![
        ("depp.view".to_string(), parse(VIEW), false),
        ("depp.geom".to_string(), parse(GEOM), false),
        ("depp.shape".to_string(), parse(SHAPE), false),
    ]
}

/// Build `root` on the wasm leg with the resolver's module tuples (plus the
/// bundled stdlib modules the entry imports) and run it on the embedded
/// host; the program's stdout.
fn render(root: &str, modules: Vec<(String, almide::ast::Program, bool)>) -> Result<String, String> {
    use almide::wasm_route::{render_wasm_routed, ModuleSource, RouteOptions};
    let mut all = almide_mir::pipeline::bundled_self_modules(root);
    all.extend(modules);
    let routed = render_wasm_routed("main.almd", root, ModuleSource::Provided(&all), RouteOptions::default())
        .map_err(|e| format!("{e:?}"))?;
    let run = almide_wasm_run::run_wasm(&routed.bytes).map_err(|e| e.to_string())?;
    if run.exit != 0 {
        return Err(format!("exit {}: {}", run.exit, run.stderr));
    }
    Ok(run.stdout)
}

#[test]
fn a_dependency_submodule_calling_a_sibling_lowers() {
    let root = r#"
import depp.shape as shape

fn main() -> Unit = println(int.to_string(shape.count(3)))
"#;
    let out = render(root, dependency_modules()).expect("dependency call lowers");
    assert_eq!(out, "3\n", "the sibling-calling dependency fn must be defined and run");
}

#[test]
fn a_dependency_submodule_using_a_siblings_type_lowers() {
    // `described` takes a `view.View` and calls into BOTH siblings, so the
    // package scope has to hold for types as well as functions.
    let root = r#"
import depp.view as v
import depp.shape as shape

fn main() -> Unit = println(shape.described(v.text("hi"), 2))
"#;
    let out = render(root, dependency_modules()).expect("cross-sibling types lower");
    assert!(!out.is_empty(), "the program must run and print");
}

/// #943's remaining half: a LIST LITERAL in a sibling-call argument position —
/// aivarium's `v.col([v.text(..), ..])` view-tree shape. The first cause (a
/// top-let VarId colliding with a sibling's parameter, PR #944) was fixed and
/// pinned separately; this pins the argument-materialization half, which walled
/// as "List argument cannot be faithfully materialized in this brick" on the
/// module-sibling path while the same expression inlined into main lowered
/// fine. Nested literals and call-result elements are the load-bearing part.
const TREE: &str = r#"
type Node =
  | Leaf(String)
  | Branch(List[Node])

fn leaf(s: String) -> Node = Leaf(s)
fn branch(kids: List[Node]) -> Node = Branch(kids)

fn render(n: Node) -> String =
  match n {
    Leaf(s) => s,
    Branch(kids) => "[" + (list.map(kids, (k) => render(k)) |> list.join(",")) + "]",
  }
"#;

#[test]
fn a_list_literal_argument_to_a_sibling_call_lowers() {
    let modules = vec![("tree".to_string(), parse(TREE), true)];
    let root = r#"
import self.tree as tree

let unrelated = 1

fn app() -> String =
  tree.render(tree.branch([tree.leaf("a"), tree.branch([tree.leaf("b")]), tree.leaf("c")]))

fn main() -> Unit = println(app())
"#;
    let out = render(root, modules).expect(
        "a list literal (nested, call-result elements) as a sibling-call argument lowers",
    );
    assert!(!out.is_empty(), "the program must run and print");
}

#[test]
fn the_same_source_as_local_modules_still_lowers() {
    // The other half of the report's A/B: leaf-named `self` modules, which
    // always worked and must keep working — the package scope must not be
    // applied to a project's own `src/*.almd`.
    let local = vec![
        ("view".to_string(), parse(VIEW), true),
        ("geom".to_string(), parse(GEOM), true),
        (
            "shape".to_string(),
            parse(
                r#"
import self.view as view
import self.geom as geom

fn count(n: Int) -> Int = list.len(geom.nums(n))
"#,
            ),
            true,
        ),
    ];
    let root = r#"
import self.shape as shape

fn main() -> Unit = println(int.to_string(shape.count(3)))
"#;
    let out = render(root, local).expect("local modules lower");
    assert_eq!(out, "3\n");
}
