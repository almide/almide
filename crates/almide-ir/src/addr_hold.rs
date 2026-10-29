//! The address-hold rule (C-376, #4153), as a target-independent IR rewrite.
//!
//! C-376 promises that a named value whose address is bound to a `let` stays
//! valid as long as that binding can be used. Since ADR-0031 the one address
//! op is `ffi`'s `local` `__address`, reached only by the Region mints, which
//! also keep the block in the Region they build (the user-facing spellings
//! `ffi.address` and `bytes.as_ptr` / `as_mut_ptr` / `data_ptr` are gone). It reads its
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
//! Only a `var` or `mut`-parameter operand is rewritten (an assigned
//! address and a `mut` parameter: #4169, [`hold_body`]): a `let` value cannot be rebound or
//! changed in place, and the legs already keep its block to the end of the
//! scope (spec/wasm_cross/bytes_rawptr.almd). A hold is added only when the
//! address op's operand is the binding value's ONLY mention of the var, so
//! nothing in the value can move the var off the addressed block before the
//! hold is taken.

use crate::visit::{walk_expr, IrVisitor};
use crate::visit_mut::{walk_expr_mut, IrMutVisitor};
use std::collections::{HashMap, HashSet};

use crate::{CallTarget, IrExpr, IrExprKind, IrParam, IrProgram, IrStmt, IrStmtKind, Mutability, VarId, VarTable};
use almide_lang::types::Ty;

/// Is `target` an op that hands out the address of its operand's block?
/// The wasm leg's `is_raw_address_op` (#3420) reads this one set.
pub fn is_address_op(target: &CallTarget) -> bool {
    matches!(target, CallTarget::Module { module, func, .. }
        if module.as_str() == "ffi" && func.as_str() == "__address")
}

/// Rewrite every let-bound address of a `var` (or a `mut` parameter) into the
/// held form above, and every address of one assigned to an outer `var` into
/// the outer-held form. Returns the number of holds inserted. Each module's
/// body allocates in its own [`VarTable`] (#2374).
pub fn hold_address_blocks(program: &mut IrProgram) -> usize {
    let IrProgram { functions, modules, top_lets, var_table, .. } = program;
    let mut held = 0;
    for f in functions.iter_mut() {
        held += hold_body(&mut f.body, &f.params, var_table);
    }
    for tl in top_lets.iter_mut() {
        held += hold_body(&mut tl.value, &[], var_table);
    }
    for m in modules.iter_mut() {
        for f in m.functions.iter_mut() {
            held += hold_body(&mut f.body, &f.params, &mut m.var_table);
        }
        for tl in m.top_lets.iter_mut() {
            held += hold_body(&mut tl.value, &[], &mut m.var_table);
        }
    }
    held
}

/// One body. A `mut` parameter is an operand like a `var` (#4169): `b = …`
/// inside the callee releases the block the caller handed over. An address
/// ASSIGNED to a `var` `p` declared in this body (`p = ffi.address(b)`, #4169)
/// takes a hold whose scope is `p`'s, not the assignment's statement:
///
/// ```text
/// var p = …                   var p = …
///                       ⟹     var __addr_hold_N: List[Bytes] = []
/// … { p = ffi.address(b) }    … { p = ffi.address(b); __addr_hold_N = [b] }
/// ```
///
/// Re-assigning the hold releases the block of the address `p` held before,
/// which `p` no longer names.
fn hold_body(body: &mut IrExpr, params: &[IrParam], var_table: &mut VarTable) -> usize {
    let mut_params: HashSet<VarId> = params.iter().filter(|p| p.is_mut).map(|p| p.var).collect();
    let mut declared = HashSet::new();
    let mut assigned = Vec::new();
    scan_targets(body, &mut declared, &mut assigned, var_table, &mut_params);
    let mut outer = HashMap::new();
    for (p, elem) in assigned {
        if declared.contains(&p) && !outer.contains_key(&p) {
            let ty = Ty::list(elem.clone());
            let hold = var_table.alloc(
                almide_base::intern::sym(&format!("__addr_hold_{}", var_table.len())),
                ty,
                Mutability::Var,
                None,
            );
            outer.insert(p, (hold, elem));
        }
    }
    let mut h = Hold { var_table, mut_params, outer, held: 0 };
    h.visit_expr_mut(body);
    h.held
}

/// The vars this body binds (outside lambdas), and the `(p, operand type)` of
/// every assignment `p = …` whose value takes an address of a mutable operand.
fn scan_targets(
    body: &IrExpr,
    declared: &mut HashSet<VarId>,
    assigned: &mut Vec<(VarId, Ty)>,
    vt: &VarTable,
    mut_params: &HashSet<VarId>,
) {
    struct Scan<'a> {
        declared: &'a mut HashSet<VarId>,
        assigned: &'a mut Vec<(VarId, Ty)>,
        vt: &'a VarTable,
        mut_params: &'a HashSet<VarId>,
    }
    impl IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Lambda { .. } = &e.kind {
                return;
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            match &s.kind {
                IrStmtKind::Bind { var, .. } => {
                    self.declared.insert(*var);
                }
                IrStmtKind::Assign { var, value } => {
                    let mut sites = Vec::new();
                    collect_sites(value, self.vt, self.mut_params, &mut sites);
                    if let Some(v) = sites.first() {
                        self.assigned.push((*var, self.vt.get(*v).ty.clone()));
                    }
                }
                _ => {}
            }
            crate::visit::walk_stmt(self, s);
        }
    }
    Scan { declared, assigned, vt, mut_params }.visit_expr(body);
}

struct Hold<'a> {
    var_table: &'a mut VarTable,
    mut_params: HashSet<VarId>,
    /// `p` → its outer hold and the hold's element type.
    outer: HashMap<VarId, (VarId, Ty)>,
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
    /// The operands a hold is taken for in `value`: each var named once.
    fn sites_of(&self, value: &IrExpr) -> Vec<VarId> {
        let mut sites = Vec::new();
        collect_sites(value, self.var_table, &self.mut_params, &mut sites);
        let mut out: Vec<VarId> = Vec::new();
        for v in sites {
            if !out.contains(&v) && mentions(value, v) == 1 {
                out.push(v);
            }
        }
        out
    }

    fn hold_in(&mut self, stmts: &mut Vec<IrStmt>) {
        let mut out = Vec::with_capacity(stmts.len());
        for st in std::mem::take(stmts) {
            let span = st.span;
            let at = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span, def_id: None };
            let mut after = Vec::new();
            match &st.kind {
                IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => {
                    for v in self.sites_of(value) {
                        let ty = self.var_table.get(v).ty.clone();
                        let hold = self.var_table.alloc(
                            almide_base::intern::sym(&format!("__addr_hold_{}", self.var_table.len())),
                            ty.clone(),
                            Mutability::Let,
                            span,
                        );
                        let value = at(IrExprKind::Var { id: v }, ty.clone());
                        after.push(IrStmt { kind: IrStmtKind::Bind { var: hold, mutability: Mutability::Let, ty, value }, span });
                        self.held += 1;
                    }
                    // The declaration of an outer hold sits right after the
                    // `var` it holds for, so it lives exactly as long.
                    if let IrStmtKind::Bind { var, .. } = &st.kind
                        && let Some((hold, elem)) = self.outer.get(var)
                    {
                        let ty = Ty::list(elem.clone());
                        let value = at(IrExprKind::List { elements: vec![] }, ty.clone());
                        after.push(IrStmt { kind: IrStmtKind::Bind { var: *hold, mutability: Mutability::Var, ty, value }, span });
                    }
                }
                IrStmtKind::Assign { var, value } => {
                    if let Some((hold, elem)) = self.outer.get(var).cloned() {
                        let elements: Vec<IrExpr> = self.sites_of(value).into_iter()
                            .map(|v| at(IrExprKind::Var { id: v }, elem.clone()))
                            .collect();
                        if !elements.is_empty() {
                            let value = at(IrExprKind::List { elements }, Ty::list(elem));
                            after.push(IrStmt { kind: IrStmtKind::Assign { var: hold, value }, span });
                            self.held += 1;
                        }
                    }
                }
                _ => {}
            }
            out.push(st);
            out.extend(after);
        }
        *stmts = out;
    }
}

/// The `var` operands of the address ops in `e`, outside any lambda (a
/// lambda's body runs later, under its own bindings).
fn collect_sites(e: &IrExpr, vt: &VarTable, mut_params: &HashSet<VarId>, out: &mut Vec<VarId>) {
    struct Scan<'a> {
        vt: &'a VarTable,
        mut_params: &'a HashSet<VarId>,
        out: &'a mut Vec<VarId>,
    }
    impl IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Lambda { .. } => return,
                IrExprKind::Call { target, args, .. } if is_address_op(target) => {
                    if let [IrExpr { kind: IrExprKind::Var { id }, .. }] = args.as_slice()
                        && (matches!(self.vt.get(*id).mutability, Mutability::Var) || self.mut_params.contains(id))
                    {
                        self.out.push(*id);
                    }
                }
                _ => {}
            }
            walk_expr(self, e);
        }
    }
    Scan { vt, mut_params, out }.visit_expr(e);
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
