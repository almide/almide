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
//! (`let x = matrix.add(...); matrix.scale(x, ...)`) which requires
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

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let rules = almide_egg_lab::matrix_fusion_rules();
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
        // `gemm → add → scale → gelu` fusion's natural direction.
        walk_expr_mut(self, expr);

        if !is_saturation_target(expr) {
            return;
        }

        if let Some(new_expr) = try_saturate(expr, self.rules, self.vt) {
            *expr = new_expr;
            self.changed = true;
        }
    }
}

/// Whether `expr` is a Call the bridge knows how to lift. Currently:
/// all `matrix.<op>` Calls, plus the list combinators the bridge has
/// a lift rule for (`map / filter / fold / flat_map / filter_map`).
/// Other list calls (e.g. `list.len`) lift as opaque slots — cheap
/// but skippable, so we avoid paying saturation cost for them.
fn is_saturation_target(expr: &IrExpr) -> bool {
    let IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func }, .. } = &expr.kind else {
        return false;
    };
    match module.as_str() {
        "matrix" => true,
        "list" => matches!(
            func.as_str(),
            "map" | "filter" | "fold" | "flat_map" | "filter_map"
        ),
        _ => false,
    }
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
    bridge.lower(&best, vt).ok()
}
