//! Parameter borrow inference (#2028): which droppable params of a
//! program function the CALLEE owns, and which it only borrows.
//!
//! The structural convention after #2004 was callee-owned for every
//! droppable param: the call site shares a Var argument (+1), the callee
//! releases the param at its exit plan. Correct, and one inc/dec pair per
//! call — on `check(t: Tree)` walking a 2^20-node tree that pair ran per
//! node and cost 51 % (#2028). The reference shape is Lean's and Koka's
//! borrow inference (Ullrich & de Moura, "Counting Immutable Beans", §5):
//! a param the body only READS is borrowed — the caller keeps the credit,
//! the site shares nothing, the callee releases nothing.
//!
//! The rule here is the conservative half of theirs. A param starts
//! borrowed and becomes OWNED when the body
//!
//! * assigns or mutates it in place (`p = …`, `p[i] = …`, `p.f = …`,
//!   the list-mutation statements) — the Assign routes release the old
//!   value, which a borrowed param's caller still holds;
//! * passes it directly (`Var`) to a program-function param that is
//!   owned (the fixpoint), or to a call this pass cannot resolve the way
//!   the emitter resolves it (the suffix and registry routes) — owned is
//!   always safe, the pair is merely paid.
//!
//! Every other use is a read or a share the emitter already pays for: a
//! bind / constructor / Retain arm / closure capture takes +1 because a
//! Var is not an owned result (`rc_share_guard`), a returned Var takes
//! its ret-inc (func.rs), a match binding is a view into the block.
//!
//! Frames outside the table keep the owned convention: lifted lambdas
//! (`env_shift != 0`) and prim-using bodies (the raw-address rule).
//! Both sides of every call edge read ONE table — `FnInfo::param_owned`
//! — so the site's share and the callee's release cannot disagree.

use std::collections::HashMap;

use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrFunction, IrStmt, IrStmtKind, VarId};

use crate::FnTable;
use crate::types_table::TypeTable;

/// One `Vec<bool>` per table entry (owned = true), aligned with
/// `FnInfo::params`. Non-droppable params are `false` (no RC site).
pub(crate) fn infer(
    program_fns: &[(&IrFunction, Option<String>, u32)],
    table: &FnTable,
    types: &TypeTable,
) -> Vec<Vec<bool>> {
    let n = program_fns.len();
    let mut owned: Vec<Vec<bool>> = (0..n)
        .map(|i| vec![false; table.infos[i].params.len()])
        .collect();
    // Frames that keep every param owned: a prim body (raw-address rule),
    // a refused body (never lowered), and a registry implementation — the
    // display / matrix / string-scan emitters call those directly by
    // index with their own argument convention (callee-owned), outside
    // lower_call_at where the table is read.
    let registry: std::collections::HashSet<usize> = table.impl_index.values().copied().collect();
    let all_owned: Vec<bool> = program_fns
        .iter()
        .enumerate()
        .map(|(i, (f, _, _))| {
            table.infos[i].refuse.is_some()
                || registry.contains(&i)
                || crate::rc_ownership::body_uses_prim(&f.body)
        })
        .collect();
    for (i, (f, _, _)) in program_fns.iter().enumerate() {
        for (k, &t) in table.infos[i].params.iter().enumerate() {
            let droppable = crate::rc_ownership::rc_droppable_ty(types, t);
            if droppable && (all_owned[i] || f.params.get(k).is_some_and(|p| p.is_mut)) {
                owned[i][k] = true;
            }
        }
    }
    loop {
        let mut changed = false;
        for (i, (f, qual, _)) in program_fns.iter().enumerate() {
            if all_owned[i] {
                continue;
            }
            let module = qual
                .as_deref()
                .and_then(|q| q.rsplit_once('.').map(|(m, _)| m.to_string()));
            let params: HashMap<VarId, usize> = f
                .params
                .iter()
                .enumerate()
                .map(|(k, p)| (p.var, k))
                .collect();
            let mut scan = Scan {
                params: &params,
                table,
                types,
                module,
                owned: &owned,
                me: i,
                mark: Vec::new(),
                cross: Vec::new(),
            };
            scan.visit_expr(&f.body);
            scan.tail_fresh(&f.body);
            let Scan { mark, cross, .. } = scan;
            for k in mark {
                if crate::rc_ownership::rc_droppable_ty(types, table.infos[i].params[k])
                    && !owned[i][k]
                {
                    owned[i][k] = true;
                    changed = true;
                }
            }
            for (j, k2) in cross {
                if let Some(t) = table.infos[j].params.get(k2)
                    && crate::rc_ownership::rc_droppable_ty(types, *t)
                    && !owned[j][k2]
                {
                    owned[j][k2] = true;
                    changed = true;
                }
            }
        }
        if !changed {
            return owned;
        }
    }
}

struct Scan<'a> {
    params: &'a HashMap<VarId, usize>,
    table: &'a FnTable,
    types: &'a TypeTable,
    module: Option<String>,
    owned: &'a [Vec<bool>],
    /// This fn's own table index.
    me: usize,
    /// This fn's params found consumed.
    mark: Vec<usize>,
    /// `(callee, param)` pairs a TRUE tail site of this body hands a fresh
    /// temporary to: the site cannot release after the jump, so the
    /// callee must own that param (calls.rs `true_tail`).
    cross: Vec<(usize, usize)>,
}

impl Scan<'_> {
    /// The result positions of the body — a Block tail, both If arms, every
    /// Match arm, and the Try / Unwrap see-through (emitter.rs) — mirroring
    /// where the emitter lowers a Named call with `tail == true`.
    fn tail_fresh(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Block { expr: Some(t), .. } => self.tail_fresh(t),
            IrExprKind::If { then, else_, .. } => {
                self.tail_fresh(then);
                self.tail_fresh(else_);
            }
            IrExprKind::Match { arms, .. } => {
                for a in arms {
                    self.tail_fresh(&a.body);
                }
            }
            IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } => self.tail_fresh(expr),
            IrExprKind::Call {
                target: CallTarget::Named { name },
                args,
                ..
            }
            | IrExprKind::TailCall {
                target: CallTarget::Named { name },
                args,
            } => {
                let Resolved::Fn(j) = self.resolve(name.as_str()) else {
                    return;
                };
                for (k2, a) in args.iter().enumerate() {
                    if !self.tail_safe_arg(a) {
                        self.cross.push((j, k2));
                    }
                }
            }
            _ => {}
        }
    }

    /// May this argument reach a BORROWED param from a true tail site? The
    /// site's exit plan releases the frame's owned locals before the jump
    /// (and a loop-form rebind releases the previous iteration's), so only
    /// a value the frame does not own survives it: a param this frame
    /// itself borrows (its caller keeps it), or a pool-static literal.
    fn tail_safe_arg(&self, a: &IrExpr) -> bool {
        match &a.kind {
            IrExprKind::LitStr { .. } => true,
            IrExprKind::Var { id } => match self.params.get(id) {
                Some(&k) => !self.owned[self.me].get(k).copied().unwrap_or(true),
                None => false,
            },
            _ => false,
        }
    }
}

impl Scan<'_> {
    fn param_of(&self, e: &IrExpr) -> Option<usize> {
        match &e.kind {
            IrExprKind::Var { id } => self.params.get(id).copied(),
            _ => None,
        }
    }

    fn mark_var(&mut self, id: &VarId) {
        if let Some(&k) = self.params.get(id) {
            self.mark.push(k);
        }
    }

    /// The emitter's resolution for a Named call, minus the suffix and
    /// registry routes: the current module's qualified name, then the
    /// entry program's. `None` = a constructor (its slots take the share
    /// +1) or a call this pass does not resolve (treated as owned).
    fn resolve(&self, name: &str) -> Resolved {
        if self.types.ctors.contains_key(name) {
            return Resolved::Ctor;
        }
        let hit = self
            .module
            .as_deref()
            .and_then(|m| self.table.by_name.get(&format!("{m}.{name}")))
            .or_else(|| self.table.by_name.get(name))
            .copied();
        match hit {
            Some(i) => Resolved::Fn(i),
            None => Resolved::Unknown,
        }
    }

    fn call_args(&mut self, target: &CallTarget, args: &[IrExpr]) {
        let resolved = match target {
            CallTarget::Named { name } => self.resolve(name.as_str()),
            // A module op may take the rc == 1 fast path (an in-place map
            // set, a bytes append, a linked body's realloc-free push):
            // "unique" must mean "mine", and a borrowed param's block is
            // the caller's — Lean's rule, a borrowed value is never unique.
            // Owned keeps the site's +1 in front of every such arm.
            CallTarget::Module { .. } => Resolved::Unknown,
            // A closure call shares a Var argument (calls.rs) and the
            // lifted body owns its params: a read here.
            CallTarget::Computed { .. } => return,
            _ => Resolved::Unknown,
        };
        for (k2, a) in args.iter().enumerate() {
            let Some(k) = self.param_of(a) else { continue };
            let owned = match resolved {
                Resolved::Ctor => false,
                Resolved::Unknown => true,
                Resolved::Fn(j) => self.owned[j].get(k2).copied().unwrap_or(true),
            };
            if owned {
                self.mark.push(k);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Resolved {
    Fn(usize),
    Ctor,
    Unknown,
}

impl IrVisitor for Scan<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
                self.call_args(target, args);
            }
            // A runtime symbol call has no declared modes here: owned.
            IrExprKind::RuntimeCall { args, .. } => {
                for a in args {
                    if let Some(k) = self.param_of(a) {
                        self.mark.push(k);
                    }
                }
            }
            // A lambda body is its own frame (the capture takes +1 at the
            // env store); its uses are of the capture local, not the param.
            IrExprKind::Lambda { .. } => return,
            _ => {}
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
            | IrStmtKind::RcInc { var }
            | IrStmtKind::RcDec { var } => self.mark_var(var),
            IrStmtKind::ListCopySlice { dst, src, .. } => {
                self.mark_var(dst);
                self.mark_var(src);
            }
            _ => {}
        }
        walk_stmt(self, s);
    }
}
