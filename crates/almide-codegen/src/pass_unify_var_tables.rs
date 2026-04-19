//! Unify `IrModule.var_table` into `IrProgram.var_table`.
//!
//! `IrModule` historically owned a local `VarTable` so that per-module
//! lowering could assign VarIds starting from 0 without seeing the
//! main program's table. This made every downstream pass decide "which
//! var_table does this VarId belong to?" and drove a
//! `top_let_globals_by_name` name-keyed mirror in `emit_wasm` to
//! resolve cross-module top-level globals when the VarId regions
//! didn't match.
//!
//! Post-pass invariant: `program.var_table` is the single authoritative
//! `VarTable`. Every `VarId` in any `IrFunction` / `IrTopLet` under
//! `program.functions` or `program.modules[_].functions` /
//! `program.modules[_].top_lets` indexes into `program.var_table`.
//! `module.var_table` is cleared (entries empty) and becomes a dead
//! field — callers that still reference it should migrate to
//! `program.var_table`.
//!
//! Roadmap: `active/var-table-unification.md`
//! (spun out of `codegen-ideal-form` #5).

use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut, walk_stmt_mut, walk_pattern_mut};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct UnifyVarTablesPass;

struct VarIdShifter {
    offset: u32,
}

impl VarIdShifter {
    #[inline]
    fn shift(&self, id: &mut VarId) {
        id.0 += self.offset;
    }
}

impl IrMutVisitor for VarIdShifter {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        match &mut expr.kind {
            IrExprKind::Var { id } => self.shift(id),
            IrExprKind::Lambda { params, .. } => {
                for (id, _) in params.iter_mut() { self.shift(id); }
            }
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.shift(var);
                if let Some(vt) = var_tuple {
                    for id in vt.iter_mut() { self.shift(id); }
                }
            }
            IrExprKind::ClosureCreate { captures, .. } => {
                for (id, _) in captures.iter_mut() { self.shift(id); }
            }
            _ => {}
        }
        walk_expr_mut(self, expr);
    }

    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        match &mut stmt.kind {
            IrStmtKind::Bind { var, .. }
            | IrStmtKind::Assign { var, .. } => self.shift(var),
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. }
            | IrStmtKind::ListSwap { target, .. }
            | IrStmtKind::ListReverse { target, .. }
            | IrStmtKind::ListRotateLeft { target, .. } => self.shift(target),
            IrStmtKind::ListCopySlice { dst, src, .. } => {
                self.shift(dst);
                self.shift(src);
            }
            _ => {}
        }
        walk_stmt_mut(self, stmt);
    }

    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        if let IrPattern::Bind { var, .. } = p {
            self.shift(var);
        }
        walk_pattern_mut(self, p);
    }
}

impl NanoPass for UnifyVarTablesPass {
    fn name(&self) -> &str { "UnifyVarTables" }
    fn targets(&self) -> Option<Vec<Target>> { None } // all targets

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut any_merged = false;
        for module in program.modules.iter_mut() {
            if module.var_table.entries.is_empty() { continue; }
            let offset = program.var_table.entries.len() as u32;
            let mut shifter = VarIdShifter { offset };

            for func in module.functions.iter_mut() {
                for p in func.params.iter_mut() {
                    shifter.shift(&mut p.var);
                }
                shifter.visit_expr_mut(&mut func.body);
            }
            for tl in module.top_lets.iter_mut() {
                shifter.shift(&mut tl.var);
                shifter.visit_expr_mut(&mut tl.value);
            }

            // Move VarInfo entries into the program-level table, then
            // leave the module's table empty so any lingering
            // `module.var_table` read is an obvious zero-result rather
            // than a stale hit from a half-migrated call site.
            let drained: Vec<VarInfo> = std::mem::take(&mut module.var_table.entries);
            program.var_table.entries.extend(drained);
            any_merged = true;
        }
        PassResult { program, changed: any_merged }
    }
}
