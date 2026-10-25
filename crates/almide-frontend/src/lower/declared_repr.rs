//! The single renderer rule (#4107): a value prints through its type's
//! DECLARED repr (an explicit `fn T.repr`, such as stdlib `Secret.repr`)
//! wherever it sits — at the top of an interpolation, inside an enclosing
//! record, list, option, result, tuple, map or variant, and in the assert /
//! abort failure block that `almide test` reads back.
//!
//! The backends' structural renderers (native `AlmideRepr`, the v1 `__repr_*`
//! walkers, the wasm emitter, the interp) print every field of a value whose
//! type has no explicit repr — and never consult a NESTED type's declared
//! repr. Rather than teach four renderers the rule, the frontend applies it
//! once, here, before any leg sees the IR: a rendered value whose type holds
//! a declared-repr type somewhere gets a synthesized IR walker that prints
//! the canonical structural form and calls the declared repr at each such
//! position. Every leg then runs the same IR, so the bytes agree by
//! construction, and a type without a declared repr anywhere inside it is
//! rendered exactly as before (the IR is left untouched).
//!
//! What it reads is the declaration, never a name: a type is routed iff
//! `find_explicit_convention_fn(ty, "repr")` answers for it.

use super::LowerCtx;
use crate::types::{Ty, TypeConstructorId as TC};
use almide_lang::types::VariantPayload;
use almide_base::intern::{sym, Sym};
use almide_ir::*;

fn e_(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn lit(s: &str) -> IrStringPart {
    IrStringPart::Lit { value: s.to_string() }
}

fn concat(a: IrExpr, b: IrExpr) -> IrExpr {
    e_(IrExprKind::BinOp { op: BinOp::ConcatStr, left: Box::new(a), right: Box::new(b) }, Ty::String)
}

fn interp(parts: Vec<IrStringPart>) -> IrExpr {
    e_(IrExprKind::StringInterp { parts }, Ty::String)
}

fn mk_fn(vt: &VarTable, name: Sym, params: Vec<(VarId, Ty)>, body: IrExpr) -> IrFunction {
    IrFunction {
        name,
        params: params.into_iter().map(|(var, ty)| IrParam {
            var, ty, name: vt.get(var).name, borrow: ParamBorrow::Own, is_mut: false,
            open_record: None, default: None, attrs: vec![],
        }).collect(),
        ret_ty: Ty::String,
        body,
        is_effect: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![],
        visibility: IrVisibility::Private,
        doc: None, blank_lines_before: 0, def_id: None,
        mutated_params: vec![], module_origin: None, // fresh-fn: synthesized repr walker, no params carry mut
    }
}

impl LowerCtx<'_> {
    /// The call target of a convention method found by
    /// [`Self::find_explicit_convention_fn`]: a bundled module's own method
    /// (`secret.Secret.repr`) is a call into that module; every other key is
    /// a named fn, as before.
    pub(super) fn convention_call_target(&self, key: Sym) -> CallTarget {
        let k = key.as_str();
        let mut segs = k.splitn(2, '.');
        if let (Some(m), Some(rest)) = (segs.next(), segs.next())
            && almide_lang::stdlib_info::is_bundled_module(m)
            && rest.contains('.')
        {
            return CallTarget::Module { module: sym(m), func: sym(rest), def_id: None };
        }
        CallTarget::Named { name: key }
    }

    /// `expr` rendered as every renderer must render it (#4107): through its
    /// type's declared repr, or a synthesized walker when one is declared
    /// somewhere inside the type. A type with no declared repr anywhere
    /// comes back unchanged, and the leg's own structural renderer prints it.
    pub(super) fn render_declared(&mut self, expr: IrExpr) -> IrExpr {
        let ty = expr.ty.clone();
        if let Some(f) = self.find_explicit_convention_fn(&ty, "repr") {
            let target = self.convention_call_target(f);
            return e_(IrExprKind::Call { target, args: vec![expr], type_args: vec![] }, Ty::String);
        }
        if !self.holds_declared_repr(&ty, &mut Vec::new()) {
            return expr;
        }
        let name = self.declared_repr_walker(&ty);
        e_(IrExprKind::Call { target: CallTarget::Named { name }, args: vec![expr], type_args: vec![] }, Ty::String)
    }

    /// Does a value of `ty` contain, anywhere, a value whose type declares
    /// its repr? `seen` breaks recursive types.
    pub(super) fn holds_declared_repr(&self, ty: &Ty, seen: &mut Vec<Ty>) -> bool {
        if seen.contains(ty) {
            return false;
        }
        match ty {
            Ty::Named(..) => {
                if self.find_explicit_convention_fn(ty, "repr").is_some() {
                    return true;
                }
                seen.push(ty.clone());
                let found = match self.unfold_named(ty) {
                    Some(Ty::Record { fields }) | Some(Ty::OpenRecord { fields }) =>
                        fields.iter().any(|(_, t)| self.holds_declared_repr(t, seen)),
                    Some(Ty::Variant { cases, .. }) => cases.iter().any(|c| match &c.payload {
                        VariantPayload::Unit => false,
                        VariantPayload::Tuple(ts) => ts.iter().any(|t| self.holds_declared_repr(t, seen)),
                        VariantPayload::Record(fs) => fs.iter().any(|(_, t)| self.holds_declared_repr(t, seen)),
                    }),
                    _ => false,
                };
                seen.pop();
                found
            }
            Ty::Applied(_, args) => args.iter().any(|a| self.holds_declared_repr(a, seen)),
            Ty::Tuple(ts) => ts.iter().any(|t| self.holds_declared_repr(t, seen)),
            Ty::Record { fields } | Ty::OpenRecord { fields } =>
                fields.iter().any(|(_, t)| self.holds_declared_repr(t, seen)),
            _ => false,
        }
    }

    /// One level of a nominal type, its arguments substituted.
    fn unfold_named(&self, ty: &Ty) -> Option<Ty> {
        crate::secret_type::unfold_named(self.env, ty)
    }

    /// The nested form of `x: ty` as a String expression (strings quoted,
    /// as inside a list): the declared repr, a walker, or — for a type with
    /// no declared repr inside — the leg's own nested form, read off a
    /// one-element list (`"${[x]}"` minus its brackets), so the bytes are
    /// exactly the ones the structural renderer would print there.
    fn nested_render(&mut self, x: IrExpr) -> IrExpr {
        let ty = x.ty.clone();
        if self.find_explicit_convention_fn(&ty, "repr").is_some() || self.holds_declared_repr(&ty, &mut Vec::new()) {
            return self.render_declared(x);
        }
        let one = e_(IrExprKind::List { elements: vec![x] }, Ty::list(ty));
        let s = interp(vec![IrStringPart::Expr { expr: one }]);
        let sv = self.var_table.alloc(sym("_s"), Ty::String, Mutability::Let, None);
        let s_ref = e_(IrExprKind::Var { id: sv }, Ty::String);
        let len = e_(IrExprKind::Call {
            target: CallTarget::Module { module: sym("string"), func: sym("len"), def_id: self.def_map.get(&sym("string.len")).copied() },
            args: vec![s_ref.clone()], type_args: vec![],
        }, Ty::Int);
        let end = e_(IrExprKind::BinOp { op: BinOp::SubInt, left: Box::new(len), right: Box::new(e_(IrExprKind::LitInt { value: 1 }, Ty::Int)) }, Ty::Int);
        let sliced = e_(IrExprKind::Call {
            target: CallTarget::Module { module: sym("string"), func: sym("slice"), def_id: self.def_map.get(&sym("string.slice")).copied() },
            args: vec![s_ref, e_(IrExprKind::LitInt { value: 1 }, Ty::Int), end], type_args: vec![],
        }, Ty::String);
        e_(IrExprKind::Block {
            stmts: vec![IrStmt { kind: IrStmtKind::Bind { var: sv, mutability: Mutability::Let, ty: Ty::String, value: s }, span: None }],
            expr: Some(Box::new(sliced)),
        }, Ty::String)
    }

    /// The walker fn for `ty`, synthesized once per lowering unit.
    fn declared_repr_walker(&mut self, ty: &Ty) -> Sym {
        let mangle: String = format!("{:?}", ty).chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        let scope = self.current_module.map(|m| format!("{}_", m.as_str().replace('.', "_"))).unwrap_or_default();
        let name = sym(&format!("__almd_repr_declared_{scope}{:016x}", fnv(&mangle)));
        if self.synthesized_fns.iter().any(|f| f.name == name) || self.declared_repr_pending.contains(&name) {
            return name;
        }
        self.declared_repr_pending.push(name);
        let p = self.var_table.alloc(sym("_v"), ty.clone(), Mutability::Let, None);
        let pv = e_(IrExprKind::Var { id: p }, ty.clone());
        let (params, body) = match ty {
            Ty::Applied(TC::List, a) if a.len() == 1 => return self.list_walker(name, ty, &a[0]),
            Ty::Applied(TC::Map, a) if a.len() == 2 => return self.map_walker(name, ty, &a[0], &a[1]),
            Ty::Applied(TC::Option, a) if a.len() == 1 => {
                let x = self.var_table.alloc(sym("_x"), a[0].clone(), Mutability::Let, None);
                let inner = self.nested_render(e_(IrExprKind::Var { id: x }, a[0].clone()));
                let body = e_(IrExprKind::Match { subject: Box::new(pv), arms: vec![
                    IrMatchArm { pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: x, ty: a[0].clone() }) }, guard: None,
                        body: interp(vec![lit("some("), IrStringPart::Expr { expr: inner }, lit(")")]) },
                    IrMatchArm { pattern: IrPattern::None, guard: None, body: e_(IrExprKind::LitStr { value: "none".into() }, Ty::String) },
                ] }, Ty::String);
                (vec![(p, ty.clone())], body)
            }
            Ty::Applied(TC::Result, a) if a.len() == 2 => {
                let x = self.var_table.alloc(sym("_x"), a[0].clone(), Mutability::Let, None);
                let y = self.var_table.alloc(sym("_y"), a[1].clone(), Mutability::Let, None);
                let rx = self.nested_render(e_(IrExprKind::Var { id: x }, a[0].clone()));
                let ry = self.nested_render(e_(IrExprKind::Var { id: y }, a[1].clone()));
                let body = e_(IrExprKind::Match { subject: Box::new(pv), arms: vec![
                    IrMatchArm { pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: x, ty: a[0].clone() }) }, guard: None,
                        body: interp(vec![lit("ok("), IrStringPart::Expr { expr: rx }, lit(")")]) },
                    IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: y, ty: a[1].clone() }) }, guard: None,
                        body: interp(vec![lit("err("), IrStringPart::Expr { expr: ry }, lit(")")]) },
                ] }, Ty::String);
                (vec![(p, ty.clone())], body)
            }
            Ty::Tuple(ts) => {
                let mut parts = vec![lit("(")];
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 { parts.push(lit(", ")); }
                    let el = e_(IrExprKind::TupleIndex { object: Box::new(pv.clone()), index: i }, t.clone());
                    parts.push(IrStringPart::Expr { expr: self.nested_render(el) });
                }
                parts.push(lit(")"));
                (vec![(p, ty.clone())], interp(parts))
            }
            Ty::Named(n, _) => match self.unfold_named(ty) {
                Some(Ty::Record { fields }) | Some(Ty::OpenRecord { fields }) => {
                    let mut parts = vec![lit(&format!("{} {{ ", almide_ir::declared_type_name(n.as_str())))];
                    for (i, (f, t)) in fields.iter().enumerate() {
                        if i > 0 { parts.push(lit(", ")); }
                        parts.push(lit(&format!("{}: ", f)));
                        let el = e_(IrExprKind::Member { object: Box::new(pv.clone()), field: *f }, t.clone());
                        parts.push(IrStringPart::Expr { expr: self.nested_render(el) });
                    }
                    parts.push(lit(" }"));
                    (vec![(p, ty.clone())], interp(parts))
                }
                Some(Ty::Variant { cases, .. }) => {
                    let mut arms = Vec::new();
                    for c in &cases {
                        let cname = c.name.to_string();
                        let (pattern, body) = match &c.payload {
                            VariantPayload::Unit => (IrPattern::Constructor { name: cname.clone(), args: vec![] }, e_(IrExprKind::LitStr { value: cname.clone() }, Ty::String)),
                            VariantPayload::Tuple(ts) => {
                                let mut pats = Vec::new();
                                let mut parts = vec![lit(&format!("{cname}("))];
                                for (i, t) in ts.iter().enumerate() {
                                    let v = self.var_table.alloc(sym("_x"), t.clone(), Mutability::Let, None);
                                    pats.push(IrPattern::Bind { var: v, ty: t.clone() });
                                    if i > 0 { parts.push(lit(", ")); }
                                    parts.push(IrStringPart::Expr { expr: self.nested_render(e_(IrExprKind::Var { id: v }, t.clone())) });
                                }
                                parts.push(lit(")"));
                                (IrPattern::Constructor { name: cname.clone(), args: pats }, interp(parts))
                            }
                            VariantPayload::Record(fs) => {
                                let mut pats = Vec::new();
                                let mut parts = vec![lit(&format!("{cname} {{ "))];
                                for (i, (f, t)) in fs.iter().enumerate() {
                                    let v = self.var_table.alloc(*f, t.clone(), Mutability::Let, None);
                                    pats.push(IrFieldPattern { name: f.to_string(), pattern: Some(IrPattern::Bind { var: v, ty: t.clone() }) });
                                    if i > 0 { parts.push(lit(", ")); }
                                    parts.push(lit(&format!("{}: ", f)));
                                    parts.push(IrStringPart::Expr { expr: self.nested_render(e_(IrExprKind::Var { id: v }, t.clone())) });
                                }
                                parts.push(lit(" }"));
                                (IrPattern::RecordPattern { name: cname.clone(), fields: pats, rest: false }, interp(parts))
                            }
                        };
                        arms.push(IrMatchArm { pattern, guard: None, body });
                    }
                    (vec![(p, ty.clone())], e_(IrExprKind::Match { subject: Box::new(pv), arms }, Ty::String))
                }
                _ => (vec![(p, ty.clone())], interp(vec![IrStringPart::Expr { expr: pv }])),
            },
            _ => (vec![(p, ty.clone())], interp(vec![IrStringPart::Expr { expr: pv }])),
        };
        self.synthesized_fns.push(mk_fn(&self.var_table, name, params, body));
        name
    }

    /// `[a, b]`: an index-recursive `_go(xs, i, acc)` like the codec list
    /// workers, so every leg lowers it without a closure.
    fn list_walker(&mut self, name: Sym, list_ty: &Ty, elem: &Ty) -> Sym {
        let go = sym(&format!("{}_go", name));
        let xs_a = self.var_table.alloc(sym("_xs"), list_ty.clone(), Mutability::Let, None);
        let entry_body = interp(vec![lit("["), IrStringPart::Expr { expr: e_(IrExprKind::Call {
            target: CallTarget::Named { name: go },
            args: vec![e_(IrExprKind::Var { id: xs_a }, list_ty.clone()), e_(IrExprKind::LitInt { value: 0 }, Ty::Int), e_(IrExprKind::LitStr { value: String::new() }, Ty::String)],
            type_args: vec![],
        }, Ty::String) }, lit("]")]);
        let xs = self.var_table.alloc(sym("_xs"), list_ty.clone(), Mutability::Let, None);
        let i = self.var_table.alloc(sym("_i"), Ty::Int, Mutability::Let, None);
        let acc = self.var_table.alloc(sym("_acc"), Ty::String, Mutability::Let, None);
        let var = |id: VarId, t: &Ty| e_(IrExprKind::Var { id }, t.clone());
        let el = e_(IrExprKind::IndexAccess { object: Box::new(var(xs, list_ty)), index: Box::new(var(i, &Ty::Int)) }, elem.clone());
        let rendered = self.nested_render(el);
        let sep = e_(IrExprKind::If {
            cond: Box::new(e_(IrExprKind::BinOp { op: BinOp::Gt, left: Box::new(var(i, &Ty::Int)), right: Box::new(e_(IrExprKind::LitInt { value: 0 }, Ty::Int)) }, Ty::Bool)),
            then: Box::new(e_(IrExprKind::LitStr { value: ", ".into() }, Ty::String)),
            else_: Box::new(e_(IrExprKind::LitStr { value: String::new() }, Ty::String)),
        }, Ty::String);
        let next_acc = concat(concat(var(acc, &Ty::String), sep), rendered);
        let len = e_(IrExprKind::Call {
            target: CallTarget::Module { module: sym("list"), func: sym("len"), def_id: self.def_map.get(&sym("list.len")).copied() },
            args: vec![var(xs, list_ty)], type_args: vec![],
        }, Ty::Int);
        let cond = e_(IrExprKind::BinOp { op: BinOp::Lt, left: Box::new(var(i, &Ty::Int)), right: Box::new(len) }, Ty::Bool);
        let next_i = e_(IrExprKind::BinOp { op: BinOp::AddInt, left: Box::new(var(i, &Ty::Int)), right: Box::new(e_(IrExprKind::LitInt { value: 1 }, Ty::Int)) }, Ty::Int);
        let go_body = e_(IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(e_(IrExprKind::Call { target: CallTarget::Named { name: go }, args: vec![var(xs, list_ty), next_i, next_acc], type_args: vec![] }, Ty::String)),
            else_: Box::new(var(acc, &Ty::String)),
        }, Ty::String);
        self.synthesized_fns.push(mk_fn(&self.var_table, name, vec![(xs_a, list_ty.clone())], entry_body));
        self.synthesized_fns.push(mk_fn(&self.var_table, go, vec![(xs, list_ty.clone()), (i, Ty::Int), (acc, Ty::String)], go_body));
        name
    }

    /// `["k": v, …]` (`[:]` when empty), over `map.entries` — the form and
    /// order the legs print a map in.
    fn map_walker(&mut self, name: Sym, map_ty: &Ty, k: &Ty, v: &Ty) -> Sym {
        let pair = Ty::Tuple(vec![k.clone(), v.clone()]);
        let pairs_ty = Ty::list(pair.clone());
        let pairs_name = sym(&format!("{}_pairs", name));
        let m = self.var_table.alloc(sym("_m"), map_ty.clone(), Mutability::Let, None);
        let to_list = e_(IrExprKind::Call {
            target: CallTarget::Module { module: sym("map"), func: sym("entries"), def_id: self.def_map.get(&sym("map.entries")).copied() },
            args: vec![e_(IrExprKind::Var { id: m }, map_ty.clone())], type_args: vec![],
        }, pairs_ty.clone());
        let is_empty = e_(IrExprKind::Call {
            target: CallTarget::Module { module: sym("map"), func: sym("is_empty"), def_id: self.def_map.get(&sym("map.is_empty")).copied() },
            args: vec![e_(IrExprKind::Var { id: m }, map_ty.clone())], type_args: vec![],
        }, Ty::Bool);
        let entry = e_(IrExprKind::If {
            cond: Box::new(is_empty),
            then: Box::new(e_(IrExprKind::LitStr { value: "[:]".into() }, Ty::String)),
            else_: Box::new(interp(vec![lit("["), IrStringPart::Expr { expr: e_(IrExprKind::Call {
                target: CallTarget::Named { name: pairs_name }, args: vec![to_list, e_(IrExprKind::LitInt { value: 0 }, Ty::Int), e_(IrExprKind::LitStr { value: String::new() }, Ty::String)], type_args: vec![],
            }, Ty::String) }, lit("]")])),
        }, Ty::String);
        let xs = self.var_table.alloc(sym("_xs"), pairs_ty.clone(), Mutability::Let, None);
        let i = self.var_table.alloc(sym("_i"), Ty::Int, Mutability::Let, None);
        let acc = self.var_table.alloc(sym("_acc"), Ty::String, Mutability::Let, None);
        let var = |id: VarId, t: &Ty| e_(IrExprKind::Var { id }, t.clone());
        let el = e_(IrExprKind::IndexAccess { object: Box::new(var(xs, &pairs_ty)), index: Box::new(var(i, &Ty::Int)) }, pair.clone());
        let rk = self.nested_render(e_(IrExprKind::TupleIndex { object: Box::new(el.clone()), index: 0 }, k.clone()));
        let rv = self.nested_render(e_(IrExprKind::TupleIndex { object: Box::new(el), index: 1 }, v.clone()));
        let sep = e_(IrExprKind::If {
            cond: Box::new(e_(IrExprKind::BinOp { op: BinOp::Gt, left: Box::new(var(i, &Ty::Int)), right: Box::new(e_(IrExprKind::LitInt { value: 0 }, Ty::Int)) }, Ty::Bool)),
            then: Box::new(e_(IrExprKind::LitStr { value: ", ".into() }, Ty::String)),
            else_: Box::new(e_(IrExprKind::LitStr { value: String::new() }, Ty::String)),
        }, Ty::String);
        let next_acc = concat(var(acc, &Ty::String), interp(vec![IrStringPart::Expr { expr: sep }, IrStringPart::Expr { expr: rk }, lit(": "), IrStringPart::Expr { expr: rv }]));
        let len = e_(IrExprKind::Call {
            target: CallTarget::Module { module: sym("list"), func: sym("len"), def_id: self.def_map.get(&sym("list.len")).copied() },
            args: vec![var(xs, &pairs_ty)], type_args: vec![],
        }, Ty::Int);
        let cond = e_(IrExprKind::BinOp { op: BinOp::Lt, left: Box::new(var(i, &Ty::Int)), right: Box::new(len) }, Ty::Bool);
        let next_i = e_(IrExprKind::BinOp { op: BinOp::AddInt, left: Box::new(var(i, &Ty::Int)), right: Box::new(e_(IrExprKind::LitInt { value: 1 }, Ty::Int)) }, Ty::Int);
        let go_body = e_(IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(e_(IrExprKind::Call { target: CallTarget::Named { name: pairs_name }, args: vec![var(xs, &pairs_ty), next_i, next_acc], type_args: vec![] }, Ty::String)),
            else_: Box::new(var(acc, &Ty::String)),
        }, Ty::String);
        self.synthesized_fns.push(mk_fn(&self.var_table, name, vec![(m, map_ty.clone())], entry));
        self.synthesized_fns.push(mk_fn(&self.var_table, pairs_name, vec![(xs, pairs_ty), (i, Ty::Int), (acc, Ty::String)], go_body));
        name
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
