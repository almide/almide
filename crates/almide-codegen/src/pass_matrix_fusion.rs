//! MatrixFusionPass: fuse `matrix.add(matrix.scale(a, ka), matrix.scale(b, kb))`
//! into `matrix.fma(a, ka, b, kb)`.
//!
//! Closes the elementwise-chain performance gap measured in
//! `almide-wasm-bindgen/examples/bench/chain_bench.mjs` — without this pass,
//! the chain runs 3 separate elementwise loops with 3 allocations.
//!
//! Target: all targets that have `matrix.fma` available (Rust + WASM today).
//! Runs early, before passes that decorate the IR with borrow / clone /
//! match-subject transforms (those break naive structural matching).

use almide_base::intern::sym;
use almide_ir::*;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct MatrixFusionPass;

impl NanoPass for MatrixFusionPass {
    fn name(&self) -> &str { "MatrixFusion" }
    fn targets(&self) -> Option<Vec<Target>> { None } // all targets
    fn depends_on(&self) -> Vec<&'static str> { vec![] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            if rewrite_expr(&mut func.body) { changed = true; }
        }
        for tl in &mut program.top_lets {
            if rewrite_expr(&mut tl.value) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if rewrite_expr(&mut func.body) { changed = true; }
            }
            for tl in &mut module.top_lets {
                if rewrite_expr(&mut tl.value) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

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

fn rewrite_expr(expr: &mut IrExpr) -> bool {
    let mut changed = false;

    // Recurse into children first so deeper chains fuse before the outer add.
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for stmt in stmts.iter_mut() { if rewrite_stmt(stmt) { changed = true; } }
            if let Some(e) = tail { if rewrite_expr(e) { changed = true; } }
        }
        IrExprKind::If { cond, then, else_ } => {
            if rewrite_expr(cond) { changed = true; }
            if rewrite_expr(then) { changed = true; }
            if rewrite_expr(else_) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if rewrite_expr(subject) { changed = true; }
            for arm in arms {
                if let Some(g) = &mut arm.guard { if rewrite_expr(g) { changed = true; } }
                if rewrite_expr(&mut arm.body) { changed = true; }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            if rewrite_expr(iterable) { changed = true; }
            for stmt in body.iter_mut() { if rewrite_stmt(stmt) { changed = true; } }
        }
        IrExprKind::While { cond, body } => {
            if rewrite_expr(cond) { changed = true; }
            for stmt in body.iter_mut() { if rewrite_stmt(stmt) { changed = true; } }
        }
        IrExprKind::Lambda { body, .. } => {
            if rewrite_expr(body) { changed = true; }
        }
        IrExprKind::Call { args, .. } => {
            for a in args.iter_mut() { if rewrite_expr(a) { changed = true; } }
        }
        IrExprKind::BinOp { left, right, .. } => {
            if rewrite_expr(left) { changed = true; }
            if rewrite_expr(right) { changed = true; }
        }
        IrExprKind::UnOp { operand, .. } => {
            if rewrite_expr(operand) { changed = true; }
        }
        _ => {}
    }

    // Detect:
    //   matrix.add(X, Y) → fma(extract_scaled(X), extract_scaled(Y))
    //   matrix.sub(X, Y) → fma(extract_scaled(X), extract_scaled(Y) with negated kb)
    // Where extract_scaled(scale(m, k)) = (m, k), neg(m) = (m, -1.0), m = (m, 1.0).
    //
    // Only fuse when at least ONE side is genuinely scaled — otherwise we'd
    // turn a plain `matrix.add(a, b)` into `fma(a, 1.0, b, 1.0)`, which is
    // strictly more expensive (2 muls per element for no payoff).
    let fused = if let IrExprKind::Call {
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
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if let Some((a, ka, b, kb)) = fused {
        let new_kind = IrExprKind::Call {
            target: CallTarget::Module { module: sym("matrix"), func: sym("fma") },
            args: vec![a, ka, b, kb],
            type_args: vec![],
        };
        expr.kind = new_kind;
        // expr.ty stays the same (Matrix).
        changed = true;
    }

    // Tree-fuse: collapse `fma(X, kx, fma(b, kb, c, kc), ky)` into
    // `fma3(X, kx, b, kb*ky, c, kc*ky)`. Algebraically:
    //   X*kx + (b*kb + c*kc)*ky = X*kx + b*(kb*ky) + c*(kc*ky)
    // This turns a 2-pass chain (inner fma + outer fma) into a single
    // 1-pass sweep — the structural fix for the nested-fma 2-pass bottleneck.
    // Mirror case handled: `fma(fma(a, ka, b, kb), kx, Y, ky)` collapses to
    //   a*(ka*kx) + b*(kb*kx) + Y*ky  →  fma3(a, ka*kx, b, kb*kx, Y, ky).
    if let Some(new_kind) = try_tree_fuse(&expr.kind) {
        expr.kind = new_kind;
        changed = true;
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

    // Case 1: right side is fma → fma(a, ka, fma(b', kb', c', kc'), kb)
    if let Some((bp, kbp, cp, kcp)) = match_fma(&b) {
        return Some(build_fma3(
            a, ka,
            bp, mul_scalar(kbp, kb.clone()),
            cp, mul_scalar(kcp, kb),
        ));
    }
    // Case 2: left side is fma → fma(fma(a', ka', b', kb'), ka, c, kb)
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

fn rewrite_stmt(stmt: &mut IrStmt) -> bool {
    let mut changed = false;
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. }
        | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. }
        | IrStmtKind::FieldAssign { value, .. } => {
            if rewrite_expr(value) { changed = true; }
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            if rewrite_expr(index) { changed = true; }
            if rewrite_expr(value) { changed = true; }
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            if rewrite_expr(key) { changed = true; }
            if rewrite_expr(value) { changed = true; }
        }
        IrStmtKind::Guard { cond, else_ } => {
            if rewrite_expr(cond) { changed = true; }
            if rewrite_expr(else_) { changed = true; }
        }
        IrStmtKind::Expr { expr } => {
            if rewrite_expr(expr) { changed = true; }
        }
        _ => {}
    }
    changed
}
