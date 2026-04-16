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
    // `parse_errors` is consumed inside the rule engine (per-rule
    // re-parse) so we discard it here.
    let (mut program, source_text, _parse_errors) = parse_file(file);

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

    // Source-level keyword removals. Both `let-in` and `return` share the
    // same shape: find the keyword at positions reported by parser
    // diagnostics, delete it plus one trailing space, word-boundary-check
    // so `into` / `return_value` etc. don't get clipped. Iterate to
    // fixpoint because parser recovery surfaces only the first occurrence
    // per pass.
    let letin_count = LETIN_REMOVAL.apply(&mut working);
    let return_count = RETURN_REMOVAL.apply(&mut working);

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

    // Exit code contract for harness integration:
    //   0 — file is clean (or was made clean by auto-fixes; no manual work left)
    //   1 — manual fixes still pending (harness should forward diagnostics to LLM retry)
    // Write errors elsewhere already exit(1); here we only signal the
    // "post-fix clean / dirty" bit. --dry-run never exits dirty so preview
    // invocations don't surprise callers that pipe them.
    if !dry_run && !manual.is_empty() {
        std::process::exit(1);
    }
}

/// Delegate to the canonical comparison-operator table in
/// `almide::stdlib::comparison_operator_of` so `almide fix`'s AST rewrite,
/// the E002 try: snippet, and `suggest_alias`'s "Did you mean?" hint
/// stay in perfect sync.
fn comparison_fn_to_operator(module: &str, func: &str) -> Option<&'static str> {
    almide::stdlib::comparison_operator_of(module, func)
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

/// Source-level fix: delete occurrences of a single keyword at positions
/// reported by parser diagnostics. Shared engine for `let-in` (`in`
/// keyword) and `return` keyword removal — the two rules differ only in
/// `keyword` and `diag_matches`, every other detail is the same.
///
/// Iterates to fixpoint because parser recovery surfaces only the first
/// occurrence per pass. `max_iter` caps runaway in case a rule becomes
/// pathological (e.g. the same position keeps being detected post-edit).
struct KeywordRemoval {
    keyword: &'static str,
    /// Predicate over diagnostic messages identifying the rule's trigger.
    diag_matches: fn(&str) -> bool,
    max_iter: usize,
}

const LETIN_REMOVAL: KeywordRemoval = KeywordRemoval {
    keyword: "in",
    diag_matches: |m| m.contains("`let ... in <expr>`"),
    max_iter: 8,
};

const RETURN_REMOVAL: KeywordRemoval = KeywordRemoval {
    keyword: "return",
    diag_matches: |m| m.starts_with("'return' is not needed in Almide"),
    max_iter: 8,
};

impl KeywordRemoval {
    /// Apply the rule to `source` until no more matches, in place. Returns
    /// total occurrences removed across all iterations.
    fn apply(&self, source: &mut String) -> usize {
        let mut total = 0;
        for _ in 0..self.max_iter {
            let positions = self.collect_positions(source);
            if positions.is_empty() { break; }
            total += positions.len();
            *source = self.delete_at(source, &positions);
        }
        total
    }

    fn collect_positions(&self, source: &str) -> Vec<(usize, usize)> {
        let tokens = almide::lexer::Lexer::tokenize(source);
        let mut parser = almide::parser::Parser::new(tokens);
        let _ = parser.parse();
        parser.errors.iter()
            .filter(|d| (self.diag_matches)(&d.message))
            .filter_map(|d| Some((d.line?, d.col?)))
            .collect()
    }

    fn delete_at(&self, source: &str, positions: &[(usize, usize)]) -> String {
        let klen = self.keyword.len();
        let mut lines: Vec<String> = source.split('\n').map(String::from).collect();
        // Apply edits in reverse so earlier positions aren't invalidated by
        // later ones on the same line.
        let mut sorted: Vec<_> = positions.iter().copied().collect();
        sorted.sort_by(|a, b| b.cmp(a));
        for (line, col) in sorted {
            let li = line.saturating_sub(1);
            let Some(l) = lines.get_mut(li) else { continue };
            let ci = col.saturating_sub(1);
            if l.get(ci..ci + klen) != Some(self.keyword) { continue; }
            if !word_boundary_ok(l.as_bytes(), ci, ci + klen) { continue; }
            // Delete the keyword plus one trailing space if present. For a
            // lone-on-indent line (e.g. `  in <body>`), collapse to empty.
            let mut end = ci + klen;
            if l.as_bytes().get(end) == Some(&b' ') { end += 1; }
            let new_line = format!("{}{}", &l[..ci], &l[end..]);
            *l = if new_line.trim().is_empty() {
                String::new()
            } else {
                new_line
            };
        }
        lines.join("\n")
    }
}

/// Standard identifier-boundary check: the chars at `start-1` and `end` must
/// not be identifier continuations, or be at the source edges. Used to
/// avoid clipping `into` / `return_value` / `in_flight` etc.
fn word_boundary_ok(bytes: &[u8], start: usize, end: usize) -> bool {
    let before_ok = start == 0
        || (!bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_');
    let after_ok = match bytes.get(end).copied() {
        None => true,
        Some(b) => !b.is_ascii_alphanumeric() && b != b'_',
    };
    before_ok && after_ok
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
