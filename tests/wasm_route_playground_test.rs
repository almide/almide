//! #2554: the playground compiled through the incumbent renderer alone and
//! so walled programs `almide run --target wasm` runs. The library entry
//! `almide::wasm_route::render_wasm_routed` is the CLI's routing as a call a
//! wasm32 consumer can make — with the modules read off disk (the CLI's form)
//! or handed over pre-parsed (the browser's tabs). The program is the issue's
//! tile example: `grid`'s tail `if` with a `guard let` was outside the retired
//! incumbent's executable subset. Since #2752 the route has one leg, and a
//! refusal comes back as a named `RouteError`, never as another renderer's
//! module.

use almide::wasm_route::{render_wasm_routed, ModuleSource, RouteError, RouteOptions};

const MAIN: &str = r#"import self.tile
import self.view

let N = 14

effect fn row(x: Int, y: Int, acc: String) -> String! =
  if x >= N then acc
  else {
    let t = tile.at(x * 2, y * 2)!
    row(x + 1, y, acc + t)!
  }

effect fn grid(y: Int, acc: String) -> String =
  if y >= N then acc
  else {
    guard let r = row(0, y, "") else { err("test") }
    let ret = grid(y + 1, acc + r)!
    ret
  }

effect fn main() -> Unit =
  println(view.doc(N * 2, grid(0, "")!))
"#;

const TILE: &str = r#"pub effect fn at(x: Int, y: Int) -> String! =
  if x < 0 then err("negative") else "${x},${y};"
"#;

const VIEW: &str = r#"pub fn doc(w: Int, body: String) -> String = "<svg w=${w}>${body}</svg>"
"#;

/// What the program prints (native and wasm agree — the issue's table).
fn expected_stdout() -> String {
    let mut body = String::new();
    for y in 0..14 {
        for x in 0..14 {
            body.push_str(&format!("{},{};", x * 2, y * 2));
        }
    }
    format!("<svg w=28>{body}</svg>\n")
}

/// The playground's `run` form: `main` required, every host op served.
fn run_form() -> RouteOptions {
    RouteOptions { library: false, ..RouteOptions::default() }
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"tile\"\nversion = \"0.1.0\"\n").expect("toml");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(src.join("main.almd"), MAIN).expect("main");
    std::fs::write(src.join("tile.almd"), TILE).expect("tile");
    std::fs::write(src.join("view.almd"), VIEW).expect("view");
    dir
}

fn parse_tab(source: &str, label: &str) -> almide::ast::Program {
    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens).with_file(label);
    let program = parser.parse().expect("tab parses");
    assert!(parser.errors.is_empty(), "{label}: parse errors");
    program
}

fn run_on_embedded_host(bytes: &[u8]) -> String {
    let result = almide_wasm_run::run_wasm(bytes).expect("embedded host runs the module");
    assert_eq!(result.exit, 0, "exit code; stderr: {}", result.stderr);
    result.stdout
}

#[test]
fn tile_program_routes_to_the_structural_leg_from_disk() {
    let dir = project();
    let main = dir.path().join("src/main.almd");
    let routed = render_wasm_routed(main.to_str().expect("utf8"), MAIN, ModuleSource::Disk { dep_paths: &[] }, run_form())
        .unwrap_or_else(|e| panic!("the route refused the tile program: {e:?}"));
    assert_eq!(run_on_embedded_host(&routed.bytes), expected_stdout());
    // The stock-WASI form (what a browser p1 runner loads) is a valid module.
    let wasi = routed.stock_wasi().expect("to_wasi");
    wasmparser::validate(&wasi).expect("the WASI form validates");
}

#[test]
fn tile_program_routes_to_the_structural_leg_from_provided_modules() {
    // The browser form: no filesystem, the tabs parsed by the caller, the
    // bundled stdlib modules resolved for the entry — the list the incumbent
    // renderer took from the playground before #2554.
    let mut modules = almide_mir::pipeline::bundled_self_modules(MAIN);
    modules.push(("tile".to_string(), parse_tab(TILE, "tile.almd"), true));
    modules.push(("view".to_string(), parse_tab(VIEW, "view.almd"), true));
    let routed = render_wasm_routed("main.almd", MAIN, ModuleSource::Provided(&modules), run_form())
        .unwrap_or_else(|e| panic!("the route refused the tile program: {e:?}"));
    assert_eq!(run_on_embedded_host(&routed.bytes), expected_stdout());
}

/// A shape the leg declines is `RouteError::Wall` with the leg's reason —
/// the whole verdict, with no second renderer behind it (#2752).
#[test]
fn a_declined_program_is_a_named_wall() {
    let src = "type Inner = { xs: List[Int] }\ntype Outer = { inner: Inner }\n\neffect fn main() -> Unit = {\n  var o = Outer { inner: Inner { xs: [1] } }\n  list.push(o.inner.xs, 2)\n  println(\"${list.len(o.inner.xs)}\")\n}\n";
    let modules = almide_mir::pipeline::bundled_self_modules(src);
    match render_wasm_routed("main.almd", src, ModuleSource::Provided(&modules), run_form()) {
        Err(RouteError::Wall { why }) => assert!(why.contains("list-push-nonvar"), "the wall names the leg's reason: {why}"),
        other => panic!("expected the leg's wall, got {other:?}"),
    }
}

/// The run form on a main-less program is refused by the route itself.
#[test]
fn the_run_form_refuses_a_program_without_main() {
    let src = "fn add(a: Int, b: Int) -> Int = a + b\n";
    let modules = almide_mir::pipeline::bundled_self_modules(src);
    match render_wasm_routed("lib.almd", src, ModuleSource::Provided(&modules), run_form()) {
        Err(RouteError::NoMain) => {}
        other => panic!("expected NoMain, got {other:?}"),
    }
    // The library form builds it (its `pub fn`s become exports).
    let lib = RouteOptions { library: true, ..RouteOptions::default() };
    render_wasm_routed("lib.almd", src, ModuleSource::Provided(&modules), lib).expect("the library form builds a main-less module");
}
