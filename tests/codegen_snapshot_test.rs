/// Codegen snapshot tests: verify generated Rust output doesn't regress.
/// Uses `insta` for snapshot management. Run `cargo insta review` to update.

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

/// Extract just the user code (after the runtime preamble).
fn user_code(full: &str) -> &str {
    // The runtime preamble ends before the first `pub fn` or `fn` declaration
    // that isn't part of a trait/impl block.
    // Simple heuristic: find the last `macro_rules!` line and take everything after.
    if let Some(pos) = full.rfind("macro_rules!") {
        let after_macro = &full[pos..];
        if let Some(newline) = after_macro.find('\n') {
            let rest = &full[pos + newline + 1..];
            // Skip blank lines and runtime code until we hit user code.
            // User functions start with `pub fn` at column 0. Earlier
            // versions relied on `\npub fn ` matching (leading newline),
            // which silently dropped the first user fn when the preamble
            // shrank enough that `rest` starts directly with `pub fn ...`
            // — notably after Stage 4c migrated the fs / process stdlib
            // types out of the unconditional preamble. Accept both
            // forms: "at start of rest" AND "after a newline".
            let hit = if rest.starts_with("pub fn ") || rest.starts_with("fn ") {
                Some(0)
            } else {
                rest.find("\npub fn ").or_else(|| rest.find("\nfn ")).map(|p| p + 1)
            };
            if let Some(fn_pos) = hit {
                return rest[fn_pos..].trim();
            }
            return rest.trim();
        }
    }
    full.trim()
}

macro_rules! assert_rust {
    ($name:expr, $src:expr) => {{
        let full = compile_to_rust($src);
        let code = user_code(&full);
        insta::assert_snapshot!($name, code);
    }};
}

// ── Basic functions ─────────────────────────────────────────────

#[test]
fn snapshot_simple_add() {
    assert_rust!("simple_add", r#"
fn add(a: Int, b: Int) -> Int = a + b
    "#);
}

#[test]
fn snapshot_string_concat() {
    assert_rust!("string_concat", r#"
fn greet(name: String) -> String = "Hello, " + name
    "#);
}

#[test]
fn snapshot_if_else() {
    assert_rust!("if_else", r#"
fn abs(x: Int) -> Int = if x < 0 then -x else x
    "#);
}

// ── Pattern matching ────────────────────────────────────────────

#[test]
fn snapshot_match_option() {
    assert_rust!("match_option", r#"
fn unwrap_or(x: Option[Int], default: Int) -> Int =
  match x {
    some(v) => v,
    none => default,
  }
    "#);
}

#[test]
fn snapshot_match_variant() {
    assert_rust!("match_variant", r#"
type Color = | Red | Green | Blue

fn to_string(c: Color) -> String =
  match c {
    Red => "red",
    Green => "green",
    Blue => "blue",
  }
    "#);
}

// ── Effect functions ────────────────────────────────────────────

#[test]
fn snapshot_effect_fn() {
    assert_rust!("effect_fn", r#"
effect fn parse_int(s: String) -> Int = int.parse(s)!
    "#);
}

// ── List operations ─────────────────────────────────────────────

#[test]
fn snapshot_list_map() {
    assert_rust!("list_map", r#"
fn doubles(xs: List[Int]) -> List[Int] =
  xs |> list.map((x) => x * 2)
    "#);
}

#[test]
fn snapshot_list_filter() {
    assert_rust!("list_filter", r#"
fn positives(xs: List[Int]) -> List[Int] =
  xs |> list.filter((x) => x > 0)
    "#);
}

// ── Records ─────────────────────────────────────────────────────

#[test]
fn snapshot_record() {
    assert_rust!("record", r#"
type Point = { x: Int, y: Int }

fn origin() -> Point = { x: 0, y: 0 }

fn translate(p: Point, dx: Int, dy: Int) -> Point =
  { x: p.x + dx, y: p.y + dy }
    "#);
}

// ── Lambda / closures ───────────────────────────────────────────

#[test]
fn snapshot_lambda() {
    assert_rust!("lambda", r#"
fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)

fn main() -> Int = apply((x) => x + 1, 41)
    "#);
}

// ── String interpolation ────────────────────────────────────────

#[test]
fn snapshot_string_interp() {
    assert_rust!("string_interp", r#"
fn describe(name: String, age: Int) -> String =
  "${name} is ${int.to_string(age)} years old"
    "#);
}

// ── Pipe chains ─────────────────────────────────────────────────

#[test]
fn snapshot_pipe_chain() {
    assert_rust!("pipe_chain", r#"
fn process(xs: List[Int]) -> Int =
  xs
    |> list.filter((x) => x > 0)
    |> list.map((x) => x * 2)
    |> list.fold(0, (acc, x) => acc + x)
    "#);
}

// ── Test blocks ─────────────────────────────────────────────────

#[test]
fn snapshot_test_block() {
    assert_rust!("test_block", r#"
fn add(a: Int, b: Int) -> Int = a + b

test "addition" {
  assert_eq(add(1, 2), 3)
}
    "#);
}
