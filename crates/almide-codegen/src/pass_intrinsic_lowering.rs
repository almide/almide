//! IntrinsicLoweringPass: rewrite `CallTarget::Module { m, f }` calls
//! targeting an `@intrinsic(symbol)`-annotated stdlib fn into
//! `IrExprKind::RuntimeCall { symbol, args }`.
//!
//! This is Phase 1e-2 of the dispatch unification arc
//! (`docs/roadmap/active/dispatch-unification-plan.md`). Starting here,
//! downstream emit (Rust walker, WASM emitter) can consume a single
//! target-neutral IR node for runtime fn calls; the per-target
//! `pass_stdlib_lowering` / `emit_<m>_call` paths remain for `@inline_rust`
//! and L2-L3 dispatchers that have not yet migrated.
//!
//! The pass reads `@intrinsic("almide_rt_...")` attributes from
//! `program.modules[*].functions[*].attrs` and builds a
//! `(module, func) → symbol` map. Its `IrMutVisitor` then rewrites every
//! matching call site across top-level fns, top-lets, and nested module fns.
//!
//! Ordering: runs on both Rust and WASM targets. Must execute before
//! `StdlibLoweringPass` so that Rust-target code sees the already-rewritten
//! `RuntimeCall` node and does NOT emit an `InlineRust` template for the
//! same call. Also before `ResolveCalls` to avoid the bundled → Named
//! rewrite competing with this rewrite.

use std::collections::HashMap;
use almide_base::intern::Sym;
use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut, walk_stmt_mut};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct IntrinsicLoweringPass;

impl NanoPass for IntrinsicLoweringPass {
    fn name(&self) -> &str { "IntrinsicLowering" }

    fn targets(&self) -> Option<Vec<Target>> {
        // Both targets: the point of this arc is a single lowering site.
        None
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let map = collect_intrinsics(&program);
        if map.is_empty() {
            return PassResult { program, changed: false };
        }

        struct Rewriter<'a> { map: &'a HashMap<(Sym, Sym), Sym> }
        impl<'a> IrMutVisitor for Rewriter<'a> {
            fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
                walk_expr_mut(self, expr);
                let IrExprKind::Call { target, args, .. } = &mut expr.kind else { return };
                let CallTarget::Module { module, func } = target else { return };
                let Some(&symbol) = self.map.get(&(*module, *func)) else { return };
                let args = std::mem::take(args);
                expr.kind = IrExprKind::RuntimeCall { symbol, args };
            }
            fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
                walk_stmt_mut(self, stmt);
            }
        }

        let mut rw = Rewriter { map: &map };
        for func in &mut program.functions {
            rw.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
        for mi in 0..program.modules.len() {
            for fi in 0..program.modules[mi].functions.len() {
                let mut body = std::mem::replace(
                    &mut program.modules[mi].functions[fi].body,
                    IrExpr::default(),
                );
                rw.visit_expr_mut(&mut body);
                program.modules[mi].functions[fi].body = body;
            }
            for ti in 0..program.modules[mi].top_lets.len() {
                let mut val = std::mem::replace(
                    &mut program.modules[mi].top_lets[ti].value,
                    IrExpr::default(),
                );
                rw.visit_expr_mut(&mut val);
                program.modules[mi].top_lets[ti].value = val;
            }
        }
        PassResult { program, changed: true }
    }
}

/// Collect every `(module, func) → runtime_symbol` declared via
/// `@intrinsic("symbol")` across bundled stdlib / user modules.
fn collect_intrinsics(program: &IrProgram) -> HashMap<(Sym, Sym), Sym> {
    use almide_lang::ast::AttrValue;
    use almide_base::intern::sym;

    let mut out = HashMap::new();
    for module in &program.modules {
        for func in &module.functions {
            let Some(attr) = func.attrs.iter().find(|a| a.name.as_str() == "intrinsic") else {
                continue;
            };
            let Some(first) = attr.args.first() else { continue };
            let AttrValue::String { value } = &first.value else { continue };
            out.insert((module.name, func.name), sym(value));
        }
    }
    out
}
