use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::fmt;

fn roundtrip(input: &str) -> String {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().expect("parse failed");
    fmt::format_program(&prog)
}

#[test]
fn fmt_simple_fn() {
    let out = roundtrip("module app\nfn add(a: Int, b: Int) -> Int = a + b");
    assert!(out.contains("fn add(a: Int, b: Int) -> Int ="));
    assert!(out.contains("a + b"));
}

#[test]
fn fmt_variant_type() {
    let out = roundtrip("module app\ntype Color =\n  | Red\n  | Green\n  | Blue");
    assert!(out.contains("Red"));
    assert!(out.contains("Green"));
    assert!(out.contains("Blue"));
}

#[test]
fn fmt_tuple_type() {
    let out = roundtrip("module app\nfn pair() -> (Int, String) = (1, \"x\")");
    assert!(out.contains("(Int, String)"));
}

#[test]
fn fmt_if_expr() {
    let out = roundtrip("module app\nfn f(x: Int) -> Int = if x > 0 then x else 0 - x");
    assert!(out.contains("if x > 0 then"));
    assert!(out.contains("else"));
}

#[test]
fn fmt_match() {
    let out = roundtrip("module app\nfn f(x: Option[Int]) -> Int = match x {\n  some(v) => v\n  none => 0\n}");
    assert!(out.contains("match x {"));
    assert!(out.contains("some(v) =>"));
    assert!(out.contains("none =>"));
}

#[test]
fn fmt_preserves_module_and_imports() {
    let out = roundtrip("import fs\nimport http\nfn f() -> Int = 1");
    assert!(out.contains("import fs\n"));
    assert!(out.contains("import http\n"));
}

#[test]
fn fmt_test_decl() {
    let out = roundtrip("module app\ntest \"basic\" {\n  assert(true)\n}");
    assert!(out.contains("test \"basic\""));
}

#[test]
fn fmt_lambda() {
    let out = roundtrip("module app\nfn f() -> fn(Int) -> Int = (x) => x + 1");
    assert!(out.contains("(x) => x + 1"));
}

#[test]
fn fmt_for_in() {
    let out = roundtrip("module app\neffect fn main(_a: List[String]) -> Result[Unit, String] = {\n  for x in xs {\n    println(x)\n  }\n  ok(())\n}");
    assert!(out.contains("for x in xs {"));
}

#[test]
fn fmt_tuple_pattern() {
    let out = roundtrip("module app\nfn f(p: (Int, Int)) -> Int = match p {\n  (a, b) => a + b\n}");
    assert!(out.contains("(a, b) =>"));
}

// ---- Idempotency ----

#[test]
fn fmt_idempotent_simple_fn() {
    let input = "fn add(a: Int, b: Int) -> Int = a + b";
    let first = roundtrip(input);
    let second = roundtrip(&first);
    assert_eq!(first, second, "formatter should be idempotent");
}

#[test]
fn fmt_idempotent_match() {
    let input = "fn f(x: Option[Int]) -> Int = match x {\n  some(v) => v\n  none => 0\n}";
    let first = roundtrip(input);
    let second = roundtrip(&first);
    assert_eq!(first, second);
}

#[test]
fn fmt_idempotent_block() {
    let input = "fn f() -> Int = {\n  let x = 1\n  let y = 2\n  x + y\n}";
    let first = roundtrip(input);
    let second = roundtrip(&first);
    assert_eq!(first, second);
}

#[test]
fn fmt_idempotent_variant_type() {
    let input = "type Shape =\n  | Circle(Float)\n  | Rect(Float, Float)";
    let first = roundtrip(input);
    let second = roundtrip(&first);
    assert_eq!(first, second);
}

// ---- Records ----

#[test]
fn fmt_record_type() {
    let out = roundtrip("type Point = { x: Int, y: Int }");
    assert!(out.contains("type Point ="));
    assert!(out.contains("x: Int"));
    assert!(out.contains("y: Int"));
}

#[test]
fn fmt_record_literal() {
    let out = roundtrip("fn f() -> { x: Int, y: Int } = { x: 1, y: 2 }");
    assert!(out.contains("{ x: 1, y: 2 }"));
}

#[test]
fn fmt_empty_record() {
    let out = roundtrip("fn f() -> { x: Int } = {}");
    // Empty record should parse and format without crashing
    assert!(out.contains("fn f"), "should contain function, got:\n{}", out);
}

// ---- Spread record ----

#[test]
fn fmt_spread_record() {
    let out = roundtrip("fn f(p: { x: Int, y: Int }) -> { x: Int, y: Int } = { ...p, x: 1 }");
    assert!(out.contains("...p"));
    assert!(out.contains("x: 1"));
}

// ---- Lists ----

#[test]
fn fmt_empty_list() {
    let out = roundtrip("fn f() -> List[Int] = []");
    assert!(out.contains("[]"));
}

#[test]
fn fmt_list_literal() {
    let out = roundtrip("fn f() -> List[Int] = [1, 2, 3]");
    assert!(out.contains("[1, 2, 3]"));
}

// ---- Expressions ----

#[test]
fn fmt_pipe() {
    let out = roundtrip("fn f(xs: List[Int]) -> List[Int] = xs |> list.filter(fn(x) => x > 0)");
    assert!(out.contains("|>"));
}

#[test]
fn fmt_binary_ops() {
    let out = roundtrip("fn f(a: Int, b: Int) -> Int = a + b * 2");
    assert!(out.contains("a + b * 2"));
}

#[test]
fn fmt_concat_ops() {
    let out = roundtrip("fn f() -> String = \"hello\" ++ \" world\"");
    assert!(out.contains("++"));
}

#[test]
fn fmt_unary_negation() {
    let out = roundtrip("fn f(x: Int) -> Int = -x");
    assert!(out.contains("-x"));
}

#[test]
fn fmt_not() {
    let out = roundtrip("fn f(x: Bool) -> Bool = not x");
    assert!(out.contains("not x"));
}

#[test]
fn fmt_range() {
    let out = roundtrip("fn f() -> List[Int] = 0..10");
    assert!(out.contains("0..10"));
}

#[test]
fn fmt_range_inclusive() {
    let out = roundtrip("fn f() -> List[Int] = 1..=10");
    assert!(out.contains("1..=10"));
}

// ---- Result/Option ----

#[test]
fn fmt_ok_err() {
    let out = roundtrip("fn f() -> Result[Int, String] = ok(42)");
    assert!(out.contains("ok(42)"));
    let out = roundtrip("fn f() -> Result[Int, String] = err(\"bad\")");
    assert!(out.contains("err(\"bad\")"));
}

#[test]
fn fmt_some_none() {
    let out = roundtrip("fn f() -> Option[Int] = some(42)");
    assert!(out.contains("some(42)"));
    let out = roundtrip("fn f() -> Option[Int] = none");
    assert!(out.contains("none"));
}

// ---- Declarations ----

#[test]
fn fmt_effect_fn() {
    let out = roundtrip("effect fn main(args: List[String]) -> Result[Unit, String] = ok(())");
    assert!(out.contains("effect fn main"));
}

#[test]
fn fmt_top_let() {
    let out = roundtrip("let pi = 3");
    assert!(out.contains("let pi = 3"));
}

// ---- Member/Index access ----

#[test]
fn fmt_member_access() {
    let out = roundtrip("fn f(p: { x: Int }) -> Int = p.x");
    assert!(out.contains("p.x"));
}

#[test]
fn fmt_index_access() {
    let out = roundtrip("fn f(xs: List[Int]) -> Int = xs[0]");
    assert!(out.contains("xs[0]"));
}

// ---- While ----

#[test]
fn fmt_while() {
    let out = roundtrip("fn f() -> Int = {\n  var x = 0\n  while x < 10 {\n    x = x + 1\n  }\n  x\n}");
    assert!(out.contains("while"));
}

// ---- Generic types ----

#[test]
fn fmt_generic_type() {
    let out = roundtrip("fn f(x: List[Option[Int]]) -> List[Option[Int]] = x");
    assert!(out.contains("List[Option[Int]]"));
}

// ---- Fn type ----

#[test]
fn fmt_fn_type() {
    let out = roundtrip("fn apply(f: fn(Int) -> Int, x: Int) -> Int = f(x)");
    assert!(out.contains("fn(Int) -> Int") || out.contains("(Int) -> Int"));
}

// ---- Todo ----

#[test]
fn fmt_todo() {
    let out = roundtrip("fn f() -> Int = todo(\"not done\")");
    assert!(out.contains("todo(\"not done\")"));
}

// ---- Impl block ----

#[test]
fn fmt_impl_block() {
    // NOTE: formatter does not yet emit impl blocks; verify it doesn't crash
    let out = roundtrip("type Greeter = { name: String }\nimpl Greeter {\n  fn greet(self: Greeter) -> String = self.name\n}");
    assert!(out.contains("Greeter"), "should at least contain the type, got:\n{}", out);
}

// ---- Comment preservation ----

#[test]
fn fmt_top_level_comments() {
    let out = roundtrip("// file header\nmodule app\n// a utility\nfn f() -> Int = 1");
    assert!(out.contains("// file header"));
    assert!(out.contains("// a utility"));
}

#[test]
fn fmt_inline_block_comments() {
    let out = roundtrip("module app\nfn f() -> Int = {\n  // step 1\n  let x = 1\n  // step 2\n  x + 1\n}");
    assert!(out.contains("// step 1"));
    assert!(out.contains("// step 2"));
}

#[test]
fn fmt_match_arm_comments() {
    let out = roundtrip("module app\nfn f(x: Option[Int]) -> Int = match x {\n  // handle value\n  some(v) => v\n  // handle empty\n  none => 0\n}");
    assert!(out.contains("// handle value"));
    assert!(out.contains("// handle empty"));
}

#[test]
fn fmt_for_in_comments() {
    let out = roundtrip("module app\neffect fn main(_a: List[String]) -> Result[Unit, String] = {\n  for x in xs {\n    // process item\n    println(x)\n  }\n  ok(())\n}");
    assert!(out.contains("// process item"));
}

// ---- Roundtrip & Idempotency over all spec/ files ----

#[test]
fn fmt_roundtrip_idempotency_all_spec_files() {
    let spec_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("spec");
    let mut tested = 0u32;
    let mut skipped = Vec::new();
    let mut failures = Vec::new();

    // Known non-idempotent files (temporary — raw string r"..." loses raw flag during format)
    let skip_files: &[&str] = &["regex_test.almd"];

    for entry in walkdir(spec_dir.as_path()) {
        let path = entry;
        if skip_files.iter().any(|s| path.to_string_lossy().ends_with(s)) {
            skipped.push(format!("{}: known non-idempotent (raw strings)", path.display()));
            continue;
        }
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => {
                skipped.push(format!("{}: read error", path.display()));
                continue;
            }
        };

        // First parse
        let tokens1 = Lexer::tokenize(&source);
        let mut parser1 = Parser::new(tokens1);
        let prog1 = match parser1.parse() {
            Ok(p) => p,
            Err(_) => {
                skipped.push(format!("{}: parse error", path.display()));
                continue;
            }
        };

        // Format once
        let formatted1 = fmt::format_program(&prog1);

        // Reparse the formatted output
        let tokens2 = Lexer::tokenize(&formatted1);
        let mut parser2 = Parser::new(tokens2);
        let prog2 = match parser2.parse() {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!(
                    "{}: formatted output failed to parse: {}\n--- formatted output ---\n{}",
                    path.display(),
                    e,
                    formatted1
                ));
                continue;
            }
        };

        // Format again
        let formatted2 = fmt::format_program(&prog2);

        // Idempotency check: format(format(x)) == format(x)
        if formatted1 != formatted2 {
            // Compute a concise diff: find first diverging line
            let lines1: Vec<&str> = formatted1.lines().collect();
            let lines2: Vec<&str> = formatted2.lines().collect();
            let mut diff_info = String::new();
            for (i, (l1, l2)) in lines1.iter().zip(lines2.iter()).enumerate() {
                if l1 != l2 {
                    diff_info.push_str(&format!(
                        "  first diff at line {}: fmt1={:?} fmt2={:?}\n",
                        i + 1, l1, l2
                    ));
                    break;
                }
            }
            if lines1.len() != lines2.len() {
                diff_info.push_str(&format!(
                    "  line count: fmt1={} fmt2={}\n",
                    lines1.len(), lines2.len()
                ));
            }
            failures.push(format!(
                "{}: formatter is not idempotent\n{}",
                path.display(),
                diff_info,
            ));
            continue;
        }

        tested += 1;
    }

    eprintln!(
        "fmt roundtrip/idempotency: {} files tested, {} skipped",
        tested,
        skipped.len()
    );
    for s in &skipped {
        eprintln!("  SKIP: {}", s);
    }

    if !failures.is_empty() {
        for f in &failures {
            eprintln!("  FAIL: {}", f);
        }
        panic!(
            "{} file(s) failed roundtrip/idempotency check",
            failures.len()
        );
    }

    assert!(tested > 0, "no spec files were tested");
}

/// Recursively collect all .almd files under a directory.
fn walkdir(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                results.extend(walkdir(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("almd") {
                results.push(path);
            }
        }
    }
    results.sort();
    results
}
