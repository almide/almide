//! The address-hold rule (C-376, #4153), as a target-independent IR rewrite.
//!
//! C-376 promises that a named value whose address is bound to a `let` stays
//! valid as long as that binding can be used. An address op (`ffi.address`,
//! and the deprecated `bytes.as_ptr` / `as_mut_ptr` / `data_ptr`) reads its
//! block without owning it, so when the named value is a `var` the promise
//! broke on the first rebinding: `b = …` released the old block while the
//! let-bound address still pointed into it, a later allocation took the
//! freed bytes, and a write through the address landed in an unrelated live
//! value (native) or over a free-list header (wasm). An in-place op that
//! grows the var (`bytes.append_i64_le`) moved the block out from under the address
//! the same way.
//!
//! # The shape
//!
//! ```text
//! let p = ffi.address(b)          // b is a `var`
//! ⟹
//! let p = ffi.address(b)
//! let __addr_hold_N = b
//! ```
//!
//! The hold is an ordinary `let` alias of the block, in the same block scope
//! as `p`, so it lives exactly as long as the binding the contract names. It
//! is taken AFTER the binding: `bytes.as_mut_ptr` makes its receiver unique
//! (copy-on-write) before it hands out the address, and an alias taken
//! before it would have sent that copy, and every write through the
//! address, to the hold instead of `b`. On every leg an alias of a heap
//! value shares the block (native `AlmideRcCow` clone, a wasm rc
//! increment), so the address stays the address of `b`'s block and a write
//! through it is seen by `b` until `b` changes. When `b` is rebound, the
//! hold keeps the old block; when `b` is changed in place, copy-on-write
//! gives `b` a fresh block and the hold keeps the old one. The address never
//! dangles inside its binding.
//!
//! Stated once here, in the IR every leg consumes, from a `let` and a `Var` —
//! the same reason the exit-code rule is a rewrite (exit_code.rs).
//!
//! Only a `var` operand is rewritten: a `let` value cannot be rebound or
//! changed in place, and the legs already keep its block to the end of the
//! scope (spec/wasm_cross/bytes_rawptr.almd). A hold is added only when the
//! address op's operand is the binding value's ONLY mention of the var, so
//! nothing in the value can move the var off the addressed block before the
//! hold is taken.

use crate::visit::{walk_expr, IrVisitor};
use crate::visit_mut::{walk_expr_mut, IrMutVisitor};
use crate::{CallTarget, IrExpr, IrExprKind, IrProgram, IrStmt, IrStmtKind, Mutability, VarId, VarTable};

/// Is `target` an op that hands out the address of its operand's block?
/// The wasm leg's `is_raw_address_op` (#3420) reads this one set.
pub fn is_address_op(target: &CallTarget) -> bool {
    matches!(target, CallTarget::Module { module, func, .. }
        if (module.as_str() == "bytes" && matches!(func.as_str(), "data_ptr" | "as_ptr" | "as_mut_ptr"))
            || (module.as_str() == "ffi" && func.as_str() == "address"))
}

/// Rewrite every let-bound address of a `var` into the held form above.
/// Returns the number of holds inserted. Each module's body allocates in its
/// own [`VarTable`] (#2374).
pub fn hold_address_blocks(program: &mut IrProgram) -> usize {
    let IrProgram { functions, modules, top_lets, var_table, .. } = program;
    let mut h = Hold { var_table, held: 0 };
    for f in functions.iter_mut() {
        h.visit_expr_mut(&mut f.body);
    }
    for tl in top_lets.iter_mut() {
        h.visit_expr_mut(&mut tl.value);
    }
    let mut held = h.held;
    for m in modules.iter_mut() {
        let mut h = Hold { var_table: &mut m.var_table, held: 0 };
        for f in m.functions.iter_mut() {
            h.visit_expr_mut(&mut f.body);
        }
        for tl in m.top_lets.iter_mut() {
            h.visit_expr_mut(&mut tl.value);
        }
        held += h.held;
    }
    held
}

struct Hold<'a> {
    var_table: &'a mut VarTable,
    held: usize,
}

impl IrMutVisitor for Hold<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Inner blocks first. A binding in a nested block takes its hold
        // there; if the nested block is itself the value of an outer
        // binding, that binding takes one too, for its own longer scope.
        walk_expr_mut(self, expr);
        match &mut expr.kind {
            IrExprKind::Block { stmts, .. } => self.hold_in(stmts),
            IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => self.hold_in(body),
            _ => {}
        }
    }
}

impl Hold<'_> {
    fn hold_in(&mut self, stmts: &mut Vec<IrStmt>) {
        let mut out = Vec::with_capacity(stmts.len());
        for st in std::mem::take(stmts) {
            let mut holds = Vec::new();
            if let IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } = &st.kind {
                let mut sites = Vec::new();
                collect_sites(value, self.var_table, &mut sites);
                for v in sites {
                    if holds.iter().any(|h: &IrStmt| matches!(&h.kind, IrStmtKind::Bind { value, .. }
                        if matches!(value.kind, IrExprKind::Var { id } if id == v)))
                        || mentions(value, v) != 1
                    {
                        continue;
                    }
                    let ty = self.var_table.get(v).ty.clone();
                    let hold = self.var_table.alloc(
                        almide_base::intern::sym(&format!("__addr_hold_{}", self.var_table.len())),
                        ty.clone(),
                        Mutability::Let,
                        st.span,
                    );
                    let span = st.span;
                    holds.push(IrStmt {
                        kind: IrStmtKind::Bind {
                            var: hold,
                            mutability: Mutability::Let,
                            ty: ty.clone(),
                            value: IrExpr { kind: IrExprKind::Var { id: v }, ty, span, def_id: None },
                        },
                        span,
                    });
                    self.held += 1;
                }
            }
            out.push(st);
            out.extend(holds);
        }
        *stmts = out;
    }
}

/// The `var` operands of the address ops in `e`, outside any lambda (a
/// lambda's body runs later, under its own bindings).
fn collect_sites(e: &IrExpr, vt: &VarTable, out: &mut Vec<VarId>) {
    struct Scan<'a> {
        vt: &'a VarTable,
        out: &'a mut Vec<VarId>,
    }
    impl IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Lambda { .. } => return,
                IrExprKind::Call { target, args, .. } if is_address_op(target) => {
                    if let [IrExpr { kind: IrExprKind::Var { id }, .. }] = args.as_slice()
                        && matches!(self.vt.get(*id).mutability, Mutability::Var)
                    {
                        self.out.push(*id);
                    }
                }
                _ => {}
            }
            walk_expr(self, e);
        }
    }
    Scan { vt, out }.visit_expr(e);
}

/// How many times `e` names `v`, as a read or as a write target.
fn mentions(e: &IrExpr, v: VarId) -> usize {
    struct Count(VarId, usize);
    impl IrVisitor for Count {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(&e.kind, IrExprKind::Var { id } if *id == self.0) {
                self.1 += 1;
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            match &s.kind {
                IrStmtKind::Assign { var, .. }
                | IrStmtKind::IndexAssign { target: var, .. }
                | IrStmtKind::MapInsert { target: var, .. }
                | IrStmtKind::FieldAssign { target: var, .. }
                | IrStmtKind::ListSwap { target: var, .. }
                | IrStmtKind::ListReverse { target: var, .. }
                | IrStmtKind::ListRotateLeft { target: var, .. }
                | IrStmtKind::ListCopySlice { dst: var, .. }
                    if *var == self.0 =>
                {
                    self.1 += 1;
                }
                _ => {}
            }
            crate::visit::walk_stmt(self, s);
        }
    }
    let mut c = Count(v, 0);
    c.visit_expr(e);
    c.1
}
