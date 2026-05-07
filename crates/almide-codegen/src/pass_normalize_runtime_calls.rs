//! NormalizeRuntimeCallsPass: collapse the legacy `Named { "almide_rt_*" }`
//! representation into the canonical `RuntimeCall { symbol, args }` IR
//! node before the walker / WASM emitter run.
//!
//! ## Why this pass exists
//!
//! Historically, several upstream passes (`ResolveCalls`, `StdlibLowering`,
//! `ResultPropagation`, `BuiltinLowering` UFCS arm, ...) produce
//! `IrExprKind::Call { target: CallTarget::Named { name: "almide_rt_..." }, ... }`
//! to represent stdlib runtime calls. `IntrinsicLoweringPass` ran early
//! and converted only the calls visible at that point; anything created
//! later leaked through to the walker as a regular `Named` call.
//!
//! That dual representation conflates two distinct intents in a single
//! IR node:
//! * **User call** — a `Named { name }` referring to a user-defined or
//!   imported `IrFunction`.
//! * **Runtime call** — a call into the bundled Rust/WASM runtime,
//!   identified by the `almide_rt_<module>_<func>` symbol.
//!
//! Conflating them previously enabled a real bug: a user function named
//! `value_to_float` was indistinguishable from the runtime symbol
//! `almide_rt_value_to_float`, and a name-prefix hack rewrote the user
//! call into a non-existent runtime symbol.
//!
//! ## What this pass does
//!
//! After every other pass has finished generating the IR, this pass
//! sweeps the program and rewrites every `Named { name }` whose name
//! starts with `almide_rt_` (the reserved runtime-symbol prefix) into
//! `RuntimeCall { symbol: name, args }`. The walker may then assume
//! a clean invariant:
//!
//!   * `RuntimeCall { symbol }` ⇒ runtime helper, prefixed `almide_rt_*`.
//!   * `Named { name }` ⇒ user function, never prefixed `almide_rt_*`.
//!
//! Combined with the walker assertion (see
//! `walker/expressions.rs::render_generic_call`), this makes the
//! "user call gets mangled into a non-existent runtime symbol" class
//! of bugs structurally impossible: there is no syntactic path from a
//! user-named call to a runtime emission.
//!
//! ## Pipeline placement
//!
//! Runs LAST, after every other pass on every Rust-target build. The
//! intent is to be a thin uniformity guarantee for downstream emit, not
//! a semantic transformation. As individual generators migrate to
//! produce `RuntimeCall` directly (the long-term goal), this pass
//! becomes a no-op on those paths and can eventually be retired.

use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut, walk_stmt_mut};
use super::pass::{NanoPass, PassResult, Target};

const RUNTIME_PREFIX: &str = "almide_rt_";

#[derive(Debug)]
pub struct NormalizeRuntimeCallsPass;

impl NanoPass for NormalizeRuntimeCallsPass {
    fn name(&self) -> &str { "NormalizeRuntimeCalls" }

    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        struct Rewriter;
        impl IrMutVisitor for Rewriter {
            fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
                walk_expr_mut(self, expr);
                let IrExprKind::Call { target, args, .. } = &mut expr.kind else { return };
                let CallTarget::Named { name } = target else { return };
                if !name.as_str().starts_with(RUNTIME_PREFIX) { return }
                let symbol = *name;
                let args = std::mem::take(args);
                expr.kind = IrExprKind::RuntimeCall { symbol, args };
            }
            fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
                walk_stmt_mut(self, stmt);
            }
        }

        let mut rw = Rewriter;
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
