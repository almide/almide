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
    let out = roundtrip("fn f(xs: List[Int]) -> List[Int] = xs |> list.filter((x) => x > 0)");
    assert!(out.contains("|>"));
}

#[test]
fn fmt_binary_ops() {
    let out = roundtrip("fn f(a: Int, b: Int) -> Int = a + b * 2");
    assert!(out.contains("a + b * 2"));
}

#[test]
fn fmt_concat_ops() {
    let out = roundtrip("fn f() -> String = \"hello\" + \" world\"");
    assert!(out.contains("+") && !out.contains("++"));
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
    let out = roundtrip("fn f() -> List[Int] = 0..<10");
    assert!(out.contains("0..<10"));
}

#[test]
fn fmt_range_inclusive() {
    let out = roundtrip("fn f() -> List[Int] = 1...10");
    assert!(out.contains("1...10"));
}

// #966: the formatter is the migration path for the retired spellings — an
// old-syntax file round-trips to the new spellings.
#[test]
fn fmt_migrates_retired_range_spellings() {
    let out = roundtrip("fn f() -> List[Int] = 0..10");
    assert!(out.contains("0..<10"), "got: {out}");
    let out = roundtrip("fn f() -> List[Int] = 1..=10");
    assert!(out.contains("1...10"), "got: {out}");
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
    // ADR-0010 D3: Option normalizes to the `?` shorthand even as a generic
    // argument; the enclosing generic keeps its bracket shape.
    let out = roundtrip("fn f(x: List[Option[Int]]) -> List[Option[Int]] = x");
    assert!(out.contains("List[Int?]"), "{out}");
    let out = roundtrip("fn f(x: Map[String, List[Int]]) -> Map[String, List[Int]] = x");
    assert!(out.contains("Map[String, List[Int]]"), "{out}");
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

// A comment inside a record body is the one artifact the compiler cannot
// reconstruct — the formatter used to collapse the record to one line and drop
// it silently, so a documented field lost its unit or invariant on every save
// (#1090).
#[test]
fn fmt_record_type_field_comments() {
    let out = roundtrip(
        "module app\ntype Tracer = {\n  service: String,\n  // ns at monotonic zero\n  anchor: Int,\n}",
    );
    assert!(out.contains("// ns at monotonic zero"), "field comment dropped: {out}");
    assert!(out.contains("service: String"));
    assert!(out.contains("anchor: Int"));
    // Idempotent: the multi-line shape the printer emits must reparse to itself.
    assert_eq!(roundtrip(&out), out);
}

#[test]
fn fmt_record_type_comments_survive_defaults_and_aliases() {
    let out = roundtrip(
        "module app\ntype W = {\n  // the wire spells it camelCase\n  trace_id as \"traceId\": String,\n  // absent means unset\n  parent: Option[String] = none,\n}",
    );
    assert!(out.contains("// the wire spells it camelCase"), "{out}");
    assert!(out.contains("// absent means unset"), "{out}");
    assert!(out.contains("as \"traceId\""), "alias dropped: {out}");
    assert!(out.contains("= none"), "default dropped: {out}");
    assert_eq!(roundtrip(&out), out);
}

// A record with no field comments keeps the single-line shape, so adding
// comment support does not churn every existing source file.
#[test]
fn fmt_record_type_without_comments_stays_single_line() {
    let out = roundtrip("module app\ntype Point = {\n  x: Int,\n  y: Int,\n}");
    assert!(out.contains("type Point = { x: Int, y: Int }"), "{out}");
}

// ---- #1129: comment ATTACHMENT (not just idempotence) ----

/// The fmt roundtrip gate is blind to this class: a shifted output is itself
/// a fixpoint. A leading declaration comment must stay on ITS declaration
/// even when the file's import list changes during formatting (an unused
/// import removal used to leave a stale `comment_map` slot, so every later
/// decl read its predecessor's comments — silent doc corruption, #1090's
/// "the compiler cannot reconstruct a comment" principle).
#[test]
fn fmt_keeps_leading_comments_on_their_declaration() {
    let src = "// header line one\n\
               // header line two\n\
               import testing\n\
               \n\
               fn alpha() -> Int = 1\n\
               \n\
               // label A: belongs to beta\n\
               fn beta(c: Bool) -> Int = 2\n\
               \n\
               // label B: belongs to gamma\n\
               fn gamma() -> Int = 3\n";
    let out = roundtrip(src);
    let lines: Vec<&str> = out.lines().collect();
    for (comment, decl) in [
        ("// label A: belongs to beta", "fn beta"),
        ("// label B: belongs to gamma", "fn gamma"),
    ] {
        let ci = lines.iter().position(|l| l.trim() == comment)
            .unwrap_or_else(|| panic!("comment {comment:?} vanished:\n{out}"));
        assert!(
            lines.get(ci + 1).is_some_and(|l| l.trim_start().starts_with(decl)),
            "{comment:?} no longer sits above {decl:?}:\n{out}"
        );
    }
    assert!(out.starts_with("// header line one"), "file header moved:\n{out}");
}

// ---- ADR-0010: `T?` Option shorthand ----

// The shorthand round-trips in every type position, and a written
// `Option[T]` NORMALIZES to it (D3: fmt owns the one canonical spelling).
#[test]
fn fmt_option_shorthand_roundtrips_and_normalizes() {
    let out = roundtrip("module app\nfn f(v: Int?) -> Int? = v");
    assert!(out.contains("fn f(v: Int?) -> Int? ="), "{out}");
    let out = roundtrip("module app\nfn f(v: Option[Int]) -> Option[Int] = v");
    assert!(out.contains("fn f(v: Int?) -> Int? ="), "normalization: {out}");
    let out = roundtrip("module app\nfn f(xs: List[Option[Int]]) -> List[Int?] = xs");
    assert!(out.contains("fn f(xs: List[Int?]) -> List[Int?] ="), "{out}");
}

// The normalized output must re-parse under the atom-binding rule: fn types
// and nested Option take parens; a tuple is already a parenthesized atom.
#[test]
fn fmt_option_shorthand_parenthesizes_non_atoms() {
    let out = roundtrip("module app\ntype Hooks = { on_tick: Option[(Int) -> Unit] }");
    assert!(out.contains("on_tick: (fn(Int) -> Unit)?"), "{out}");
    let out = roundtrip("module app\nfn f() -> Option[Option[Int]] = some(none)");
    assert!(out.contains("-> (Int?)? ="), "{out}");
    let out = roundtrip("module app\nfn f() -> Option[(String, Int)] = none");
    assert!(out.contains("-> (String, Int)? ="), "{out}");
    // every normalized form reaches a fixpoint
    for src in [
        "module app\ntype Hooks = { on_tick: Option[(Int) -> Unit] }",
        "module app\nfn f() -> Option[Option[Int]] = some(none)",
        "module app\nfn f() -> Option[(String, Int)] = none",
    ] {
        let once = roundtrip(src);
        assert_eq!(roundtrip(&once), once, "not idempotent for {src}");
    }
}

// `?` binds to the type atom, never across `->` (D2): a fn-type slot keeps
// its Option RETURN unparenthesized, and `?!` layers Result over Option.
#[test]
fn fmt_option_shorthand_atom_binding() {
    let out = roundtrip("module app\nfn pick(f: (Int) -> Int?) -> Int = 0");
    assert!(out.contains("f: fn(Int) -> Int?"), "{out}");
    let out = roundtrip("module app\nfn g(s: String) -> Int?! = ok(none)");
    assert!(out.contains("-> Int?! ="), "{out}");
}

// ---- Roundtrip & Idempotency over all spec/ files ----

#[test]
fn fmt_roundtrip_idempotency_all_spec_files() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut tested = 0u32;
    let mut failures = Vec::new();

    // No skip list: every checked-in .almd must parse, reformat, reparse, and
    // reach a fixpoint. A file that stops parsing is a FAILURE, not a skip —
    // silent skips shrink coverage without anyone noticing. (The old
    // regex_test.almd raw-string entry was stale: it round-trips today.)
    let mut files = walkdir(root.join("spec").as_path());
    files.extend(walkdir(root.join("examples").as_path()));

    for path in files {
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: read error: {}", path.display(), e));
                continue;
            }
        };

        // First parse — every checked-in .almd must parse.
        let tokens1 = Lexer::tokenize(&source);
        let mut parser1 = Parser::new(tokens1);
        let prog1 = match parser1.parse() {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{}: source failed to parse: {}", path.display(), e));
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

    eprintln!("fmt roundtrip/idempotency: {} files tested, 0 skipped", tested);

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

// ---- Semantic preservation (completeness §7) ----
//
// The roundtrip gate above asserts reparseability and a formatting fixpoint —
// it is structurally BLIND to formatting that changes meaning while staying
// parseable. Two real instances shipped: the unused-import scanner deleted
// imports whose only uses were in TYPE position, and the record-type printer
// dropped field DEFAULTS and serialization aliases. The unit pins below lock
// those exact classes, and `fmt_output_typechecks_single_file_specs` makes
// the general claim machine-checked: if a file type-checks, its formatted
// output must type-check too.

#[test]
fn fmt_keeps_import_used_only_in_type_position() {
    let out = roundtrip("import varlib\n\nfn h(p: varlib.Policy) -> Int = 1\n");
    assert!(out.contains("import varlib"),
        "an import whose only use is a TYPE position must survive formatting:\n{}", out);
}

#[test]
fn fmt_keeps_import_used_only_in_type_decl_payload() {
    let out = roundtrip("import varlib\n\ntype T = | A(varlib.Policy) | B\n");
    assert!(out.contains("import varlib"),
        "an import used only in a variant payload type must survive formatting:\n{}", out);
}

#[test]
fn fmt_keeps_record_field_defaults_and_aliases() {
    let out = roundtrip("type Cfg = { name: String = \"x\", n as \"num\": Int = 7 }\n");
    assert!(out.contains("= \"x\""), "field default dropped:\n{}", out);
    assert!(out.contains("= 7"), "field default dropped:\n{}", out);
    assert!(out.contains("as \"num\""), "serialization alias dropped:\n{}", out);
}

#[test]
fn fmt_keeps_variant_case_field_defaults() {
    let out = roundtrip("type S = | Rect { w: Float, color: String = \"\" } | Dot\n");
    assert!(out.contains("color: String = \"\""), "variant-case field default dropped:\n{}", out);
}

#[test]
fn fmt_output_typechecks_single_file_specs() {
    // Shells out to the freshly built binary: `almide fmt` a COPY of every
    // single-file spec, then `almide check` the formatted copy. Multi-module
    // corpora (spec/integration) need their sibling modules and are covered
    // by the unit pins above instead.
    let bin = {
        if let Ok(b) = std::env::var("ALMIDE_BIN") { b } else {
            let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
            if !p.exists() { return; } // debug-only invocation: covered in CI by the release run
            p.to_str().unwrap().to_string()
        }
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = walkdir(root.join("spec/lang").as_path());
    files.extend(walkdir(root.join("spec/stdlib").as_path()));
    files.extend(walkdir(root.join("spec/wasm_cross").as_path()));

    let dir = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    let mut tested = 0u32;
    for path in files {
        // Pre-condition: the ORIGINAL must check clean; files that do not
        // (deliberate negative fixtures) are out of scope for this gate.
        let orig_check = std::process::Command::new(&bin)
            .args(["check", path.to_str().unwrap()])
            .output().expect("almide check original");
        if !orig_check.status.success() { continue; }

        let copy = dir.path().join(path.file_name().unwrap());
        std::fs::copy(&path, &copy).unwrap();
        let fmt_out = std::process::Command::new(&bin)
            .args(["fmt", copy.to_str().unwrap()])
            .output().expect("almide fmt");
        if !fmt_out.status.success() {
            failures.push(format!("{}: fmt failed:\n{}", path.display(), String::from_utf8_lossy(&fmt_out.stderr)));
            continue;
        }
        let check = std::process::Command::new(&bin)
            .args(["check", copy.to_str().unwrap()])
            .output().expect("almide check formatted");
        if !check.status.success() {
            failures.push(format!(
                "{}: original checks clean but the FORMATTED output does not — fmt changed meaning:\n{}",
                path.display(), String::from_utf8_lossy(&check.stdout)
            ));
        }
        tested += 1;
    }
    assert!(failures.is_empty(),
        "fmt semantic-preservation gate: {} of {} file(s) failed:\n\n{}",
        failures.len(), tested, failures.join("\n\n"));
    assert!(tested > 100, "gate coverage collapsed: only {} files tested", tested);
}

#[test]
fn fmt_output_typechecks_multi_module_specs() {
    // §7 residual (#532): the single-file gate above copies each file ALONE
    // into a temp dir, so multi-module corpora were excluded (their sibling
    // modules would be missing). This gate copies the WHOLE spec/integration
    // tree, formats EVERY .almd in the copy, then re-checks each file with
    // its (also formatted) siblings present — fmt must preserve meaning
    // across module boundaries too (imports, cross-module types, aliases).
    let bin = {
        if let Ok(b) = std::env::var("ALMIDE_BIN") { b } else {
            let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
            if !p.exists() { return; } // debug-only invocation: covered in CI by the release run
            p.to_str().unwrap().to_string()
        }
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_root = root.join("spec/integration");
    let files = walkdir(src_root.as_path());

    // Copy the entire tree so every sibling/module is present in the copy.
    let dir = tempfile::tempdir().unwrap();
    let dst_root = dir.path().join("integration");
    for path in &files {
        let rel = path.strip_prefix(&src_root).unwrap();
        let dst = dst_root.join(rel);
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        std::fs::copy(path, &dst).unwrap();
    }

    let mut failures = Vec::new();
    let mut tested = 0u32;
    // Precompute which ORIGINALS check clean (negative fixtures are out of
    // scope), then format every copy, then re-check the clean set.
    let mut clean: Vec<std::path::PathBuf> = Vec::new();
    for path in &files {
        let ok = std::process::Command::new(&bin)
            .args(["check", path.to_str().unwrap()])
            .output().expect("almide check original").status.success();
        if ok { clean.push(path.clone()); }
    }
    for path in &files {
        let rel = path.strip_prefix(&src_root).unwrap();
        let copy = dst_root.join(rel);
        let fmt_out = std::process::Command::new(&bin)
            .args(["fmt", copy.to_str().unwrap()])
            .output().expect("almide fmt");
        if !fmt_out.status.success() && clean.contains(path) {
            failures.push(format!("{}: fmt failed:\n{}", path.display(), String::from_utf8_lossy(&fmt_out.stderr)));
        }
    }
    for path in &clean {
        let rel = path.strip_prefix(&src_root).unwrap();
        let copy = dst_root.join(rel);
        let check = std::process::Command::new(&bin)
            .args(["check", copy.to_str().unwrap()])
            .output().expect("almide check formatted");
        if !check.status.success() {
            failures.push(format!(
                "{}: original checks clean but the FORMATTED multi-module copy does not — fmt changed meaning:\n{}",
                path.display(), String::from_utf8_lossy(&check.stdout)
            ));
        }
        tested += 1;
    }
    assert!(failures.is_empty(),
        "fmt multi-module semantic-preservation gate: {} of {} file(s) failed:\n\n{}",
        failures.len(), tested, failures.join("\n\n"));
    assert!(tested > 10, "gate coverage collapsed: only {} files tested", tested);
}

/// `almide fmt --check` is a GATE: it must report which files are not formatted and
/// exit NON-ZERO, so a CI job written against it can actually fail. It used to print the
/// formatted text and exit 0 unconditionally — indistinguishable from success, which made
/// every gate written against it a no-op (#919). A DIRECTORY argument must recurse; it
/// used to reach the file reader as-is, print "Is a directory", and still exit 0.
/// `--dry-run` stays the SHOW mode from `docs/specs/cli.md`: print, never fail.
#[test]
fn fmt_check_exits_nonzero_on_unformatted_input_and_recurses_directories() {
    let bin = {
        if let Ok(b) = std::env::var("ALMIDE_BIN") { b } else {
            let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
            if !p.exists() { return; } // debug-only invocation: covered in CI by the release run
            p.to_str().unwrap().to_string()
        }
    };
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("nested");
    std::fs::create_dir_all(&nested).unwrap();

    // Deliberately unformatted: the formatter re-indents this body.
    let messy = nested.join("messy.almd");
    std::fs::write(&messy, "module app\nfn add(a: Int,b: Int) -> Int =\n        a+b\n").unwrap();

    let run = |args: &[&str]| {
        std::process::Command::new(&bin).args(args).output().expect("almide fmt")
    };

    let one = run(&["fmt", "--check", messy.to_str().unwrap()]);
    assert!(!one.status.success(), "fmt --check on an unformatted file must exit non-zero");
    let one_err = String::from_utf8_lossy(&one.stderr);
    assert!(one_err.contains("not formatted"), "it must NAME the file; got:\n{one_err}");

    // The same file reached through its DIRECTORY: the walk must find it.
    let recursed = run(&["fmt", "--check", dir.path().to_str().unwrap()]);
    assert!(!recursed.status.success(), "fmt --check on a directory must recurse and fail");
    assert!(
        String::from_utf8_lossy(&recursed.stderr).contains("messy.almd"),
        "the directory walk must reach nested files"
    );

    // Formatting it makes the same check pass — the gate tracks the file, not a constant.
    let wrote = run(&["fmt", messy.to_str().unwrap()]);
    assert!(wrote.status.success(), "plain fmt must write the file back");
    let after = run(&["fmt", "--check", dir.path().to_str().unwrap()]);
    assert!(
        after.status.success(),
        "a formatted tree must pass --check; stderr:\n{}",
        String::from_utf8_lossy(&after.stderr)
    );

    // `--dry-run` prints the formatted text and never fails, even unformatted.
    std::fs::write(&messy, "module app\nfn add(a: Int,b: Int) -> Int =\n        a+b\n").unwrap();
    let dry = run(&["fmt", "--dry-run", messy.to_str().unwrap()]);
    assert!(dry.status.success(), "--dry-run must not fail on unformatted input");
    assert!(
        String::from_utf8_lossy(&dry.stdout).contains("fn add"),
        "--dry-run must print the formatted text"
    );
}
