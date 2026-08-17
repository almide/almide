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

use crate::oracle::{run_ladder, Finding, FindingKind, Outcome, ReferenceOracle, Toolchain};

/// Cap on minimization rounds, so a stubborn input cannot stall the
/// campaign. Each round is one full statement+expression sweep.
const MAX_ROUNDS: u32 = 8;

/// The minimized program AND the evidence *it* produced.
///
/// The two must travel together. The artifact writer used to pair the
/// minimized `repro.almd` with the evidence of the PRE-minimization
/// program, so `native.out` / `wasm.out` described a program the triager
/// could not see — the six-line output of an original next to a two-line
/// repro. Reading it top-to-bottom leads to conclusions about the repro
/// that its own run does not support. Minimization re-runs the ladder on
/// every accepted candidate anyway; keeping the last one costs nothing
/// and makes the three files one consistent observation.
pub struct Minimized {
    pub source: String,
    /// `None` only when nothing shrank — then the caller's own finding
    /// already describes this exact source.
    pub finding: Option<Finding>,
}

/// Minimize `source` (which triggers `target_kind`) to a smaller program
/// that still triggers the same kind. If nothing shrinks, returns the
/// original with no evidence (the caller's is already correct for it).
pub fn minimize(
    tc: &Toolchain,
    source: &str,
    target_kind: FindingKind,
    work_dir: &Path,
    reference: Option<&dyn ReferenceOracle>,
) -> Minimized {
    // Parse once; if the source does not parse (shouldn't happen for a
    // finding past the check rung, except fmt-instability), return as-is.
    let Some(mut program) = parse(source) else {
        return Minimized { source: source.to_string(), finding: None };
    };

    // Every shrink below mutates the tree and re-renders it. A literal that
    // still carries its source spelling reprints verbatim (#1263), so a shrink
    // reaching inside a `${…}` hole would be silently undone — the minimizer
    // would loop believing it made progress. Render from values instead.
    almide::ast::strip_literal_raw(&mut program);

    let mut best = format_program(&program);
    let mut best_finding = None;

    for _ in 0..MAX_ROUNDS {
        let before = best.clone();

        // Pass 1: try removing each top-level statement.
        program =
            shrink_statements(tc, program, target_kind, work_dir, reference, &mut best, &mut best_finding);

        // Pass 2: try simplifying expressions to minimal leaves.
        program =
            shrink_expressions(tc, program, target_kind, work_dir, reference, &mut best, &mut best_finding);

        // Fixed point: no change this round ⇒ done.
        if best == before {
            break;
        }
    }

    Minimized { source: best, finding: best_finding }
}

/// Try deleting each top-level statement of every `fn` body; keep a
/// deletion only if the finding still reproduces.
fn shrink_statements(
    tc: &Toolchain,
    mut program: Program,
    target_kind: FindingKind,
    work_dir: &Path,
    reference: Option<&dyn ReferenceOracle>,
    best: &mut String,
    best_finding: &mut Option<Finding>,
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
            if let Some(f) = reproduces(tc, &src, target_kind, work_dir, reference) {
                program = candidate;
                *best = src;
                *best_finding = Some(f);
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
    reference: Option<&dyn ReferenceOracle>,
    best: &mut String,
    best_finding: &mut Option<Finding>,
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
            if let Some(f) = reproduces(tc, &src, target_kind, work_dir, reference) {
                program = candidate;
                *best = src;
                *best_finding = Some(f);
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

/// Does `src` still trigger `target_kind` at the ladder? Returns THIS
/// candidate's own finding (evidence included) so an accepted shrink can
/// carry its evidence forward. Generator rejects and clean runs both
/// count as "no longer reproduces".
fn reproduces(
    tc: &Toolchain,
    src: &str,
    target_kind: FindingKind,
    work_dir: &Path,
    reference: Option<&dyn ReferenceOracle>,
) -> Option<Finding> {
    let file = work_dir.join("min_candidate.almd");
    let wasm = work_dir.join("min_candidate.wasm");
    std::fs::write(&file, src).ok()?;
    // The reference oracle MUST be the same one the campaign ran with.
    // Passing `None` here silently disabled minimization for every finding
    // the interpreter rung produces (`both targets disagree with reference
    // interpreter`): without that rung a native==wasm candidate yields no
    // finding at all, so no shrink is ever accepted and the artifact keeps
    // the full unshrunk mutant.
    // The by-construction oracle travels IN the source (`// @expect` lines),
    // so a shrink candidate carries its own expected output — no separate
    // bookkeeping, and a hand-edited repro is judged the same way.
    let expected = crate::generator::identity::expected_from_source(src);
    match run_ladder(tc, src, &file, &wasm, reference, expected.as_deref()) {
        Outcome::Finding(f) if f.kind == target_kind => Some(f),
        _ => None,
    }
}

/// Minimize an IDENTITY-family finding (#1332) by shrinking its **plan**,
/// not its text.
///
/// Text-level delta debugging is unsound here: almost every line of an
/// identity program is one half of an inverse pair, so deleting a line
/// changes the value the program is supposed to print. The candidate would
/// then "still reproduce" for a reason that has nothing to do with the
/// bug, and the minimizer would happily shrink a real miscompile into a
/// generator artifact. Shrinking the plan instead means every candidate is
/// re-rendered by the same construction as the original and re-derives its
/// own expected output, so the oracle survives minimization.
pub fn minimize_plan(
    tc: &Toolchain,
    plan: &crate::generator::identity::Plan,
    target_kind: FindingKind,
    work_dir: &Path,
    reference: Option<&dyn ReferenceOracle>,
) -> Minimized {
    use crate::generator::identity;

    let mut current = plan.clone();
    let mut best = identity::render(&current).0;
    let mut best_finding = None;

    for _ in 0..MAX_ROUNDS {
        let mut shrank = false;
        for candidate in identity::shrink(&current) {
            let (src, _) = identity::render(&candidate);
            if let Some(f) = reproduces(tc, &src, target_kind, work_dir, reference) {
                current = candidate;
                best = src;
                best_finding = Some(f);
                shrank = true;
                break;
            }
        }
        if !shrank {
            break;
        }
    }

    Minimized { source: best, finding: best_finding }
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
