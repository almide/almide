//! MatrixFusionPass — declarative chain-fusion driven by `pass_matrix_fusion_rules`
//! plus a pair of legacy rules for `add/sub → fma` and `fma → fma3`
//! tree-fuse that don't fit the pure chain schema (both coefficients
//! need to be composed from nested scale operands).
//!
//! ## Layout
//!
//! - `rewrite_expr` walks the IR. After recursing into children it:
//!   1. Applies the `add/sub → fma` transform (requires `extract_scaled`
//!      plumbing; not expressible as a single-pattern rule).
//!   2. Applies the `fma/fma3` tree-fuse (mutates coefficients; also
//!      outside the rule schema).
//!   3. Applies every entry of `fusion_rules()` in order. Each chain
//!      fusion (gemm/bias/scale/gelu, attention_weights, SDPA, …) lives
//!      as a data-only `FusionRule` in `pass_matrix_fusion_rules` —
//!      adding a new fusion means appending one rule and a runtime
//!      implementation; no matcher boilerplate.
//! - `fuse_let_split_chain` walks a Block / ForIn / While body and
//!   collapses each rule's **let-split form** via the generic
//!   `match_chain_let_window`. The rule's pattern tree determines the
//!   k-gram length automatically (pattern depth).
//!
//! ## Why declarative
//!
//! `pass_matrix_fusion_rules::Pattern` is isomorphic to MLIR's op
//! pattern matcher and egg's `Rewrite<L, N>` LHS. When the MLIR / egg
//! arc lands we translate the rule table mechanically — the fusion
//! catalogue is data, not hand-rolled matchers.

use almide_base::intern::sym;
use almide_ir::*;
use super::pass::{NanoPass, PassResult, Target};
use super::pass_matrix_fusion_rules::{
    self as rules, count_var_refs, match_chain_let_window, ChainStep, FusionRule,
};

#[derive(Debug)]
pub struct MatrixFusionPass;

impl NanoPass for MatrixFusionPass {
    fn name(&self) -> &str { "MatrixFusion" }
    fn targets(&self) -> Option<Vec<Target>> { None } // all targets
    fn depends_on(&self) -> Vec<&'static str> { vec![] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let linearized = linearize_rules();
        let rules_list = rules::fusion_rules();
        let mut changed = false;
        for func in &mut program.functions {
            if rewrite_expr(&mut func.body, &rules_list, &linearized) { changed = true; }
        }
        for tl in &mut program.top_lets {
            if rewrite_expr(&mut tl.value, &rules_list, &linearized) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if rewrite_expr(&mut func.body, &rules_list, &linearized) { changed = true; }
            }
            for tl in &mut module.top_lets {
                if rewrite_expr(&mut tl.value, &rules_list, &linearized) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

/// Pre-compute `(rule, chain_steps)` for every chain-shaped rule.
/// Non-chain-shaped rules (two nested Call children, etc.) skip
/// let-split and are only applied in nested form. Sorted by chain
/// length descending so deeper patterns win on overlap.
fn linearize_rules() -> Vec<(FusionRule, Vec<ChainStep>)> {
    let mut out: Vec<(FusionRule, Vec<ChainStep>)> = rules::fusion_rules()
        .into_iter()
        .filter_map(|r| {
            let steps = r.pattern.linearize_chain()?;
            Some((r, steps))
        })
        .collect();
    out.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    out
}

// ── Legacy add/sub → fma helpers (not expressible as chain rules) ──

/// Returns Some((m, k)) if expr is `matrix.scale(m, k)`, else None.
fn match_matrix_scale(expr: &IrExpr) -> Option<(IrExpr, IrExpr)> {
    if let IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } = &expr.kind {
        if module.as_str() == "matrix" && func.as_str() == "scale" && args.len() == 2 {
            return Some((args[0].clone(), args[1].clone()));
        }
    }
    None
}

/// Decompose any matrix expression into (matrix, coefficient).
///   matrix.scale(m, k) → (m, k)
///   matrix.neg(m)      → (m, -1.0)
///   any other expr     → (expr, 1.0)
/// This lets `add` / `sub` of arbitrary matrix expressions fold into a
/// single `fma` call as long as we can name a coefficient for each side.
fn extract_scaled(expr: &IrExpr) -> (IrExpr, IrExpr) {
    if let Some((m, k)) = match_matrix_scale(expr) {
        return (m, k);
    }
    if let IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } = &expr.kind {
        if module.as_str() == "matrix" && func.as_str() == "neg" && args.len() == 1 {
            return (args[0].clone(), float_lit(-1.0));
        }
    }
    (expr.clone(), float_lit(1.0))
}

fn float_lit(v: f64) -> IrExpr {
    IrExpr {
        kind: IrExprKind::LitFloat { value: v },
        ty: almide_lang::types::Ty::Float,
        span: None,
    }
}

/// Build a `matrix.scale(s, -1.0)` expression to negate a scalar.
fn negate_scalar(s: IrExpr) -> IrExpr {
    // Use BinOp::MulFloat with -1.0 for safety with both Int and Float scalars.
    let ty = s.ty.clone();
    let neg_one = IrExpr {
        kind: IrExprKind::LitFloat { value: -1.0 },
        ty: almide_lang::types::Ty::Float,
        span: None,
    };
    IrExpr {
        kind: IrExprKind::BinOp {
            op: BinOp::MulFloat,
            left: Box::new(s),
            right: Box::new(neg_one),
        },
        ty,
        span: None,
    }
}

fn rewrite_expr(
    expr: &mut IrExpr,
    rules_list: &[FusionRule],
    linearized: &[(FusionRule, Vec<ChainStep>)],
) -> bool {
    let mut changed = false;

    // Recurse into children first so deeper chains fuse before the outer op.
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for stmt in stmts.iter_mut() {
                if rewrite_stmt(stmt, rules_list, linearized) { changed = true; }
            }
            if let Some(e) = tail { if rewrite_expr(e, rules_list, linearized) { changed = true; } }
            if fuse_let_split_chain(stmts, tail.as_deref(), linearized) { changed = true; }
        }
        IrExprKind::If { cond, then, else_ } => {
            if rewrite_expr(cond, rules_list, linearized) { changed = true; }
            if rewrite_expr(then, rules_list, linearized) { changed = true; }
            if rewrite_expr(else_, rules_list, linearized) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if rewrite_expr(subject, rules_list, linearized) { changed = true; }
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    if rewrite_expr(g, rules_list, linearized) { changed = true; }
                }
                if rewrite_expr(&mut arm.body, rules_list, linearized) { changed = true; }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            if rewrite_expr(iterable, rules_list, linearized) { changed = true; }
            for stmt in body.iter_mut() {
                if rewrite_stmt(stmt, rules_list, linearized) { changed = true; }
            }
            if fuse_let_split_chain(body, None, linearized) { changed = true; }
        }
        IrExprKind::While { cond, body } => {
            if rewrite_expr(cond, rules_list, linearized) { changed = true; }
            for stmt in body.iter_mut() {
                if rewrite_stmt(stmt, rules_list, linearized) { changed = true; }
            }
            if fuse_let_split_chain(body, None, linearized) { changed = true; }
        }
        IrExprKind::Lambda { body, .. } => {
            if rewrite_expr(body, rules_list, linearized) { changed = true; }
        }
        IrExprKind::Call { args, .. } => {
            for a in args.iter_mut() {
                if rewrite_expr(a, rules_list, linearized) { changed = true; }
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            if rewrite_expr(left, rules_list, linearized) { changed = true; }
            if rewrite_expr(right, rules_list, linearized) { changed = true; }
        }
        IrExprKind::UnOp { operand, .. } => {
            if rewrite_expr(operand, rules_list, linearized) { changed = true; }
        }
        _ => {}
    }

    // ── Legacy 1: add/sub with at-least-one scaled side → fma ──
    //
    // Only fires when at least ONE side is genuinely scaled; otherwise
    // converting `add(a, b)` into `fma(a, 1.0, b, 1.0)` would add two
    // pointless multiplies per element. This is expressible as two
    // rules in principle but the per-side `extract_scaled` (which also
    // recognises `neg`) and the `-1.0` / `scalar * -1.0` plumbing live
    // below; keeping them inline is cheaper than extending the schema.
    let fused_add = if let IrExprKind::Call {
        target: CallTarget::Module { module, func },
        args,
        ..
    } = &expr.kind {
        if module.as_str() == "matrix" && args.len() == 2 {
            let is_add = func.as_str() == "add";
            let is_sub = func.as_str() == "sub";
            if is_add || is_sub {
                let has_scale_a = match_matrix_scale(&args[0]).is_some()
                    || matches!(&args[0].kind, IrExprKind::Call {
                        target: CallTarget::Module { module, func }, .. }
                        if module.as_str() == "matrix" && func.as_str() == "neg");
                let has_scale_b = match_matrix_scale(&args[1]).is_some()
                    || matches!(&args[1].kind, IrExprKind::Call {
                        target: CallTarget::Module { module, func }, .. }
                        if module.as_str() == "matrix" && func.as_str() == "neg");
                if has_scale_a || has_scale_b {
                    let (a, ka) = extract_scaled(&args[0]);
                    let (b, kb_raw) = extract_scaled(&args[1]);
                    let kb = if is_sub { negate_scalar(kb_raw) } else { kb_raw };
                    Some((a, ka, b, kb))
                } else { None }
            } else { None }
        } else { None }
    } else { None };

    if let Some((a, ka, b, kb)) = fused_add {
        expr.kind = IrExprKind::Call {
            target: CallTarget::Module { module: sym("matrix"), func: sym("fma") },
            args: vec![a, ka, b, kb],
            type_args: vec![],
        };
        changed = true;
    }

    // ── Legacy 2: fma + nested fma → fma3 tree-fuse ──
    //
    // `fma(X, kx, fma(b, kb, c, kc), ky)` → `fma3(X, kx, b, kb*ky, c, kc*ky)`.
    // Coefficient multiplication crosses rule boundaries, so this
    // stays procedural rather than a single Pattern rule.
    if let Some(new_kind) = try_tree_fuse(&expr.kind) {
        expr.kind = new_kind;
        changed = true;
    }

    // ── Declarative fusion rules (chain-shaped) ──
    //
    // Applied top-down after the legacy transforms settle. Order
    // within `fusion_rules()` is deliberate — the table puts deeper /
    // more specific patterns first so outer rules don't pre-empt their
    // inner prerequisites.
    for rule in rules_list {
        if let Some(m) = rule.pattern.try_match(expr) {
            let new_kind = (rule.rewrite)(&m);
            expr.kind = new_kind;
            changed = true;
            // Re-check with the rewritten expr in case a second rule
            // wraps this one (e.g. attention_weights → SDPA).
        }
    }

    changed
}

/// Let-split chain fusion: for every `(rule, chain_steps)` entry,
/// scan the block for a sliding window of `chain_steps.len()`
/// consecutive `Bind` statements that matches the rule's chain shape.
/// When found, rewrite the terminal `Bind` to the fused call and drop
/// the intermediate bindings (after verifying they have no references
/// in the remainder of the block).
fn fuse_let_split_chain(
    stmts: &mut Vec<IrStmt>,
    tail: Option<&IrExpr>,
    linearized: &[(FusionRule, Vec<ChainStep>)],
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let mut fired = false;
        for (rule, steps) in linearized {
            let k = steps.len();
            if i + k > stmts.len() { continue; }
            let window = &stmts[i..i + k];
            let Some(cm) = match_chain_let_window(steps, window) else { continue };
            // Single-use enforcement: every intermediate var must have
            // zero references in the rest of the block (statements
            // after the window + tail expression).
            let rest = &stmts[i + k..];
            if !cm.intermediate_vars
                .iter()
                .all(|v| count_var_refs(rest, tail, *v) == 0)
            {
                continue;
            }
            // Extract the terminal bind's VarId / mutability / ty so
            // the fused statement inherits them exactly.
            let (term_var, term_mut, term_ty) = match &stmts[i + k - 1].kind {
                IrStmtKind::Bind { var, mutability, ty, .. } => (*var, *mutability, ty.clone()),
                _ => continue,
            };
            let span = stmts[i + k - 1].span;
            let fused_kind = (rule.rewrite)(&cm.captures);
            let fused_stmt = IrStmt {
                kind: IrStmtKind::Bind {
                    var: term_var,
                    mutability: term_mut,
                    ty: term_ty.clone(),
                    value: IrExpr { kind: fused_kind, ty: term_ty, span },
                },
                span,
            };
            stmts.splice(i..i + k, std::iter::once(fused_stmt));
            changed = true;
            fired = true;
            break; // re-check same index with the shorter window
        }
        if !fired { i += 1; }
    }
    changed
}

/// If `expr` is `fma(A, kA, B, kB)` where one of A/B is itself an `fma`,
/// collapse into a 3-term `fma3` with multiplied coefficients.
fn try_tree_fuse(kind: &IrExprKind) -> Option<IrExprKind> {
    let (a, ka, b, kb) = match kind {
        IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. }
            if module.as_str() == "matrix" && func.as_str() == "fma" && args.len() == 4 =>
        {
            (args[0].clone(), args[1].clone(), args[2].clone(), args[3].clone())
        }
        _ => return None,
    };
    if let Some((bp, kbp, cp, kcp)) = match_fma(&b) {
        return Some(build_fma3(
            a, ka,
            bp, mul_scalar(kbp, kb.clone()),
            cp, mul_scalar(kcp, kb),
        ));
    }
    if let Some((ap, kap, bp, kbp)) = match_fma(&a) {
        return Some(build_fma3(
            ap, mul_scalar(kap, ka.clone()),
            bp, mul_scalar(kbp, ka),
            b,  kb,
        ));
    }
    None
}

fn match_fma(e: &IrExpr) -> Option<(IrExpr, IrExpr, IrExpr, IrExpr)> {
    if let IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } = &e.kind {
        if module.as_str() == "matrix" && func.as_str() == "fma" && args.len() == 4 {
            return Some((args[0].clone(), args[1].clone(), args[2].clone(), args[3].clone()));
        }
    }
    None
}

fn build_fma3(a: IrExpr, ka: IrExpr, b: IrExpr, kb: IrExpr, c: IrExpr, kc: IrExpr) -> IrExprKind {
    IrExprKind::Call {
        target: CallTarget::Module { module: sym("matrix"), func: sym("fma3") },
        args: vec![a, ka, b, kb, c, kc],
        type_args: vec![],
    }
}

/// Multiply two scalar exprs. If both are `LitFloat`, fold at compile time;
/// otherwise emit `BinOp::MulFloat`. The later ConstFoldPass also handles the
/// literal case but we do it here too for readable IR dumps.
fn mul_scalar(x: IrExpr, y: IrExpr) -> IrExpr {
    if let (IrExprKind::LitFloat { value: a }, IrExprKind::LitFloat { value: b })
        = (&x.kind, &y.kind) {
        return IrExpr {
            kind: IrExprKind::LitFloat { value: a * b },
            ty: almide_lang::types::Ty::Float,
            span: None,
        };
    }
    IrExpr {
        kind: IrExprKind::BinOp {
            op: BinOp::MulFloat,
            left: Box::new(x),
            right: Box::new(y),
        },
        ty: almide_lang::types::Ty::Float,
        span: None,
    }
}

fn rewrite_stmt(
    stmt: &mut IrStmt,
    rules_list: &[FusionRule],
    linearized: &[(FusionRule, Vec<ChainStep>)],
) -> bool {
    let mut changed = false;
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. }
        | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. }
        | IrStmtKind::FieldAssign { value, .. } => {
            if rewrite_expr(value, rules_list, linearized) { changed = true; }
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            if rewrite_expr(index, rules_list, linearized) { changed = true; }
            if rewrite_expr(value, rules_list, linearized) { changed = true; }
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            if rewrite_expr(key, rules_list, linearized) { changed = true; }
            if rewrite_expr(value, rules_list, linearized) { changed = true; }
        }
        IrStmtKind::Guard { cond, else_ } => {
            if rewrite_expr(cond, rules_list, linearized) { changed = true; }
            if rewrite_expr(else_, rules_list, linearized) { changed = true; }
        }
        IrStmtKind::Expr { expr } => {
            if rewrite_expr(expr, rules_list, linearized) { changed = true; }
        }
        _ => {}
    }
    changed
}
