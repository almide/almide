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

pub fn cmd_fix(file: &str, dry_run: bool) {
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

    let any_change = has_import_changes || operator_count > 0 || letin_count > 0;

    let op_msg = |n: usize| format!(
        "Rewrote {} comparison function call(s) to operator form (int.gt/lt/eq/... → > < == ...)", n
    );

    if dry_run {
        if !any_change {
            println!("no auto-applicable fixes");
        } else {
            println!("--- would apply ---");
            for m in &import_messages {
                println!("  {}", m);
            }
            if operator_count > 0 { println!("  {}", op_msg(operator_count)); }
            if letin_count > 0 {
                println!("  Removed {} OCaml-style `in` keyword(s) (let-in → newline chain)", letin_count);
            }
            println!("\n--- new file contents ---");
            println!("{}", working);
        }
    } else if any_change {
        if let Err(e) = std::fs::write(file, &working) {
            eprintln!("error: failed to write {}: {}", file, e);
            std::process::exit(1);
        }
        eprintln!("{}:", file);
        for m in &import_messages {
            eprintln!("  {}", m);
        }
        if operator_count > 0 { eprintln!("  {}", op_msg(operator_count)); }
        if letin_count > 0 {
            eprintln!("  Removed {} OCaml-style `in` keyword(s) (let-in → newline chain)", letin_count);
        }
    }

    // After auto-fixes, report any remaining `try:` snippets so callers
    // know what's left to do by hand.
    report_manual_fixes(file, &working);
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

fn report_manual_fixes(file: &str, source: &str) {
    use almide::check::Checker;
    use almide::canonicalize;
    use almide::diagnostic;

    // Re-parse the (possibly modified) source and type-check.
    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens);
    let mut prog = match parser.parse() {
        Ok(p) => p,
        Err(_) => return,
    };
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.set_source(file, source);
    checker.diagnostics = canon.diagnostics;
    let diagnostics = checker.infer_program(&mut prog);

    let manual: Vec<&diagnostic::Diagnostic> = diagnostics.iter()
        .chain(parser.errors.iter())
        .filter(|d| d.level == diagnostic::Level::Error && d.try_snippet.is_some())
        .collect();

    if manual.is_empty() {
        return;
    }
    eprintln!("\n{} diagnostic(s) have `try:` snippets that need manual application:",
        manual.len());
    for d in &manual {
        let loc = match (d.line, d.col) {
            (Some(l), Some(c)) => format!("{}:{}", l, c),
            (Some(l), None) => format!("{}", l),
            _ => "?".into(),
        };
        let code = d.code.as_deref().unwrap_or("E???");
        eprintln!("  [{code}] {file}:{loc}  {}", d.message);
    }
    eprintln!("\nRun `almide check {}` for the full text of each `try:` snippet.", file);
}
