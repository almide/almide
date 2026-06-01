//! GlobalizeClosureIdsPass — Closure Architecture v2, phase P0 (WASM-only).
//!
//! `lambda_id` (the field on `IrExprKind::Lambda`) is the key the WASM emitter
//! uses to correlate a raw-Lambda creation site with its pre-scanned
//! `LambdaInfo` (table slot + capture layout): see `emit_lambda_closure`, which
//! takes the FIRST `info.lambda_id == Some(lid)` match. But the frontend assigns
//! `lambda_id` with a counter that **resets to 0 in every module**
//! (`LowerCtx::new`), so a module's lambda and the main program's lambda routinely
//! share id 0. The first-match then resolves a module closure to the WRONG
//! function.
//!
//! Verified failure (the JSON-parser-class bug): with
//! `pub fn neg() -> (Int)->Int = (n) => 0 - n` in a submodule and
//! `fn add() -> (Int)->Int = (n) => n + 1000` in main, `lib.neg()(5)` returns
//! `1005` (main's `add` body) on WASM instead of `-5` — and a variant with no main
//! lambda emits invalid WASM ("unknown table 0: table index out of bounds").
//!
//! This pass re-stamps every `Lambda` node across functions + modules + top-lets
//! with a program-unique, total id, so the emitter's correlation is exact. It is
//! the perf-neutral half of the redesign: it does NOT change which lambdas stay
//! raw, the inline fast path, or the table layout (slots are assigned by the
//! emitter in scan order, independent of these ids). It runs late — after
//! `ClosureConversionPass` has turned capturing lambdas into `ClosureCreate`
//! (which resolve by globally-unique name and need no id) — so it only re-stamps
//! the residual raw lambdas the emitter still matches by id.
//!
//! Full design: docs/roadmap/active/closure-architecture-v2.md.

use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut, walk_stmt_mut};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct GlobalizeClosureIdsPass;

struct Restamp {
    next: u32,
}

impl IrMutVisitor for Restamp {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        if let IrExprKind::Lambda { lambda_id, .. } = &mut expr.kind {
            *lambda_id = Some(self.next);
            self.next += 1;
        }
        walk_expr_mut(self, expr);
    }
    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        walk_stmt_mut(self, stmt);
    }
}

impl NanoPass for GlobalizeClosureIdsPass {
    fn name(&self) -> &str { "GlobalizeClosureIds" }
    // WASM-only: lambda_id is purely a WASM-emit correlation key; the Rust target
    // ignores it and emits native closures.
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }
    fn depends_on(&self) -> Vec<&'static str> { vec![] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // One deterministic traversal over the whole linked program. Order is
        // irrelevant to emitted bytes (ids are a correlation key, not emitted);
        // only program-wide UNIQUENESS matters.
        let mut r = Restamp { next: 0 };
        for func in &mut program.functions {
            r.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            r.visit_expr_mut(&mut tl.value);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                r.visit_expr_mut(&mut func.body);
            }
            for tl in &mut module.top_lets {
                r.visit_expr_mut(&mut tl.value);
            }
        }
        PassResult { program, changed: true }
    }
}
