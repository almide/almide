//! `almide fix` — apply mechanically-safe fixes to a source file.
//!
//! **The engine (#1312)**: fixes come from the diagnostics themselves.
//! A diagnostic that knows the span and the replacement IS the fix — it
//! attaches a `(span, replacement, Applicability)` triple, and this
//! command applies every triple tagged `MachineApplicable`, re-checks, and
//! iterates. There is no per-code branch anywhere in the loop, so a new
//! diagnostic that can state its fix precisely gets auto-fix for free.
//!
//! Two safety rails, both of which prefer "no fix" over "wrong fix":
//! - **Only `MachineApplicable`.** Everything else — a rename picked by
//!   edit distance, a `T` → `Option[T]` rewrite, a placement heuristic —
//!   is reported for a human or a model to choose. `Diagnostic::machine_fix`
//!   is the single read path, so an untagged fix-it cannot leak in.
//! - **A round is discarded if it made the parse worse.** After applying a
//!   batch we re-parse; if the parse-error count went UP the batch is
//!   dropped and the previous text stands.
//!
//! **Still hand-maintained** (each one cannot be stated as a single exact
//! span, so tagging it machine-applicable would be a lie):
//! - `auto_imports` — the import block's correct position depends on what
//!   is already there; the diagnostic's 1:1 insert is a placement guess.
//!   Shared with `almide fmt`, which owns that block.
//! - comparison-call → operator (`int.gt(a, b)` → `a > b`) — needs both
//!   argument sub-trees and a precedence decision; a textual splice would
//!   silently reassociate the surrounding expression.
//! - `return` removal — deleting `return` is only meaning-preserving in
//!   tail position, and the parser cannot establish that from the token
//!   alone. It stays a text rule so the applicability tag stays honest.

use crate::{parse_file, project, project_fetch, out, err};
use almide::ast::{self, Expr, ExprKind};
use almide::fmt::{auto_imports, format_program};
use almide_base::diagnostic::Diagnostic;
use almide_base::intern::sym;
use serde::Serialize;

/// JSON output shape (stable contract for harnesses) so the dojo retry loop
/// can decide "re-check vs pass-through to LLM" without parsing human text.
/// Bump on any breaking change (field removal, semantic shift). Additive
/// changes (new fields) don't require a bump — harnesses should ignore
/// unknown fields.
const FIX_REPORT_SCHEMA_VERSION: u32 = 1;

/// Cap on engine rounds. rustfix settled on the same number for the same
/// reason: overlapping spans push a fix into a later round, a handful of
/// rounds covers every real file, and the cap bounds a pathological loop
/// where a fix-it keeps re-triggering itself.
const MAX_FIX_ROUNDS: usize = 4;

#[derive(Serialize)]
struct FixReport<'a> {
    schema_version: u32,
    file: &'a str,
    imports_added: Vec<String>,
    letin_removed: usize,
    operator_rewrites: usize,
    range_rewrites: usize,
    return_removed: usize,
    /// #1312: every span-anchored fix-it this run applied, in application
    /// order. Each one is `machine-applicable` by construction — the engine
    /// reads nothing else.
    applied: Vec<FixItJson>,
    /// #1312: the span-anchored fix-its still on the table after fixing.
    /// These carry an exact span but an applicability that forbids an
    /// unattended apply, so they are the retry loop's menu, not its work.
    suggestions: Vec<FixItJson>,
    manual_pending: Vec<ManualDiag>,
    /// True if the file was written (or would be, in --dry-run). Harness can
    /// use this to gate a follow-up `almide check`: if false, nothing changed
    /// and retry proceeds with the original diagnostics.
    changed: bool,
    dry_run: bool,
}

/// A `(span, replacement, applicability)` triple lifted out of a diagnostic.
/// The engine's unit of work and the JSON report's unit of evidence.
#[derive(Serialize, Clone)]
struct FixItJson {
    code: String,
    line: usize,
    col: usize,
    end_col: usize,
    replacement: String,
    applicability: String,
}

#[derive(Serialize)]
struct ManualDiag {
    code: String,
    line: Option<usize>,
    col: Option<usize>,
    message: String,
    /// #1312: why this one was not applied — `maybe-incorrect`,
    /// `has-placeholders`, or `unspecified` (a display-only snippet).
    applicability: String,
}

/// Bundle for `report_fix_result` — was 11 positional params (a max-params
/// violation of its own), grouped here into the one struct `cmd_fix` already
/// builds `FixReport` from. Fields are read-only from every helper except
/// the JSON branch, which consumes them once to build `FixReport`.
struct FixOutcome<'a> {
    file: &'a str,
    working: &'a str,
    import_messages: &'a [String],
    imports_added: Vec<String>,
    operator_count: usize,
    return_count: usize,
    /// The engine's output: every machine-applicable fix-it applied.
    /// The per-code counts below are views over this list — there is no
    /// second bookkeeping to drift from it.
    applied: Vec<FixItJson>,
    suggestions: Vec<FixItJson>,
    manual: Vec<ManualDiag>,
    any_change: bool,
}

impl FixOutcome<'_> {
    /// How many applied fix-its came from diagnostic `code`.
    fn applied_count(&self, code: &str) -> usize {
        self.applied.iter().filter(|f| f.code == code).count()
    }
}

/// `print_fix_human_summary`'s per-rule detail lines — shared by both the
/// dry-run preview (sink `out`) and the diff summary (sink `err`).
///
/// The named lines exist because they are the wording harnesses already
/// grep for; everything the engine applies beyond them is reported
/// generically, keyed by the diagnostic code that stated the fix.
fn print_fix_detail_lines(outcome: &FixOutcome, print: fn(&str)) {
    if outcome.operator_count > 0 {
        print(&format!(
            "  Rewrote {} comparison function call(s) to operator form (int.gt/lt/eq/... → > < == ...)",
            outcome.operator_count
        ));
    }
    let range_count = outcome.applied_count("E031");
    if range_count > 0 {
        print(&format!(
            "  Migrated {} retired range spelling(s) (`..` -> `..<`, `..=` -> `...`)",
            range_count
        ));
    }
    let letin_count = outcome.applied_count("E049");
    if letin_count > 0 {
        print(&format!("  Removed {} OCaml-style `in` keyword(s) (let-in → newline chain)", letin_count));
    }
    if outcome.return_count > 0 {
        print(&format!("  Removed {} `return` keyword(s) (Almide uses trailing expression)", outcome.return_count));
    }
    // Everything else the engine applied, attributed to the diagnostic
    // that stated it. No per-code branch here — a new machine-applicable
    // fix-it shows up in this line the day it is emitted.
    let other: Vec<&FixItJson> = outcome.applied.iter()
        .filter(|f| f.code != "E031" && f.code != "E049")
        .collect();
    if !other.is_empty() {
        let codes: Vec<String> = other.iter()
            .map(|f| format!("{} at line {}", f.code, f.line))
            .collect();
        print(&format!("  Applied {} machine-applicable fix-it(s) from diagnostics: {}",
            other.len(), codes.join(", ")));
    }
}

/// `report_fix_result`'s human dry-run preview / human diff summary.
/// Extracted verbatim — reads only `outcome`, writes only stdout/stderr.
fn print_fix_human_summary(outcome: &FixOutcome, dry_run: bool) {
    if dry_run {
        if !outcome.any_change {
            out(&format!("no auto-applicable fixes"));
            return;
        }
        out(&format!("--- would apply ---"));
        for m in outcome.import_messages { out(&format!("  {}", m)); }
        print_fix_detail_lines(outcome, out);
        out(&format!("\n--- new file contents ---"));
        out(&format!("{}", outcome.working));
        return;
    }
    if outcome.any_change {
        err(&format!("{}:", outcome.file));
        for m in outcome.import_messages { err(&format!("  {}", m)); }
        print_fix_detail_lines(outcome, err);
    }
}

/// `report_fix_result`'s manual-fix listing. Extracted verbatim — reads
/// only `outcome`, writes only stderr.
fn print_fix_manual_pending(outcome: &FixOutcome) {
    if !outcome.manual.is_empty() {
        err(&format!("\n{} diagnostic(s) have `try:` snippets that need manual application:", outcome.manual.len()));
        for d in &outcome.manual {
            let loc = match (d.line, d.col) {
                (Some(l), Some(c)) => format!("{}:{}", l, c),
                (Some(l), None) => format!("{}", l),
                _ => "?".into(),
            };
            err(&format!("  [{code}] {file}:{loc}  {} ({app})",
                d.message, code = d.code, file = outcome.file, app = d.applicability));
        }
        err(&format!("\nRun `almide check {}` for the full text of each `try:` snippet.", outcome.file));
    }
}

/// `cmd_fix`'s reporting tail — JSON report / human dry-run preview / human
/// diff summary / manual-fix listing / exit-code contract. Extracted
/// verbatim as the function's true tail (nothing in `cmd_fix` runs after
/// this call), so the early `return` inside the JSON branch is preserved
/// exactly (it terminated `cmd_fix` before too — there was nothing after).
fn report_fix_result(outcome: FixOutcome, dry_run: bool, json: bool) {
    if json {
        let (letin_removed, range_rewrites) = (outcome.applied_count("E049"), outcome.applied_count("E031"));
        let report = FixReport {
            schema_version: FIX_REPORT_SCHEMA_VERSION,
            file: outcome.file,
            imports_added: outcome.imports_added,
            letin_removed,
            operator_rewrites: outcome.operator_count,
            range_rewrites,
            return_removed: outcome.return_count,
            applied: outcome.applied,
            suggestions: outcome.suggestions,
            manual_pending: outcome.manual,
            changed: outcome.any_change,
            dry_run,
        };
        out(&format!("{}", serde_json::to_string_pretty(&report).unwrap()));
        return;
    }

    // Human output (default).
    print_fix_human_summary(&outcome, dry_run);
    print_fix_manual_pending(&outcome);

    // Exit code contract for harness integration:
    //   0 — file is clean (or was made clean by auto-fixes; no manual work left)
    //   1 — manual fixes still pending (harness should forward diagnostics to LLM retry)
    // Write errors elsewhere already exit(1); here we only signal the
    // "post-fix clean / dirty" bit. --dry-run never exits dirty so preview
    // invocations don't surprise callers that pipe them.
    if !dry_run && !outcome.manual.is_empty() {
        std::process::exit(1);
    }
}

// ── The generic engine ────────────────────────────────────────────────

/// Parse + type-check `source` and hand back every diagnostic the compiler
/// produced, parser and checker alike. This is the engine's ONLY input —
/// there is no per-code table anywhere below it.
///
/// The checker runs only on a clean parse (#1077): on a recovery AST the
/// checker reasons about a program the author did not write, and a fix-it
/// derived from it would be anchored to a span the parser invented.
fn collect_diagnostics(file: &str, source: &str) -> (Vec<Diagnostic>, usize) {
    use almide::check::Checker;
    use almide::canonicalize;

    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens).with_file(file);
    let Ok(mut prog) = parser.parse() else {
        // A parse that could not even recover — one hard error, no spans
        // we would trust.
        return (Vec::new(), 1);
    };
    let parse_errors = std::mem::take(&mut parser.errors);
    let parse_error_count = parse_errors.len();
    if parse_error_count > 0 {
        return (parse_errors, parse_error_count);
    }

    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.set_source(file, source);
    checker.diagnostics = canon.diagnostics;
    (checker.infer_program(&mut prog), 0)
}

/// True when two single-line replacement ranges touch. Half-open
/// `[col, end_col)`; a zero-width insertion is a point, which conflicts
/// with any range that contains it and with another insertion at the same
/// point. Two fixes that overlap cannot both be spliced into one text, so
/// the loser waits for the next round.
fn spans_overlap(a: (usize, usize, usize), b: (usize, usize, usize)) -> bool {
    if a.0 != b.0 { return false; }
    let ((a_s, a_e), (b_s, b_e)) = ((a.1, a.2), (b.1, b.2));
    match (a_s == a_e, b_s == b_e) {
        (true, true) => a_s == b_s,
        (true, false) => a_s >= b_s && a_s < b_e,
        (false, true) => b_s >= a_s && b_s < a_e,
        (false, false) => a_s < b_e && b_s < a_e,
    }
}

/// Pick the batch of machine-applicable fix-its that can be spliced into
/// `source` in one pass: every one whose span does not overlap a fix
/// already chosen, deduped, in DESCENDING span order so that applying one
/// never shifts the offsets of the ones still to come.
fn select_batch(diagnostics: &[Diagnostic]) -> Vec<(&Diagnostic, (usize, usize, usize), String)> {
    let mut candidates: Vec<(&Diagnostic, (usize, usize, usize), String)> = diagnostics.iter()
        .filter_map(|d| {
            let (line, col, end_col, replacement) = d.machine_fix()?;
            Some((d, (line, col, end_col), replacement.to_string()))
        })
        .collect();
    candidates.sort_by_key(|(_, span, _)| std::cmp::Reverse(*span));
    let mut chosen: Vec<(&Diagnostic, (usize, usize, usize), String)> = Vec::new();
    for cand in candidates {
        if chosen.iter().any(|(_, span, _)| spans_overlap(*span, cand.1)) { continue; }
        chosen.push(cand);
    }
    chosen
}

/// Apply one batch to `source`. Returns the rewritten text and the fix-its
/// that actually landed — `apply_try_to` returns `None` for a span that no
/// longer names real source, and such a fix is dropped rather than forced.
fn apply_batch(source: &str, diagnostics: &[Diagnostic]) -> (String, Vec<FixItJson>) {
    let mut working = source.to_string();
    let mut applied = Vec::new();
    for (d, (line, col, end_col), replacement) in select_batch(diagnostics) {
        let Some(rewritten) = d.apply_try_to(&working) else { continue };
        working = rewritten;
        applied.push(FixItJson {
            code: d.code.unwrap_or("E???").to_string(),
            line, col, end_col,
            replacement,
            applicability: d.try_applicability.as_str().to_string(),
        });
    }
    (working, applied)
}

/// Drive the engine to a fixpoint: apply every machine-applicable fix-it,
/// re-check, repeat. Bounded by `MAX_FIX_ROUNDS`.
///
/// The safety rail is the parse-error count. If a round raised it, some
/// fix-it's span or replacement was wrong, and the round is thrown away —
/// the text from before the batch is what the caller gets. A `fix` that
/// leaves the file alone is a bad day; a `fix` that corrupts it is the
/// failure mode this command exists to avoid.
fn run_fix_engine(file: &str, source: &str) -> (String, Vec<FixItJson>) {
    let mut working = source.to_string();
    let mut applied: Vec<FixItJson> = Vec::new();
    let (mut diagnostics, mut parse_errors) = collect_diagnostics(file, &working);
    for _ in 0..MAX_FIX_ROUNDS {
        let (candidate, round) = apply_batch(&working, &diagnostics);
        if round.is_empty() { break; }
        let (next_diagnostics, next_parse_errors) = collect_diagnostics(file, &candidate);
        if next_parse_errors > parse_errors { break; }
        working = candidate;
        applied.extend(round);
        diagnostics = next_diagnostics;
        parse_errors = next_parse_errors;
    }
    (working, applied)
}

/// Dependency package names, for `auto_imports`. Empty outside a project.
fn dependency_names() -> (Vec<String>, std::collections::HashMap<String, String>) {
    if !std::path::Path::new("almide.toml").exists() {
        return (vec![], std::collections::HashMap::new());
    }
    match project::parse_toml(std::path::Path::new("almide.toml")) {
        Ok(proj) => {
            let fetched = project_fetch::fetch_all_deps(&proj).unwrap_or_default();
            let names: Vec<String> = fetched.iter().map(|fd| fd.pkg_id.name.clone()).collect();
            (names, std::collections::HashMap::new())
        }
        Err(_) => (vec![], std::collections::HashMap::new()),
    }
}

/// The hand-maintained AST-level fix family: `auto_imports` (owns the
/// import block, shared with `almide fmt`) and the comparison-call →
/// operator rewrite (needs both argument sub-trees plus a precedence
/// decision, which no single span can state). Neither can be expressed as
/// a `(span, replacement)` pair, which is exactly why they did not migrate.
///
/// The whole file is re-rendered from the AST, so on a parse-error file
/// this must not run at all: that AST is the recovery result and the
/// regions the parser dropped would be silently DELETED by the write
/// (#1077). Returns `source` untouched in that case.
fn apply_ast_family(file: &str, source: &str) -> (String, Vec<String>, usize) {
    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens).with_file(file);
    let Ok(mut program) = parser.parse() else { return (source.to_string(), Vec::new(), 0) };
    if !parser.errors.is_empty() { return (source.to_string(), Vec::new(), 0); }

    let (dep_names, dep_submodules) = dependency_names();
    // Auto-imports: adds missing `import json` / `import fs` / etc.
    let import_messages = auto_imports(&mut program, source, &dep_names, &dep_submodules);
    // AST-level rewrite: `int.gt(a, b)` / `.lt` / `.eq` / `.neq` / `.le` /
    // `.ge` etc. (on int/float/string/bool) → the corresponding operator.
    // Almide never defined these comparison functions; LLMs reach for them
    // from Go-ish / Java-ish training data. Mechanically substituting to
    // `a > b` etc. turns the error case into working code.
    let operator_count = rewrite_comparison_calls(&mut program);
    // The rewrite can land INSIDE a `${…}` interpolation hole, and a string
    // literal reprints its own source text verbatim (#1263) — which would
    // reprint the pre-rewrite hole and silently drop the fix. Dropping the
    // cached spellings makes fmt render those literals from their values.
    if operator_count > 0 {
        ast::strip_literal_raw(&mut program);
    }
    if import_messages.is_empty() && operator_count == 0 {
        // Nothing changed — keep the original text verbatim so the other
        // fixes don't reformat things they shouldn't.
        return (source.to_string(), import_messages, operator_count);
    }
    (format_program(&program), import_messages, operator_count)
}

pub fn cmd_fix(file: &str, dry_run: bool, json: bool) {
    let (_, disk_source, _) = parse_file(file);

    // The engine runs FIRST, on the raw text, so everything after it —
    // auto-imports, the comparison-call rewrite, the formatter — sees a
    // program whose mechanically-stated errors are already gone instead of
    // tripping over them (E031's retired ranges used to be the special case
    // hard-coded here; now it is just one of the codes that states a fix).
    let (source_text, applied) = run_fix_engine(file, &disk_source);

    let (mut working, import_messages, operator_count) = apply_ast_family(file, &source_text);

    // `return` removal stays a text rule (see the module header): deleting
    // the keyword only preserves meaning in TAIL position, and neither the
    // parser nor the checker can establish that from the token alone — so
    // no fix-it may claim to be machine-applicable for it.
    let return_count = RETURN_REMOVAL.apply(&mut working);

    let any_change = !applied.is_empty()
        || !import_messages.is_empty()
        || operator_count > 0
        || return_count > 0;

    // Extract "Added `import X`" → bare module names for JSON.
    let imports_added: Vec<String> = import_messages.iter()
        .filter_map(|m| m.strip_prefix("Added `import ").and_then(|s| s.strip_suffix('`')))
        .map(String::from)
        .collect();

    let (manual, suggestions) = collect_residual_fixes(file, &working);

    if !dry_run && any_change {
        if let Err(e) = std::fs::write(file, &working) {
            err(&format!("error: failed to write {}: {}", file, e));
            std::process::exit(1);
        }
    }

    report_fix_result(FixOutcome {
        file, working: &working, import_messages: &import_messages, imports_added,
        operator_count, return_count, applied, suggestions, manual, any_change,
    }, dry_run, json);
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
/// reported by parser diagnostics, matched by MESSAGE.
///
/// This is the shape #1312 replaced. The last rule standing is `return`
/// removal, and it stays here on purpose: deleting `return` preserves
/// meaning only in TAIL position, which the parser cannot establish from
/// the token alone, so no fix-it may honestly claim `MachineApplicable`
/// for it. Its sibling — the OCaml-style `let ... in` — moved out to a
/// span-anchored fix-it on E049 (the `in` keyword does not exist in
/// Almide, so deleting it is a pure re-spelling).
///
/// Iterates to fixpoint because parser recovery surfaces only the first
/// occurrence per pass. `max_iter` caps runaway in case the rule becomes
/// pathological (e.g. the same position keeps being detected post-edit).
struct KeywordRemoval {
    keyword: &'static str,
    /// Predicate over diagnostic messages identifying the rule's trigger.
    diag_matches: fn(&str) -> bool,
    max_iter: usize,
}

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

/// Re-parse + type-check the FIXED text and split what is left over:
///
/// - `manual_pending` — errors carrying a `try:` snippet the engine did
///   not apply. Same population as before #1312, minus anything the engine
///   handles, plus the applicability that explains why it was left alone.
/// - `suggestions` — the subset that is nonetheless span-anchored. These
///   are exactly the fixes an IDE or a model can apply with one keystroke
///   once a human has agreed to the reading.
fn collect_residual_fixes(file: &str, source: &str) -> (Vec<ManualDiag>, Vec<FixItJson>) {
    use almide::check::Checker;
    use almide::canonicalize;
    use almide::diagnostic;

    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens).with_file(file);
    let mut prog = match parser.parse() {
        Ok(p) => p,
        Err(_) => return (Vec::new(), Vec::new()),
    };
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.set_source(file, source);
    checker.diagnostics = canon.diagnostics;
    let diagnostics = checker.infer_program(&mut prog);

    let residual: Vec<&Diagnostic> = diagnostics.iter()
        .chain(parser.errors.iter())
        .filter(|d| d.level == diagnostic::Level::Error && d.try_snippet.is_some())
        // A machine-applicable fix-it that survived the engine (its batch
        // was discarded, or the round cap ran out) is not "manual" work —
        // re-running `almide fix` is what closes it.
        .filter(|d| d.machine_fix().is_none())
        .collect();

    let manual = residual.iter().map(|d| ManualDiag {
        code: d.code.unwrap_or("E???").to_string(),
        line: d.line,
        col: d.col,
        message: d.message.clone(),
        applicability: d.try_applicability.as_str().to_string(),
    }).collect();

    let suggestions = residual.iter().filter_map(|d| {
        let (line, col, end_col) = d.try_replace_span?;
        Some(FixItJson {
            code: d.code.unwrap_or("E???").to_string(),
            line, col, end_col,
            replacement: d.try_snippet.clone().unwrap_or_default(),
            applicability: d.try_applicability.as_str().to_string(),
        })
    }).collect();

    (manual, suggestions)
}

#[cfg(test)]
mod batching_tests {
    use super::*;

    #[test]
    fn overlapping_ranges_conflict_disjoint_ones_do_not() {
        // Same line, overlapping halves.
        assert!(spans_overlap((3, 5, 9), (3, 7, 12)));
        // Same line, adjacent but disjoint: `[5,7)` then `[7,9)`.
        assert!(!spans_overlap((3, 5, 7), (3, 7, 9)));
        // Different lines never conflict — every replacement span is
        // single-line, so a line number is a partition.
        assert!(!spans_overlap((3, 5, 9), (4, 5, 9)));
    }

    #[test]
    fn insertions_conflict_only_where_they_land() {
        // A zero-width insert inside another fix's range: applying both
        // would splice text into bytes the other fix is replacing.
        assert!(spans_overlap((1, 7, 7), (1, 5, 9)));
        assert!(spans_overlap((1, 5, 9), (1, 7, 7)));
        // Two inserts at the same point are the same edit position.
        assert!(spans_overlap((1, 7, 7), (1, 7, 7)));
        // An insert at the exclusive end of a range is past it.
        assert!(!spans_overlap((1, 9, 9), (1, 5, 9)));
    }

    #[test]
    fn a_batch_keeps_one_fix_per_conflicting_region_in_descending_order() {
        // Three fix-its, two of which overlap: the batch takes the later
        // one (spans are visited descending so earlier offsets stay valid)
        // and leaves the loser for the next round.
        let diags = vec![
            Diagnostic::error("a", "", "").with_machine_fix(2, 5, 9, "AAA"),
            Diagnostic::error("b", "", "").with_machine_fix(2, 7, 12, "BBB"),
            Diagnostic::error("c", "", "").with_machine_fix(5, 1, 3, "CCC"),
        ];
        let batch = select_batch(&diags);
        let spans: Vec<(usize, usize, usize)> = batch.iter().map(|(_, s, _)| *s).collect();
        assert_eq!(spans, vec![(5, 1, 3), (2, 7, 12)]);
    }
}
