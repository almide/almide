//! EggSaturationPass — opt-in equality-saturation front for matrix
//! fusion.
//!
//! When enabled, each function body is walked bottom-up. Every matrix
//! Call expression (recognised by `CallTarget::Module { module:
//! "matrix", .. }`) is lifted into an egg e-graph via
//! `almide-egg-lab::Bridge`, saturated against the
//! auto-generated `matrix_fusion_rules()` (built from stdlib
//! `@rewrite` attributes), and the extracted best form is lowered
//! back into IR to replace the original subtree.
//!
//! ## Scope in Stage 1
//!
//! Tree-shaped matrix expressions only. `MatrixFusionPass` runs after
//! this pass and handles the complementary let-split chain form
//! (`let x = a + b; matrix.scale(x, ...)`) which requires
//! reasoning across binders that the egg bridge does not yet model.
//! The two passes compose: egg fuses everything it can reach, the
//! imperative pass mops up whatever remains.
//!
//! ## Opt-in
//!
//! Driven by `CodegenOptions::opt_egg`. `build_pipeline(target,
//! opt_egg=true)` slots the pass in ahead of `MatrixFusionPass`;
//! otherwise the pass is omitted entirely and `MatrixFusionPass`
//! alone drives fusion.

use almide_ir::{walk_expr_mut, IrExpr, IrExprKind, IrMutVisitor, IrProgram, VarTable};

use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct EggSaturationPass;

impl NanoPass for EggSaturationPass {
    fn name(&self) -> &str { "EggSaturation" }
    fn targets(&self) -> Option<Vec<Target>> { None }

    /// Matches the `CallTarget::Module` matrix chains the intrinsic and stdlib
    /// lowerings replace.
    fn run_before(&self) -> Vec<&'static str> { vec!["IntrinsicLowering", "StdlibLowering"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Full rule set, matrix + list; only matrix Calls are lifted
        // (`is_saturation_target`), so the list half is inert here.
        let rules = almide_egg_lab::fusion_rules();
        let mut v = EggVisitor { rules: &rules, vt: &mut program.var_table, changed: false };
        for func in &mut program.functions {
            v.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            v.visit_expr_mut(&mut tl.value);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                v.visit_expr_mut(&mut func.body);
            }
            for tl in &mut module.top_lets {
                v.visit_expr_mut(&mut tl.value);
            }
        }
        let changed = v.changed;
        PassResult { program, changed }
    }
}

struct EggVisitor<'a> {
    rules: &'a [egg::Rewrite<almide_egg_lab::AlmideExpr, ()>],
    vt: &'a mut VarTable,
    changed: bool,
}

impl<'a> IrMutVisitor for EggVisitor<'a> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Recurse first so inner subtrees fuse before enclosing ones
        // are considered. Bottom-up matches the chain-shaped
        // `mul(a, scale(b, s))` fusion's natural direction.
        walk_expr_mut(self, expr);

        if let Some(fused) = fused_fma(expr) {
            *expr = fused;
            self.changed = true;
            return;
        }

        if !is_saturation_target(expr) {
            return;
        }

        if let Some(new_expr) = try_saturate(expr, self.rules, self.vt) {
            *expr = new_expr;
            self.changed = true;
        }
    }
}

/// The args of a `matrix.<func>(..)` call of `arity` operands.
fn matrix_call_args<'a>(e: &'a IrExpr, func: &str, arity: usize) -> Option<&'a [IrExpr]> {
    match &e.kind {
        IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func: f, .. }, args, .. }
            if module.as_str() == "matrix" && f.as_str() == func && args.len() == arity =>
        {
            Some(args)
        }
        _ => None,
    }
}

/// R-matrix-03 on the native leg, the same rewrite the v1 lowering
/// (`almide-mir` `fused_fma`) and the wasm front desugar make:
/// `scale(a, ka) + scale(b, kb)` is `__fma(a, ka, b, kb)` and
/// `__fma(..) + scale(c, kc)` is `__fma3(..)` — the same per-cell products
/// and left-to-right sum over the operands' overlap, operands in written
/// order, but one pass over the overlap instead of a whole-operand `scale`
/// copy each (#3644). The visitor is post-order, so a three-term sum arrives
/// as `__fma(..) + scale(..)`.
fn fused_fma(expr: &IrExpr) -> Option<IrExpr> {
    let (a, b) = match &expr.kind {
        IrExprKind::BinOp { op: almide_ir::BinOp::AddMatrix, left, right } => (&**left, &**right),
        IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "matrix" && func.as_str() == "__add" && args.len() == 2 =>
        {
            (&args[0], &args[1])
        }
        _ => return None,
    };
    let rhs = matrix_call_args(b, "scale", 2)?;
    let (fused, lhs) = matrix_call_args(a, "scale", 2)
        .map(|l| ("__fma", l))
        .or_else(|| matrix_call_args(a, "__fma", 4).map(|l| ("__fma3", l)))?;
    Some(IrExpr {
        kind: IrExprKind::Call {
            target: almide_ir::CallTarget::Module {
                module: almide_base::intern::sym("matrix"),
                func: almide_base::intern::sym(fused),
                def_id: None,
            },
            args: lhs.iter().chain(rhs).cloned().collect(),
            type_args: vec![],
        },
        ty: expr.ty.clone(),
        span: expr.span,
        def_id: None,
    })
}

/// Whether `expr` is a node the bridge knows how to lift: every
/// `matrix.<op>` Call, and the matrix operators (`a * b`, `a + b`, a scale),
/// which lower to `matrix.__mul` / `__add` / `scale` (R-matrix-14). The list combinators are NOT lifted any more
/// (#2045): the bridge's list rules compose lambdas by substituting one
/// body into another (`map-filter-fuse` copies `f`'s body three times),
/// which duplicates and reorders a callback's side effects — the
/// native leg printed `m1 f1 m1 m1` where the spec leg prints
/// `m1 m2 m3 f1`. List fusion is `StreamFusionPass` (after
/// StdlibLowering), which keeps each callback intact and fuses only
/// when the interleaving is unobservable. The rules themselves stay in
/// `almide-egg-lab` (its bridge tests cover them).
fn is_saturation_target(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Call { target: almide_ir::CallTarget::Module { module, .. }, .. } => module.as_str() == "matrix",
        IrExprKind::BinOp { op, .. } => is_matrix_operator(*op),
        _ => false,
    }
}

fn is_matrix_operator(op: almide_ir::BinOp) -> bool {
    matches!(op, almide_ir::BinOp::MulMatrix | almide_ir::BinOp::AddMatrix | almide_ir::BinOp::ScaleMatrix)
}

fn try_saturate(
    expr: &IrExpr,
    rules: &[egg::Rewrite<almide_egg_lab::AlmideExpr, ()>],
    vt: &mut VarTable,
) -> Option<IrExpr> {
    use egg::{Extractor, Runner};

    let mut bridge = almide_egg_lab::Bridge::new();
    let (rec, root) = bridge.lift(expr);
    let runner = Runner::default()
        .with_iter_limit(32)
        .with_node_limit(4_000)
        .with_expr(&rec)
        .run(rules);
    let canonical = runner.egraph.find(root);
    let extractor = Extractor::new(&runner.egraph, almide_egg_lab::FusionCost);
    let (_, best) = extractor.find_best(canonical);
    // A subtree is replaced only by a fusion: an unfused one keeps its own
    // nodes, so an operator keeps rendering as the operator it is.
    if !best.as_ref().iter().any(|n| matches!(n, almide_egg_lab::AlmideExpr::MatrixMulScaled(_))) {
        return None;
    }
    bridge.lower(&best, vt).ok()
}
