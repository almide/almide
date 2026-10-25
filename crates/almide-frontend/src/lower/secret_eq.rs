//! `==` / `!=` over a value holding a `Secret` (#4107, R-secret-03).
//!
//! The checker admits only Secrets whose payload has a constant-time
//! comparison (`secret_type::constant_time_eq_fn`), inside records, tuples,
//! options, results and lists (E103 refuses the rest). Here every such
//! comparison is lowered once, before any leg: the Secret positions call the
//! constant-time fn on the revealed payloads, and the positions around them
//! compare as before. A synthesized walker per compared type keeps each
//! operand evaluated once and lets every leg run the same IR.

use super::LowerCtx;
use crate::secret_type::{constant_time_eq_fn, holds_secret, secret_payload};
use crate::types::{Ty, TypeConstructorId as TC};
use almide_base::intern::{sym, Sym};
use almide_ir::*;

fn e_(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn and(a: IrExpr, b: IrExpr) -> IrExpr {
    e_(IrExprKind::BinOp { op: BinOp::And, left: Box::new(a), right: Box::new(b) }, Ty::Bool)
}

fn lit_bool(v: bool) -> IrExpr {
    e_(IrExprKind::LitBool { value: v }, Ty::Bool)
}

impl LowerCtx<'_> {
    /// `l == r` (or `!=`) when the operand type holds a Secret; None for
    /// every other type, which lowers as before.
    pub(super) fn lower_secret_eq(&mut self, l: IrExpr, r: IrExpr, negate: bool) -> Option<IrExpr> {
        let ty = l.ty.clone();
        if !holds_secret(self.env, &ty) {
            return None;
        }
        let eq = self.secret_eq_expr(l, r, &ty);
        Some(if negate { e_(IrExprKind::UnOp { op: UnOp::Not, operand: Box::new(eq) }, Ty::Bool) } else { eq })
    }

    fn secret_eq_expr(&mut self, a: IrExpr, b: IrExpr, ty: &Ty) -> IrExpr {
        if let Some(p) = secret_payload(ty) {
            let Some((m, f)) = constant_time_eq_fn(p) else {
                // Refused by the checker (E103); never executed.
                return lit_bool(false);
            };
            let hidden = |x: IrExpr| e_(IrExprKind::Member { object: Box::new(x), field: sym("hidden") }, p.clone());
            return e_(IrExprKind::Call {
                target: CallTarget::Module { module: sym(m), func: sym(f), def_id: self.def_map.get(&sym(&format!("{m}.{f}"))).copied() },
                args: vec![hidden(a), hidden(b)], type_args: vec![],
            }, Ty::Bool);
        }
        if !holds_secret(self.env, ty) {
            return e_(IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(a), right: Box::new(b) }, Ty::Bool);
        }
        let name = self.secret_eq_walker(ty);
        e_(IrExprKind::Call { target: CallTarget::Named { name }, args: vec![a, b], type_args: vec![] }, Ty::Bool)
    }

    fn secret_eq_walker(&mut self, ty: &Ty) -> Sym {
        let mangle = format!("{:?}", ty);
        let scope = self.current_module.map(|m| format!("{}_", m.as_str().replace('.', "_"))).unwrap_or_default();
        let name = sym(&format!("__almd_eq_secret_{scope}{:016x}", fnv(&mangle)));
        if self.synthesized_fns.iter().any(|f| f.name == name) || self.declared_repr_pending.contains(&name) {
            return name;
        }
        self.declared_repr_pending.push(name);
        let a = self.var_table.alloc(sym("_a"), ty.clone(), Mutability::Let, None);
        let b = self.var_table.alloc(sym("_b"), ty.clone(), Mutability::Let, None);
        let va = e_(IrExprKind::Var { id: a }, ty.clone());
        let vb = e_(IrExprKind::Var { id: b }, ty.clone());
        let mut extra: Vec<IrFunction> = Vec::new();
        let body = match ty {
            Ty::Tuple(ts) => {
                let mut acc = lit_bool(true);
                for (i, t) in ts.iter().enumerate() {
                    let x = e_(IrExprKind::TupleIndex { object: Box::new(va.clone()), index: i }, t.clone());
                    let y = e_(IrExprKind::TupleIndex { object: Box::new(vb.clone()), index: i }, t.clone());
                    acc = and(acc, self.secret_eq_expr(x, y, t));
                }
                acc
            }
            Ty::Applied(TC::Option, args) if args.len() == 1 => {
                // Nested single-subject matches (not a match over the
                // `(a, b)` pair): the shape every leg's MIR rung lowers.
                let t = &args[0];
                let (x, y) = (self.var_table.alloc(sym("_x"), t.clone(), Mutability::Let, None), self.var_table.alloc(sym("_y"), t.clone(), Mutability::Let, None));
                let both = self.secret_eq_expr(e_(IrExprKind::Var { id: x }, t.clone()), e_(IrExprKind::Var { id: y }, t.clone()), t);
                let b_is_none = e_(IrExprKind::Call {
                    target: CallTarget::Module { module: sym("option"), func: sym("is_none"), def_id: self.def_map.get(&sym("option.is_none")).copied() },
                    args: vec![vb.clone()], type_args: vec![],
                }, Ty::Bool);
                let b_some = e_(IrExprKind::Match { subject: Box::new(vb.clone()), arms: vec![
                    IrMatchArm { pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: y, ty: t.clone() }) }, guard: None, body: both },
                    IrMatchArm { pattern: IrPattern::None, guard: None, body: lit_bool(false) },
                ] }, Ty::Bool);
                e_(IrExprKind::Match { subject: Box::new(va.clone()), arms: vec![
                    IrMatchArm { pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: x, ty: t.clone() }) }, guard: None, body: b_some },
                    IrMatchArm { pattern: IrPattern::None, guard: None, body: b_is_none },
                ] }, Ty::Bool)
            }
            Ty::Applied(TC::Result, args) if args.len() == 2 => {
                let (t, u) = (&args[0], &args[1]);
                let (x, y) = (self.var_table.alloc(sym("_x"), t.clone(), Mutability::Let, None), self.var_table.alloc(sym("_y"), t.clone(), Mutability::Let, None));
                let (p, q) = (self.var_table.alloc(sym("_p"), u.clone(), Mutability::Let, None), self.var_table.alloc(sym("_q"), u.clone(), Mutability::Let, None));
                let oks = self.secret_eq_expr(e_(IrExprKind::Var { id: x }, t.clone()), e_(IrExprKind::Var { id: y }, t.clone()), t);
                let errs = self.secret_eq_expr(e_(IrExprKind::Var { id: p }, u.clone()), e_(IrExprKind::Var { id: q }, u.clone()), u);
                let pair_ty = Ty::Tuple(vec![ty.clone(), ty.clone()]);
                e_(IrExprKind::Match {
                    subject: Box::new(e_(IrExprKind::Tuple { elements: vec![va.clone(), vb.clone()] }, pair_ty)),
                    arms: vec![
                        IrMatchArm { pattern: IrPattern::Tuple { elements: vec![
                            IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: x, ty: t.clone() }) },
                            IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: y, ty: t.clone() }) },
                        ] }, guard: None, body: oks },
                        IrMatchArm { pattern: IrPattern::Tuple { elements: vec![
                            IrPattern::Err { inner: Box::new(IrPattern::Bind { var: p, ty: u.clone() }) },
                            IrPattern::Err { inner: Box::new(IrPattern::Bind { var: q, ty: u.clone() }) },
                        ] }, guard: None, body: errs },
                        IrMatchArm { pattern: IrPattern::Wildcard, guard: None, body: lit_bool(false) },
                    ],
                }, Ty::Bool)
            }
            Ty::Applied(TC::List, args) if args.len() == 1 => {
                let t = &args[0];
                let go = sym(&format!("{}_go", name));
                let len = |ctx: &Self, x: IrExpr| e_(IrExprKind::Call {
                    target: CallTarget::Module { module: sym("list"), func: sym("len"), def_id: ctx.def_map.get(&sym("list.len")).copied() },
                    args: vec![x], type_args: vec![],
                }, Ty::Int);
                let same_len = e_(IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(len(self, va.clone())), right: Box::new(len(self, vb.clone())) }, Ty::Bool);
                let n0 = len(self, va.clone());
                let start = e_(IrExprKind::Call { target: CallTarget::Named { name: go }, args: vec![va.clone(), vb.clone(), e_(IrExprKind::LitInt { value: 0 }, Ty::Int), n0], type_args: vec![] }, Ty::Bool);
                let (ga, gb, gi, gn) = (
                    self.var_table.alloc(sym("_a"), ty.clone(), Mutability::Let, None),
                    self.var_table.alloc(sym("_b"), ty.clone(), Mutability::Let, None),
                    self.var_table.alloc(sym("_i"), Ty::Int, Mutability::Let, None),
                    self.var_table.alloc(sym("_n"), Ty::Int, Mutability::Let, None),
                );
                let var = |id: VarId, t: &Ty| e_(IrExprKind::Var { id }, t.clone());
                let at = |l: VarId| e_(IrExprKind::IndexAccess { object: Box::new(var(l, ty)), index: Box::new(var(gi, &Ty::Int)) }, t.clone());
                let el = self.secret_eq_expr(at(ga), at(gb), t);
                let next_i = e_(IrExprKind::BinOp { op: BinOp::AddInt, left: Box::new(var(gi, &Ty::Int)), right: Box::new(e_(IrExprKind::LitInt { value: 1 }, Ty::Int)) }, Ty::Int);
                let rec = e_(IrExprKind::Call { target: CallTarget::Named { name: go }, args: vec![var(ga, ty), var(gb, ty), next_i, var(gn, &Ty::Int)], type_args: vec![] }, Ty::Bool);
                // The count is a parameter (`i < n` over two scalars, the
                // condition the MIR rung resolves), and the element and the
                // rest are bound before they are combined, so a mismatch
                // never cuts the walk short.
                let cond = e_(IrExprKind::BinOp { op: BinOp::Lt, left: Box::new(var(gi, &Ty::Int)), right: Box::new(var(gn, &Ty::Int)) }, Ty::Bool);
                let (ge, gr) = (
                    self.var_table.alloc(sym("_e"), Ty::Bool, Mutability::Let, None),
                    self.var_table.alloc(sym("_r"), Ty::Bool, Mutability::Let, None),
                );
                let bind = |v: VarId, value: IrExpr| IrStmt { kind: IrStmtKind::Bind { var: v, mutability: Mutability::Let, ty: Ty::Bool, value }, span: None };
                let step = e_(IrExprKind::Block {
                    stmts: vec![bind(ge, el), bind(gr, rec)],
                    expr: Some(Box::new(and(var(ge, &Ty::Bool), var(gr, &Ty::Bool)))),
                }, Ty::Bool);
                let go_body = e_(IrExprKind::If { cond: Box::new(cond), then: Box::new(step), else_: Box::new(lit_bool(true)) }, Ty::Bool);
                extra.push(eq_fn(&self.var_table, go, vec![(ga, ty.clone()), (gb, ty.clone()), (gi, Ty::Int), (gn, Ty::Int)], go_body));
                and(same_len, start)
            }
            Ty::Named(..) => match crate::secret_type::unfold_named(self.env, ty) {
                Some(Ty::Record { fields }) | Some(Ty::OpenRecord { fields }) => {
                    let mut acc = lit_bool(true);
                    for (f, t) in &fields {
                        let x = e_(IrExprKind::Member { object: Box::new(va.clone()), field: *f }, t.clone());
                        let y = e_(IrExprKind::Member { object: Box::new(vb.clone()), field: *f }, t.clone());
                        acc = and(acc, self.secret_eq_expr(x, y, t));
                    }
                    acc
                }
                // A variant holding a Secret is E103 at check time.
                _ => lit_bool(false),
            },
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                let mut acc = lit_bool(true);
                for (f, t) in fields {
                    let x = e_(IrExprKind::Member { object: Box::new(va.clone()), field: *f }, t.clone());
                    let y = e_(IrExprKind::Member { object: Box::new(vb.clone()), field: *f }, t.clone());
                    acc = and(acc, self.secret_eq_expr(x, y, t));
                }
                acc
            }
            // A map or set holding a Secret is E103 at check time.
            _ => lit_bool(false),
        };
        self.synthesized_fns.push(eq_fn(&self.var_table, name, vec![(a, ty.clone()), (b, ty.clone())], body));
        self.synthesized_fns.extend(extra);
        name
    }
}

fn eq_fn(vt: &VarTable, name: Sym, params: Vec<(VarId, Ty)>, body: IrExpr) -> IrFunction {
    IrFunction {
        name,
        params: params.into_iter().map(|(var, ty)| IrParam {
            var, ty, name: vt.get(var).name, borrow: ParamBorrow::Own, is_mut: false,
            open_record: None, default: None, attrs: vec![],
        }).collect(),
        ret_ty: Ty::Bool,
        body,
        is_effect: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![],
        visibility: IrVisibility::Private,
        doc: None, blank_lines_before: 0, def_id: None,
        mutated_params: vec![], module_origin: None, // fresh-fn: synthesized Secret eq walker, no params carry mut
    }
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
