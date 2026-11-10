//! `m[k] = e(map.get(m, k) ?? d)` as ONE keyed operation on the native
//! render: `m = map.upsert(m, k, e(d), (old) => e(old))`. Spelled literally
//! the statement is a lookup followed by a `map.set` that looks the same key
//! up again — two hashes and two probes per update. The standard render
//! applies the same rule at the statement (`statements_map_upsert.rs`).
//!
//! Equivalence with the two-step form. `e` holds only literals, immutable
//! local variables other than `m` and `k`, the operators below, and the ONE
//! `map.get(m, k) ?? d` read, so nothing in it observes or changes `m`.
//! `map.upsert` evaluates its `init` — `e(d)` — on every call, also when the
//! key is present, so every admitted operator is total: Int `+ - *` wrap and
//! Float arithmetic follows IEEE-754. An operator that can abort (`/` and `%`
//! on Int, `**`) keeps the two-step form, as does a call, a second read of
//! `m`, a key expression, a global or a parameter target.
use almide_ir::{visit_mut::{walk_stmt_mut, IrMutVisitor}, *};
use almide_lang::{intern::sym, types::{Ty, TypeConstructorId as C}};
use std::collections::HashSet;

pub(crate) fn expand(ir: &mut IrProgram) {
    // A metered region charges what its plan names; the update closure is a
    // callback the plan never saw, so such a program keeps the two steps.
    if almide_ir::meter_plan::program_has_regions(ir) { return; }
    let globals = ir.top_lets.iter().map(|t| t.var).collect();
    let mut pass = Upsert { fixed: globals, vars: &mut ir.var_table };
    for f in &mut ir.functions {
        let params: Vec<VarId> = f.params.iter().map(|p| p.var).collect();
        pass.fixed.extend(params.iter().copied());
        pass.visit_expr_mut(&mut f.body);
        for p in &params { pass.fixed.remove(p); }
    }
}

/// `fixed`: the globals and the current function's parameters — neither a
/// rebind target nor an operand here.
struct Upsert<'a> { fixed: HashSet<VarId>, vars: &'a mut VarTable }

impl IrMutVisitor for Upsert<'_> {
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        walk_stmt_mut(self, s);
        let IrStmtKind::MapInsert { target, key, value } = &s.kind else { return; };
        if let Some(call) = self.rewrite(*target, key, value) {
            s.kind = IrStmtKind::Assign { var: *target, value: call };
        }
    }
}

/// The one statement being matched: its map, its key and the value type.
struct Site<'a> { map: VarId, key: VarId, fixed: &'a HashSet<VarId>, vars: &'a VarTable }

impl Upsert<'_> {
    fn rewrite(&mut self, map: VarId, key: &IrExpr, value: &IrExpr) -> Option<IrExpr> {
        let IrExprKind::Var { id: key_id } = key.kind else { return None; };
        let map_ty = self.vars.get(map).ty.clone();
        if self.fixed.contains(&map) || !admitted_map(&map_ty, &key.ty, &value.ty) { return None; }
        let site = Site { map, key: key_id, fixed: &self.fixed, vars: self.vars };
        let mut reads = 0;
        let init = site.body(value, None, &mut reads)?;
        if reads != 1 { return None; }
        let old = self.vars.alloc_fresh("upsert_old", value.ty.clone(), Mutability::Let, None);
        let site = Site { map, key: key_id, fixed: &self.fixed, vars: self.vars };
        let read = node(IrExprKind::Var { id: old }, value.ty.clone());
        let update = site.body(value, Some(&read), &mut reads)?;
        let closure = Ty::Fn { params: vec![value.ty.clone()], ret: Box::new(value.ty.clone()), is_effect: false };
        let lambda = node(IrExprKind::Lambda { params: vec![(old, value.ty.clone())], body: Box::new(update), lambda_id: None }, closure);
        let receiver = node(IrExprKind::Var { id: map }, map_ty.clone());
        let target = CallTarget::Module { module: sym("map"), func: sym("upsert"), def_id: None };
        Some(node(IrExprKind::Call { target, args: vec![receiver, key.clone(), init, lambda], type_args: Vec::new() }, map_ty))
    }
}

impl Site<'_> {
    /// `e` with its `map.get(m, k) ?? d` read replaced by `read` (by `d`
    /// itself when `None`), counting the reads; `None` outside the shape.
    fn body(&self, e: &IrExpr, read: Option<&IrExpr>, reads: &mut usize) -> Option<IrExpr> {
        let kind = match &e.kind {
            IrExprKind::UnwrapOr { expr, fallback } => {
                if !self.is_read(expr) { return None; }
                *reads += 1;
                return read.cloned().or_else(|| self.operand(fallback));
            }
            IrExprKind::BinOp { op, left, right } if total_bin(*op) => IrExprKind::BinOp {
                op: *op,
                left: Box::new(self.body(left, read, reads)?),
                right: Box::new(self.body(right, read, reads)?),
            },
            IrExprKind::UnOp { op, operand } if matches!(op, UnOp::NegInt | UnOp::NegFloat) => IrExprKind::UnOp {
                op: *op,
                operand: Box::new(self.body(operand, read, reads)?),
            },
            _ => return self.operand(e),
        };
        Some(IrExpr { kind, ty: e.ty.clone(), span: e.span, def_id: None })
    }

    /// `map.get(m, k)` of this site's own map and key.
    fn is_read(&self, e: &IrExpr) -> bool {
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind else { return false; };
        let [m, k] = args.as_slice() else { return false; };
        module.as_str() == "map" && func.as_str() == "get"
            && matches!(m.kind, IrExprKind::Var { id } if id == self.map)
            && matches!(k.kind, IrExprKind::Var { id } if id == self.key)
    }

    /// A leaf: a number literal, or an immutable local that is neither the
    /// map nor the key.
    fn operand(&self, e: &IrExpr) -> Option<IrExpr> {
        let leaf = match &e.kind {
            IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } => true,
            IrExprKind::Var { id } => *id != self.map && *id != self.key && !self.fixed.contains(id)
                && self.vars.get(*id).mutability == Mutability::Let,
            _ => false,
        };
        (leaf && matches!(e.ty, Ty::Int | Ty::Float)).then(|| e.clone())
    }
}

/// The operators that cannot abort: evaluating one early is unobservable.
fn total_bin(op: BinOp) -> bool {
    matches!(op, BinOp::AddInt | BinOp::SubInt | BinOp::MulInt
        | BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat | BinOp::DivFloat)
}

/// A `Map[String | Int, Int | Float]` written with its own key and value types.
fn admitted_map(map: &Ty, key: &Ty, value: &Ty) -> bool {
    let Ty::Applied(C::Map, args) = map else { return false; };
    let [k, v] = args.as_slice() else { return false; };
    k == key && v == value && matches!(k, Ty::String | Ty::Int) && matches!(v, Ty::Int | Ty::Float)
}

fn node(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}
