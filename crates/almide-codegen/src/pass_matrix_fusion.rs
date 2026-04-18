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
use almide_ir::visit::{IrVisitor, walk_expr};
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
            // Collapse let-split gemm-bias-scale-gelu chains inside this
            // block. Runs after the per-stmt rewrites so every inner
            // expression is already in its fused-or-final form.
            if fuse_let_split_chain(stmts, tail.as_deref()) { changed = true; }
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
            if fuse_let_split_chain(body, None) { changed = true; }
        }
        IrExprKind::While { cond, body } => {
            if rewrite_expr(cond) { changed = true; }
            for stmt in body.iter_mut() { if rewrite_stmt(stmt) { changed = true; } }
            if fuse_let_split_chain(body, None) { changed = true; }
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

    // Linear-layer chain fusion: `gelu(scale(add(mul(a, b), bias), alpha))`
    // → `fused_gemm_bias_scale_gelu(a, b, bias, alpha)`. Runs after the
    // add/fma fusion above so we match on the original unfused form, not
    // on a surviving `fma` outer.
    if let Some(new_kind) = try_fuse_gemm_bias_scale_gelu(&expr.kind) {
        expr.kind = new_kind;
        changed = true;
    }

    changed
}

/// Returns `(a, b)` if `expr` is `matrix.<func>(a, b)`.
fn match_matrix_binary<'a>(expr: &'a IrExpr, func_name: &str) -> Option<(&'a IrExpr, &'a IrExpr)> {
    if let IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } = &expr.kind {
        if module.as_str() == "matrix" && func.as_str() == func_name && args.len() == 2 {
            return Some((&args[0], &args[1]));
        }
    }
    None
}

/// Returns `m` if `expr` is `matrix.<func>(m)`.
fn match_matrix_unary<'a>(expr: &'a IrExpr, func_name: &str) -> Option<&'a IrExpr> {
    if let IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } = &expr.kind {
        if module.as_str() == "matrix" && func.as_str() == func_name && args.len() == 1 {
            return Some(&args[0]);
        }
    }
    None
}

/// Pattern-match `gelu(scale(add(mul(a, b), bias), alpha))` and rewrite
/// the whole 4-layer chain into a single `fused_gemm_bias_scale_gelu`
/// call. The fused runtime implementation does `alpha*(a@b + bias)` via
/// one cblas_dgemm (`C = bias`, `β = alpha`, `α = alpha`) and then runs
/// GELU in place — collapses 3 intermediate allocations and 3 loop
/// sweeps into one BLAS call plus one pass.
fn try_fuse_gemm_bias_scale_gelu(kind: &IrExprKind) -> Option<IrExprKind> {
    let gelu_arg = match kind {
        IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. }
            if module.as_str() == "matrix" && func.as_str() == "gelu" && args.len() == 1 =>
        {
            &args[0]
        }
        _ => return None,
    };
    let (scale_inner, alpha) = match_matrix_binary(gelu_arg, "scale")?;
    let (add_lhs, bias) = match_matrix_binary(scale_inner, "add")?;
    let (a, b) = match_matrix_binary(add_lhs, "mul")?;

    Some(IrExprKind::Call {
        target: CallTarget::Module {
            module: sym("matrix"),
            func: sym("fused_gemm_bias_scale_gelu"),
        },
        args: vec![a.clone(), b.clone(), bias.clone(), alpha.clone()],
        type_args: vec![],
    })
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

/// Let-split chain fusion: collapse
///   let c = matrix.mul(a, b);
///   let d = matrix.add(c, bias);
///   let e = matrix.scale(d, alpha);
///   let f = matrix.gelu(e);
/// into a single
///   let f = matrix.fused_gemm_bias_scale_gelu(a, b, bias, alpha);
///
/// Safety conditions enforced:
/// 1. The four bindings must appear in sequence, each a single `Bind`.
/// 2. Each intermediate var (c, d, e) must flow directly into the next op
///    — no interposed uses or transforms.
/// 3. c, d, and e must have **zero** references in the remainder of the
///    block (subsequent stmts + tail). This is conservative (some valid
///    fusions get skipped) but keeps us from dropping values a later
///    line still needs.
///
/// Without this pass the nested-expression form is the only way to opt
/// into fusion, which is unnatural for human-written code that tends to
/// name intermediate stages. With it, both styles compile to the same
/// single-BLAS-call path.
fn fuse_let_split_chain(stmts: &mut Vec<IrStmt>, tail: Option<&IrExpr>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i + 4 <= stmts.len() {
        if let Some(m) = match_chain_4gram(&stmts[i..i + 4]) {
            let rest = &stmts[i + 4..];
            let c_out = count_refs_in_stmts_and_tail(rest, tail, m.c_id);
            let d_out = count_refs_in_stmts_and_tail(rest, tail, m.d_id);
            let e_out = count_refs_in_stmts_and_tail(rest, tail, m.e_id);
            if c_out == 0 && d_out == 0 && e_out == 0 {
                let span = stmts[i + 3].span;
                let fused_value = IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: sym("matrix"),
                            func: sym("fused_gemm_bias_scale_gelu"),
                        },
                        args: vec![m.a, m.b, m.bias, m.alpha],
                        type_args: vec![],
                    },
                    ty: m.f_ty.clone(),
                    span,
                };
                let new_bind = IrStmt {
                    kind: IrStmtKind::Bind {
                        var: m.f_id,
                        mutability: m.f_mutability,
                        ty: m.f_ty,
                        value: fused_value,
                    },
                    span,
                };
                stmts.splice(i..i + 4, std::iter::once(new_bind));
                changed = true;
                // Don't advance i — re-check starting here in case the
                // new shorter window opens another match.
                continue;
            }
        }
        i += 1;
    }
    changed
}

/// Pattern match helper for `fuse_let_split_chain`. Returns extracted
/// operands if the 4-stmt window is a `mul → add → scale → gelu` chain
/// threaded through single-use intermediate vars.
struct ChainMatch {
    c_id: VarId,
    d_id: VarId,
    e_id: VarId,
    f_id: VarId,
    a: IrExpr,
    b: IrExpr,
    bias: IrExpr,
    alpha: IrExpr,
    f_mutability: Mutability,
    f_ty: almide_lang::types::Ty,
}

fn match_chain_4gram(window: &[IrStmt]) -> Option<ChainMatch> {
    if window.len() < 4 { return None; }
    let (c_id, c_value) = as_bind(&window[0])?;
    let (a, b) = match_matrix_binary(c_value, "mul")?;

    let (d_id, d_value) = as_bind(&window[1])?;
    let (add_lhs, bias) = match_matrix_binary(d_value, "add")?;
    if !is_var_with_id(add_lhs, c_id) { return None; }

    let (e_id, e_value) = as_bind(&window[2])?;
    let (scale_lhs, alpha) = match_matrix_binary(e_value, "scale")?;
    if !is_var_with_id(scale_lhs, d_id) { return None; }

    let (f_id, f_value) = as_bind(&window[3])?;
    let gelu_arg = match_matrix_unary(f_value, "gelu")?;
    if !is_var_with_id(gelu_arg, e_id) { return None; }

    let (f_mutability, f_ty) = match &window[3].kind {
        IrStmtKind::Bind { mutability, ty, .. } => (*mutability, ty.clone()),
        _ => return None,
    };

    Some(ChainMatch {
        c_id, d_id, e_id, f_id,
        a: a.clone(),
        b: b.clone(),
        bias: bias.clone(),
        alpha: alpha.clone(),
        f_mutability,
        f_ty,
    })
}

fn as_bind(stmt: &IrStmt) -> Option<(VarId, &IrExpr)> {
    match &stmt.kind {
        IrStmtKind::Bind { var, value, .. } => Some((*var, value)),
        _ => None,
    }
}

fn is_var_with_id(expr: &IrExpr, target: VarId) -> bool {
    matches!(&expr.kind, IrExprKind::Var { id } if *id == target)
}

struct VarRefCounter {
    target: VarId,
    count: usize,
}

impl IrVisitor for VarRefCounter {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExprKind::Var { id } = &expr.kind {
            if *id == self.target { self.count += 1; }
        }
        walk_expr(self, expr);
    }
}

fn count_refs_in_stmts_and_tail(stmts: &[IrStmt], tail: Option<&IrExpr>, target: VarId) -> usize {
    let mut v = VarRefCounter { target, count: 0 };
    for s in stmts { v.visit_stmt(s); }
    if let Some(t) = tail { v.visit_expr(t); }
    v.count
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
