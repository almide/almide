//! Delta-debugging minimizer.
//!
//! Given a program that triggers a finding, shrink it to a minimal
//! source that still reproduces *the same finding kind*. We work on the
//! parsed AST so every candidate is structurally well-formed, and we
//! re-run the ladder on each candidate, keeping it only if the finding
//! reproduces.
//!
//! Two passes, coarse→fine:
//!   1. **Statement removal** — drop top-level `let`/`println` statements
//!      from `fn main`'s body (and any unused bindings they leave).
//!   2. **Expression simplification** — replace a subexpression with a
//!      minimal literal of a compatible shape, collapsing calls/`if`/
//!      constructions down to leaves.
//!
//! The result is a small, human-readable repro — the artifact that lands
//! in `findings/`.

use std::path::Path;

use almide::ast::{Decl, Expr, ExprKind, Program};
use almide::fmt::format_program;

use crate::oracle::{run_ladder, FindingKind, Outcome, Toolchain};

/// Cap on minimization rounds, so a stubborn input cannot stall the
/// campaign. Each round is one full statement+expression sweep.
const MAX_ROUNDS: u32 = 8;

/// Minimize `source` (which triggers `target_kind`) to a smaller program
/// that still triggers the same kind. Returns the minimized source. If
/// nothing shrinks, returns the original.
pub fn minimize(
    tc: &Toolchain,
    source: &str,
    target_kind: FindingKind,
    work_dir: &Path,
) -> String {
    // Parse once; if the source does not parse (shouldn't happen for a
    // finding past the check rung, except fmt-instability), return as-is.
    let Some(mut program) = parse(source) else {
        return source.to_string();
    };

    let mut best = format_program(&program);

    for _ in 0..MAX_ROUNDS {
        let before = best.clone();

        // Pass 1: try removing each top-level statement.
        program = shrink_statements(tc, program, target_kind, work_dir, &mut best);

        // Pass 2: try simplifying expressions to minimal leaves.
        program = shrink_expressions(tc, program, target_kind, work_dir, &mut best);

        // Fixed point: no change this round ⇒ done.
        if best == before {
            break;
        }
    }

    best
}

/// Try deleting each top-level statement of every `fn` body; keep a
/// deletion only if the finding still reproduces.
fn shrink_statements(
    tc: &Toolchain,
    mut program: Program,
    target_kind: FindingKind,
    work_dir: &Path,
    best: &mut String,
) -> Program {
    // We repeatedly attempt to remove a statement at a given (fn, index)
    // position. After a successful removal, indices shift, so we restart
    // the scan — bounded by the shrinking statement count.
    loop {
        let positions = top_level_stmt_positions(&program);
        let mut removed_any = false;

        for (fn_idx, stmt_idx) in positions {
            let mut candidate = program.clone();
            if !remove_stmt(&mut candidate, fn_idx, stmt_idx) {
                continue;
            }
            let src = format_program(&candidate);
            if reproduces(tc, &src, target_kind, work_dir) {
                program = candidate;
                *best = src;
                removed_any = true;
                break; // restart scan with the smaller program
            }
        }

        if !removed_any {
            break;
        }
    }
    program
}

/// Try replacing each expression with a minimal leaf; keep a
/// simplification only if the finding still reproduces.
fn shrink_expressions(
    tc: &Toolchain,
    mut program: Program,
    target_kind: FindingKind,
    work_dir: &Path,
    best: &mut String,
) -> Program {
    loop {
        let count = count_simplifiable(&program);
        let mut simplified_any = false;

        for target in 0..count {
            let mut candidate = program.clone();
            if !simplify_nth(&mut candidate, target) {
                continue;
            }
            let src = format_program(&candidate);
            if reproduces(tc, &src, target_kind, work_dir) {
                program = candidate;
                *best = src;
                simplified_any = true;
                break;
            }
        }

        if !simplified_any {
            break;
        }
    }
    program
}

/// Does `src` still trigger `target_kind` at the ladder? Generator
/// rejects and clean runs both count as "no longer reproduces".
fn reproduces(tc: &Toolchain, src: &str, target_kind: FindingKind, work_dir: &Path) -> bool {
    let file = work_dir.join("min_candidate.almd");
    let wasm = work_dir.join("min_candidate.wasm");
    if std::fs::write(&file, src).is_err() {
        return false;
    }
    match run_ladder(tc, src, &file, &wasm, None) {
        Outcome::Finding(f) => f.kind == target_kind,
        _ => false,
    }
}

// ── AST surgery helpers ──

fn parse(src: &str) -> Option<Program> {
    let tokens = almide::lexer::Lexer::tokenize(src);
    let mut parser = almide::parser::Parser::new(tokens);
    parser.parse().ok()
}

/// All `(fn_decl_index, body_stmt_index)` positions of top-level
/// statements in fn bodies.
fn top_level_stmt_positions(program: &Program) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (di, decl) in program.decls.iter().enumerate() {
        if let Decl::Fn { body: Some(expr), .. } = decl {
            if let ExprKind::Block { stmts, .. } = &expr.kind {
                for si in 0..stmts.len() {
                    out.push((di, si));
                }
            }
        }
    }
    out
}

/// Remove the statement at `(fn_idx, stmt_idx)`. Returns `false` if the
/// position is no longer valid.
fn remove_stmt(program: &mut Program, fn_idx: usize, stmt_idx: usize) -> bool {
    let Some(Decl::Fn { body: Some(expr), .. }) = program.decls.get_mut(fn_idx) else {
        return false;
    };
    let ExprKind::Block { stmts, .. } = &mut expr.kind else {
        return false;
    };
    if stmt_idx >= stmts.len() {
        return false;
    }
    stmts.remove(stmt_idx);
    true
}

/// Count expressions that can be simplified to a leaf (non-trivial
/// shapes: calls, ifs, binaries, lists, etc.).
fn count_simplifiable(program: &Program) -> usize {
    let mut n = 0;
    for decl in &program.decls {
        if let Decl::Fn { body: Some(expr), .. } = decl {
            count_simplifiable_expr(expr, &mut n);
        }
    }
    n
}

fn count_simplifiable_expr(expr: &Expr, n: &mut usize) {
    if is_simplifiable(&expr.kind) {
        *n += 1;
    }
    for child in child_exprs(expr) {
        count_simplifiable_expr(child, n);
    }
}

/// Replace the `target`-th simplifiable expression (pre-order) with a
/// minimal leaf of a plausible type. Returns `false` if not found.
fn simplify_nth(program: &mut Program, target: usize) -> bool {
    let mut counter = 0usize;
    let mut done = false;
    for decl in program.decls.iter_mut() {
        if done {
            break;
        }
        if let Decl::Fn { body: Some(expr), .. } = decl {
            simplify_expr(expr, target, &mut counter, &mut done);
        }
    }
    done
}

fn simplify_expr(expr: &mut Expr, target: usize, counter: &mut usize, done: &mut bool) {
    if *done {
        return;
    }
    if is_simplifiable(&expr.kind) {
        if *counter == target {
            // Collapse to a minimal Int literal — a leaf that re-parses
            // and keeps fmt happy. If the original drove a string/float
            // divergence the statement annotation still pins the type;
            // when the collapse breaks typing, `reproduces` rejects it
            // and we move on, so an over-eager collapse is self-correcting.
            expr.kind = ExprKind::Int {
                value: serde_json::Value::from(0),
                raw: "0".to_string(),
            };
            *done = true;
            return;
        }
        *counter += 1;
    }
    for child in child_exprs_mut(expr) {
        simplify_expr(child, target, counter, done);
        if *done {
            return;
        }
    }
}

/// Whether an expression node is worth attempting to collapse.
fn is_simplifiable(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::Call { .. }
            | ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Pipe { .. }
            | ExprKind::List { .. }
            | ExprKind::InterpolatedString { .. }
    )
}

/// Immediate child expressions of `expr` (for the recursion). Covers the
/// shapes the generator produces; exhaustive coverage is unnecessary
/// because unvisited children simply are not minimized.
fn child_exprs(expr: &Expr) -> Vec<&Expr> {
    let mut out: Vec<&Expr> = Vec::new();
    match &expr.kind {
        ExprKind::Call { callee, args, .. } => {
            out.push(callee);
            out.extend(args.iter());
        }
        ExprKind::If { cond, then, else_ } => {
            out.push(cond);
            out.push(then);
            out.push(else_);
        }
        ExprKind::Binary { left, right, .. } => {
            out.push(left);
            out.push(right);
        }
        ExprKind::Pipe { left, right } => {
            out.push(left);
            out.push(right);
        }
        ExprKind::List { elements } => out.extend(elements.iter()),
        ExprKind::Paren { expr } | ExprKind::Some { expr } | ExprKind::Ok { expr } => {
            out.push(expr)
        }
        ExprKind::Lambda { body, .. } => out.push(body),
        _ => {}
    }
    out
}

fn child_exprs_mut(expr: &mut Expr) -> Vec<&mut Expr> {
    let mut out: Vec<&mut Expr> = Vec::new();
    match &mut expr.kind {
        ExprKind::Call { callee, args, .. } => {
            out.push(callee);
            out.extend(args.iter_mut());
        }
        ExprKind::If { cond, then, else_ } => {
            out.push(cond);
            out.push(then);
            out.push(else_);
        }
        ExprKind::Binary { left, right, .. } => {
            out.push(left);
            out.push(right);
        }
        ExprKind::Pipe { left, right } => {
            out.push(left);
            out.push(right);
        }
        ExprKind::List { elements } => out.extend(elements.iter_mut()),
        ExprKind::Paren { expr } | ExprKind::Some { expr } | ExprKind::Ok { expr } => {
            out.push(expr)
        }
        ExprKind::Lambda { body, .. } => out.push(body),
        _ => {}
    }
    out
}
