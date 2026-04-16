//! `almide fix` — apply mechanically-safe fixes to a source file.
//!
//! **Current scope**:
//! - `auto_imports` — adds missing `import json` / `import fs` / etc.
//! - **let-in → newline chain** — deletes the OCaml-style `in` keyword
//!   where the parser recovered from `let x = expr\n  in <body>`. The
//!   trailing body is already valid Almide; we just drop the token.
//!
//! Remaining `try:` snippets (cons patterns, int.gt operator rewrites,
//! E001 Unit-leak structural rewrites) are reported as "manual fix
//! needed" until the deterministic rewrite infrastructure grows to
//! cover them too.

use crate::{parse_file, project, project_fetch};
use almide::ast::{self, Expr, ExprKind};
use almide::fmt::{auto_imports, format_program};
use almide_base::intern::sym;
use serde::Serialize;

/// JSON output shape (stable contract for harnesses) so the dojo retry loop
/// can decide "re-check vs pass-through to LLM" without parsing human text.
/// Bump on any breaking change (field removal, semantic shift). Additive
/// changes (new fields) don't require a bump — harnesses should ignore
/// unknown fields.
const FIX_REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
struct FixReport<'a> {
    schema_version: u32,
    file: &'a str,
    imports_added: Vec<String>,
    letin_removed: usize,
    operator_rewrites: usize,
    return_removed: usize,
    manual_pending: Vec<ManualDiag>,
    /// True if the file was written (or would be, in --dry-run). Harness can
    /// use this to gate a follow-up `almide check`: if false, nothing changed
    /// and retry proceeds with the original diagnostics.
    changed: bool,
    dry_run: bool,
}

#[derive(Serialize)]
struct ManualDiag {
    code: String,
    line: Option<usize>,
    col: Option<usize>,
    message: String,
}

pub fn cmd_fix(file: &str, dry_run: bool, json: bool) {
    let (mut program, source_text, parse_errors) = parse_file(file);

    let (dep_names, dep_submodules): (Vec<String>, std::collections::HashMap<String, String>) =
        if std::path::Path::new("almide.toml").exists() {
            match project::parse_toml(std::path::Path::new("almide.toml")) {
                Ok(proj) => {
                    let fetched = project_fetch::fetch_all_deps(&proj).unwrap_or_default();
                    let names: Vec<String> = fetched.iter().map(|fd| fd.pkg_id.name.clone()).collect();
                    (names, std::collections::HashMap::new())
                }
                Err(_) => (vec![], std::collections::HashMap::new()),
            }
        } else {
            (vec![], std::collections::HashMap::new())
        };

    let import_messages = auto_imports(&mut program, &dep_names, &dep_submodules);
    let has_import_changes = !import_messages.is_empty();

    // AST-level rewrite: `int.gt(a, b)` / `.lt` / `.eq` / `.neq` / `.le` /
    // `.ge` etc. (on int/float/string/bool) → the corresponding operator.
    // Almide never defined these comparison functions; LLMs reach for them
    // from Go-ish / Java-ish training data. Mechanically substituting to
    // `a > b` etc. turns the error case into working code.
    let operator_count = rewrite_comparison_calls(&mut program);
    let has_ast_changes = has_import_changes || operator_count > 0;

    // Start from the formatter output if any AST-level change happened,
    // else keep the original text verbatim so other textual fixes don't
    // reformat things they shouldn't.
    let mut working = if has_ast_changes {
        format_program(&program)
    } else {
        source_text.clone()
    };

    // Textual rewrite: let-in → drop `in`. The parse_errors collected above
    // correspond to the ORIGINAL source (line/col match pre-edit). If we've
    // already reformatted via AST-level fixes, re-parse to get positions
    // that match the working text.
    let letin_errors: Vec<(usize, usize)> = if has_ast_changes {
        collect_letin_positions(&working)
    } else {
        parse_errors.iter()
            .filter(|d| d.message.contains("`let ... in <expr>`"))
            .filter_map(|d| Some((d.line?, d.col?)))
            .collect()
    };

    let letin_count = letin_errors.len();
    if letin_count > 0 {
        working = delete_in_tokens(&working, &letin_errors);
    }

    // `return` keyword removal. Almide has no `return`; the trailing expr of
    // a block/fn is the value. LLMs habitually write `return expr` from
    // Go/Rust/JS — same mechanical fix shape as let-in (delete the keyword,
    // keep the following expression). Parser recovery surfaces only the
    // FIRST `return` per file, so iterate to fixpoint (capped) to sweep
    // multiple returns in one pass.
    let mut return_count = 0;
    for _ in 0..8 {
        let positions = collect_return_positions(&working);
        if positions.is_empty() { break; }
        return_count += positions.len();
        working = delete_return_tokens(&working, &positions);
    }

    let any_change = has_import_changes || operator_count > 0 || letin_count > 0 || return_count > 0;

    // Extract "Added `import X`" → bare module names for JSON.
    let imports_added: Vec<String> = import_messages.iter()
        .filter_map(|m| m.strip_prefix("Added `import ").and_then(|s| s.strip_suffix('`')))
        .map(String::from)
        .collect();

    let manual = collect_manual_fixes(file, &working);

    if !dry_run && any_change {
        if let Err(e) = std::fs::write(file, &working) {
            eprintln!("error: failed to write {}: {}", file, e);
            std::process::exit(1);
        }
    }

    if json {
        let report = FixReport {
            schema_version: FIX_REPORT_SCHEMA_VERSION,
            file,
            imports_added,
            letin_removed: letin_count,
            operator_rewrites: operator_count,
            return_removed: return_count,
            manual_pending: manual,
            changed: any_change,
            dry_run,
        };
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        return;
    }

    // Human output (default).
    let op_msg = |n: usize| format!(
        "Rewrote {} comparison function call(s) to operator form (int.gt/lt/eq/... → > < == ...)", n
    );
    if dry_run {
        if !any_change {
            println!("no auto-applicable fixes");
        } else {
            println!("--- would apply ---");
            for m in &import_messages { println!("  {}", m); }
            if operator_count > 0 { println!("  {}", op_msg(operator_count)); }
            if letin_count > 0 {
                println!("  Removed {} OCaml-style `in` keyword(s) (let-in → newline chain)", letin_count);
            }
            if return_count > 0 {
                println!("  Removed {} `return` keyword(s) (Almide uses trailing expression)", return_count);
            }
            println!("\n--- new file contents ---");
            println!("{}", working);
        }
    } else if any_change {
        eprintln!("{}:", file);
        for m in &import_messages { eprintln!("  {}", m); }
        if operator_count > 0 { eprintln!("  {}", op_msg(operator_count)); }
        if letin_count > 0 {
            eprintln!("  Removed {} OCaml-style `in` keyword(s) (let-in → newline chain)", letin_count);
        }
        if return_count > 0 {
            eprintln!("  Removed {} `return` keyword(s) (Almide uses trailing expression)", return_count);
        }
    }

    if !manual.is_empty() {
        eprintln!("\n{} diagnostic(s) have `try:` snippets that need manual application:", manual.len());
        for d in &manual {
            let loc = match (d.line, d.col) {
                (Some(l), Some(c)) => format!("{}:{}", l, c),
                (Some(l), None) => format!("{}", l),
                _ => "?".into(),
            };
            eprintln!("  [{code}] {file}:{loc}  {}", d.message, code = d.code);
        }
        eprintln!("\nRun `almide check {}` for the full text of each `try:` snippet.", file);
    }
}

/// Map `<module>.<func>` → operator symbol for comparison-style calls LLMs
/// commonly write but which Almide doesn't define (Go / Java idioms).
fn comparison_fn_to_operator(module: &str, func: &str) -> Option<&'static str> {
    match (module, func) {
        ("int" | "float", "gt") => Some(">"),
        ("int" | "float", "lt") => Some("<"),
        ("int" | "float", "gte" | "ge") => Some(">="),
        ("int" | "float", "lte" | "le") => Some("<="),
        ("int" | "float" | "string" | "bool", "eq") => Some("=="),
        ("int" | "float" | "string" | "bool", "neq" | "ne") => Some("!="),
        _ => None,
    }
}

/// Walk the program and rewrite every `<m>.<op>(a, b)` call whose
/// `(module, func)` resolves via `comparison_fn_to_operator` into a
/// `Binary` expression. Returns the number of rewrites performed.
fn rewrite_comparison_calls(program: &mut ast::Program) -> usize {
    let mut count = 0;
    ast::visit_exprs_mut(program, &mut |expr: &mut Expr| {
        let (op_sym, left_box, right_box) = match &mut expr.kind {
            ExprKind::Call { callee, args, named_args, type_args } => {
                if !named_args.is_empty() || type_args.is_some() || args.len() != 2 {
                    return;
                }
                let Some((module, func)) = extract_module_call(callee) else { return };
                let Some(op) = comparison_fn_to_operator(&module, &func) else { return };
                // Take the args out of the Call without mutating yet —
                // we'll rebuild the whole expr.kind below.
                let mut drained = std::mem::take(args);
                let right = drained.pop().unwrap();
                let left = drained.pop().unwrap();
                (op, Box::new(left), Box::new(right))
            }
            _ => return,
        };
        expr.kind = ExprKind::Binary {
            op: sym(op_sym),
            left: left_box,
            right: right_box,
        };
        count += 1;
    });
    count
}

/// If `callee` is the expression `<module>.<func>` (a Member access on a
/// bare module ident), return (module, func). Otherwise None.
fn extract_module_call(callee: &Expr) -> Option<(String, String)> {
    let ExprKind::Member { object, field } = &callee.kind else { return None };
    let ExprKind::Ident { name } = &object.kind else { return None };
    Some((name.to_string(), field.to_string()))
}

fn collect_return_positions(source: &str) -> Vec<(usize, usize)> {
    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens);
    let _ = parser.parse();
    parser.errors.iter()
        .filter(|d| d.message.starts_with("'return' is not needed in Almide"))
        .filter_map(|d| Some((d.line?, d.col?)))
        .collect()
}

fn delete_return_tokens(source: &str, positions: &[(usize, usize)]) -> String {
    let mut lines: Vec<String> = source.split('\n').map(String::from).collect();
    let mut sorted: Vec<_> = positions.iter().copied().collect();
    sorted.sort_by(|a, b| b.cmp(a));
    for (line, col) in sorted {
        let li = line.saturating_sub(1);
        let Some(l) = lines.get_mut(li) else { continue };
        let ci = col.saturating_sub(1);
        if l.get(ci..ci + 6) != Some("return") { continue; }
        // word boundaries
        let before_ok = ci == 0
            || !l.as_bytes()[ci - 1].is_ascii_alphanumeric()
                && l.as_bytes()[ci - 1] != b'_';
        let after_byte = l.as_bytes().get(ci + 6).copied();
        let after_ok = match after_byte {
            None => true,
            Some(b) => !b.is_ascii_alphanumeric() && b != b'_',
        };
        if !(before_ok && after_ok) { continue; }
        // Delete `return` plus a single trailing space if present, so
        // `return 42` becomes `42` and the trailing expression slides into
        // tail position naturally.
        let mut end = ci + 6;
        if l.as_bytes().get(end) == Some(&b' ') { end += 1; }
        let new_line = format!("{}{}", &l[..ci], &l[end..]);
        if new_line.trim().is_empty() {
            *l = String::new();
        } else {
            *l = new_line;
        }
    }
    lines.join("\n")
}

fn collect_letin_positions(source: &str) -> Vec<(usize, usize)> {
    // Re-parse to find let-in diagnostic positions in the possibly-modified
    // `source`. Share the diagnostic-detection code with the parser by
    // invoking it and filtering on the message string.
    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens);
    let _ = parser.parse();
    parser.errors.iter()
        .filter(|d| d.message.contains("`let ... in <expr>`"))
        .filter_map(|d| Some((d.line?, d.col?)))
        .collect()
}

/// Delete `in` keyword tokens at the given (line, col) positions.
/// Positions are 1-indexed as reported by the parser.
fn delete_in_tokens(source: &str, positions: &[(usize, usize)]) -> String {
    let mut lines: Vec<String> = source.split('\n').map(String::from).collect();
    // Apply edits in reverse so earlier positions aren't invalidated by
    // later ones on the same line.
    let mut sorted: Vec<_> = positions.iter().copied().collect();
    sorted.sort_by(|a, b| b.cmp(a));
    for (line, col) in sorted {
        let li = line.saturating_sub(1);
        let Some(l) = lines.get_mut(li) else { continue };
        let ci = col.saturating_sub(1);
        // Sanity: we expect `in` as a word at byte column `ci`.
        if l.get(ci..ci + 2) != Some("in") {
            continue;
        }
        // Check word boundaries so we don't clip things like `into`.
        let before_ok = ci == 0
            || !l.as_bytes()[ci - 1].is_ascii_alphanumeric()
                && l.as_bytes()[ci - 1] != b'_';
        let after_byte = l.as_bytes().get(ci + 2).copied();
        let after_ok = match after_byte {
            None => true,
            Some(b) => !b.is_ascii_alphanumeric() && b != b'_',
        };
        if !(before_ok && after_ok) { continue; }
        // Delete `in` plus a single trailing space (if present) so the
        // result reads cleanly. If `in` sits alone on an indented line
        // (e.g. `  in <body>`), also trim the now-trailing whitespace so
        // we don't leave a blank indented line behind.
        let mut end = ci + 2;
        if l.as_bytes().get(end) == Some(&b' ') { end += 1; }
        let new_line = format!("{}{}", &l[..ci], &l[end..]);
        // If removing `in` leaves only whitespace, collapse the line.
        if new_line.trim().is_empty() {
            *l = String::new();
        } else {
            *l = new_line;
        }
    }
    lines.join("\n")
}

/// Re-parse + type-check `source` and extract every diagnostic that has a
/// `try:` snippet. These are the "manual fix needed" items returned to the
/// caller (human reporter or JSON serializer). We don't print from here so
/// the JSON path can emit a clean object.
fn collect_manual_fixes(file: &str, source: &str) -> Vec<ManualDiag> {
    use almide::check::Checker;
    use almide::canonicalize;
    use almide::diagnostic;

    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens);
    let mut prog = match parser.parse() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.set_source(file, source);
    checker.diagnostics = canon.diagnostics;
    let diagnostics = checker.infer_program(&mut prog);

    diagnostics.iter()
        .chain(parser.errors.iter())
        .filter(|d| d.level == diagnostic::Level::Error && d.try_snippet.is_some())
        .map(|d| ManualDiag {
            code: d.code.unwrap_or("E???").to_string(),
            line: d.line,
            col: d.col,
            message: d.message.clone(),
        })
        .collect()
}
