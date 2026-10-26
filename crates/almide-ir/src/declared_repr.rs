//! The single renderer rule (#4107): a value prints through its type's
//! DECLARED repr (an explicit `fn T.repr`, such as stdlib `Secret.repr`)
//! wherever it sits — at the top of an interpolation, inside an enclosing
//! record, list, set, option, result, tuple, map or variant, and in the
//! assert / abort failure block that `almide test` reads back.
//!
//! The backends' structural renderers (native `AlmideRepr`, the v1 `__repr_*`
//! walkers, the wasm emitter, the interp) print every field of a value whose
//! type has no explicit repr — and never consult a NESTED type's declared
//! repr. Rather than teach four renderers the rule, it is applied to the IR
//! before any leg sees it: a rendered value whose type holds a declared-repr
//! type somewhere gets a synthesized IR walker that prints the canonical
//! structural form and calls the declared repr at each such position. Every
//! leg then runs the same IR, so the bytes agree by construction, and a type
//! without a declared repr anywhere inside it is rendered exactly as before
//! (the IR is left untouched).
//!
//! The rule runs TWICE, through this one synthesizer:
//!
//! - at lowering, for every segment whose type is known there
//!   (`almide-frontend`'s `lower/declared_repr.rs` supplies the facts);
//! - after monomorphization ([`route_instantiated_segments`]), for a segment
//!   inside a generic body (#4239): `fn show[A](x: A) = "${x}"` has a type
//!   variable where the lowering looked, and only the instance knows that
//!   `A` is a `Secret`. The lowering records the declared reprs and type
//!   shapes it knows in [`DeclaredReprTable`] for this second run.
//!
//! What it reads is the declaration, never a name: a type is routed iff the
//! facts name a declared `repr` for it.

use crate::*;
use almide_base::intern::{sym, Sym};
use almide_lang::types::{Ty, TypeConstructorId as TC, VariantCase, VariantPayload};
use std::collections::HashMap;

/// What the synthesizer needs to know about types: which declare their
/// repr, and what a nominal type is one level down.
pub trait ReprFacts {
    /// The call that renders a value of `ty` through its own declared repr.
    fn repr_target(&self, ty: &Ty) -> Option<CallTarget>;
    /// One level of a nominal type, its arguments substituted.
    fn unfold_named(&self, ty: &Ty) -> Option<Ty>;
    /// The definition a stdlib call resolves to, when the caller knows it.
    fn def_id(&self, key: &str) -> Option<DefId>;
}

/// The declared reprs and nominal shapes of a program, recorded by the
/// lowering for the post-monomorphization run (#4239). Keyed by the name a
/// `Ty::Named` carries.
#[derive(Debug, Clone, Default)]
pub struct DeclaredReprTable {
    /// Type name → the call rendering it through its declared repr.
    pub targets: HashMap<Sym, CallTarget>,
    /// Type name → (generic parameters, declared shape).
    pub shapes: HashMap<Sym, (Vec<Sym>, Ty)>,
}

impl ReprFacts for DeclaredReprTable {
    fn repr_target(&self, ty: &Ty) -> Option<CallTarget> {
        let Ty::Named(n, _) = ty else { return None };
        self.targets.get(n).cloned()
    }

    fn unfold_named(&self, ty: &Ty) -> Option<Ty> {
        let Ty::Named(n, args) = ty else { return None };
        let (params, shape) = self.shapes.get(n)?;
        if args.is_empty() || params.is_empty() {
            return Some(shape.clone());
        }
        let bindings: HashMap<Sym, Ty> = params.iter().copied().zip(args.iter().cloned()).collect();
        Some(almide_lang::types::substitute(shape, &bindings))
    }

    fn def_id(&self, _key: &str) -> Option<DefId> {
        None
    }
}

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

/// The walker synthesizer over one lowering unit (the entry program or one
/// module): its var table, the fns it synthesizes, and its walker scope.
pub struct ReprSynth<'a> {
    pub facts: &'a dyn ReprFacts,
    pub var_table: &'a mut VarTable,
    /// Synthesized walkers land here.
    pub out: &'a mut Vec<IrFunction>,
    /// Walker names already taken (a recursive type names its own walker
    /// before the body exists; a second run finds the first run's walkers).
    pub pending: &'a mut Vec<Sym>,
    /// `""` for the entry program, `"{module}_"` (dots as `_`) in a module.
    pub scope: String,
}

impl ReprSynth<'_> {
    /// `expr` rendered as every renderer must render it (#4107): through its
    /// type's declared repr, or a synthesized walker when one is declared
    /// somewhere inside the type. A type with no declared repr anywhere
    /// comes back unchanged, and the leg's own structural renderer prints it.
    pub fn render_declared(&mut self, expr: IrExpr) -> IrExpr {
        let ty = expr.ty.clone();
        if let Some(target) = self.facts.repr_target(&ty) {
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
    pub fn holds_declared_repr(&self, ty: &Ty, seen: &mut Vec<Ty>) -> bool {
        holds_declared_repr(self.facts, ty, seen)
    }

    /// The nested form of `x: ty` as a String expression (strings quoted,
    /// as inside a list): the declared repr, a walker, or — for a type with
    /// no declared repr inside — the leg's own nested form, read off a
    /// one-element list (`"${[x]}"` minus its brackets), so the bytes are
    /// exactly the ones the structural renderer would print there.
    fn nested_render(&mut self, x: IrExpr) -> IrExpr {
        let ty = x.ty.clone();
        if self.facts.repr_target(&ty).is_some() || self.holds_declared_repr(&ty, &mut Vec::new()) {
            return self.render_declared(x);
        }
        let one = e_(IrExprKind::List { elements: vec![x] }, Ty::list(ty));
        let s = interp(vec![IrStringPart::Expr { expr: one }]);
        let sv = self.var_table.alloc(sym("_s"), Ty::String, Mutability::Let, None);
        let s_ref = e_(IrExprKind::Var { id: sv }, Ty::String);
        let len = e_(IrExprKind::Call {
            target: CallTarget::Module { module: sym("string"), func: sym("len"), def_id: self.facts.def_id("string.len") },
            args: vec![s_ref.clone()], type_args: vec![],
        }, Ty::Int);
        let end = e_(IrExprKind::BinOp { op: BinOp::SubInt, left: Box::new(len), right: Box::new(e_(IrExprKind::LitInt { value: 1 }, Ty::Int)) }, Ty::Int);
        let sliced = e_(IrExprKind::Call {
            target: CallTarget::Module { module: sym("string"), func: sym("slice"), def_id: self.facts.def_id("string.slice") },
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
        let name = sym(&format!("__almd_repr_declared_{}{:016x}", self.scope, fnv(&mangle)));
        if self.out.iter().any(|f| f.name == name) || self.pending.contains(&name) {
            return name;
        }
        self.pending.push(name);
        let p = self.var_table.alloc(sym("_v"), ty.clone(), Mutability::Let, None);
        let pv = e_(IrExprKind::Var { id: p }, ty.clone());
        let body = match ty {
            Ty::Applied(TC::List, a) if a.len() == 1 => return self.list_walker(name, ty, &a[0]),
            Ty::Applied(TC::Map, a) if a.len() == 2 => return self.map_walker(name, ty, &a[0], &a[1]),
            Ty::Applied(TC::Set, a) if a.len() == 1 => self.set_repr_body(pv, &a[0]),
            Ty::Applied(TC::Option, a) if a.len() == 1 => self.option_repr_body(pv, &a[0]),
            Ty::Applied(TC::Result, a) if a.len() == 2 => self.result_repr_body(pv, &a[0], &a[1]),
            Ty::Tuple(ts) => self.tuple_repr_body(&pv, ts),
            Ty::Named(n, _) => match self.facts.unfold_named(ty) {
                Some(Ty::Record { fields }) | Some(Ty::OpenRecord { fields }) => self.record_repr_body(&pv, *n, &fields),
                Some(Ty::Variant { cases, .. }) => self.variant_repr_body(pv, &cases),
                _ => interp(vec![IrStringPart::Expr { expr: pv }]),
            },
            _ => interp(vec![IrStringPart::Expr { expr: pv }]),
        };
        let params = vec![(p, ty.clone())];
        self.out.push(mk_fn(self.var_table, name, params, body));
        name
    }

    /// `set.from_list([<e0>, ...])` (#4240): the set's elements in the
    /// order the legs print them (`set.to_list`), through the ONE list
    /// walker every collection's elements take — never a structural print
    /// of an element type that declares its repr.
    fn set_repr_body(&mut self, pv: IrExpr, elem: &Ty) -> IrExpr {
        let list_ty = Ty::list(elem.clone());
        let as_list = e_(IrExprKind::Call {
            target: CallTarget::Module { module: sym("set"), func: sym("to_list"), def_id: self.facts.def_id("set.to_list") },
            args: vec![pv], type_args: vec![],
        }, list_ty.clone());
        let list_walker = self.declared_repr_walker(&list_ty);
        let rendered = e_(IrExprKind::Call { target: CallTarget::Named { name: list_walker }, args: vec![as_list], type_args: vec![] }, Ty::String);
        interp(vec![lit("set.from_list("), IrStringPart::Expr { expr: rendered }, lit(")")])
    }

    /// `some(<inner>)` / `none`.
    fn option_repr_body(&mut self, pv: IrExpr, a: &Ty) -> IrExpr {
        let x = self.var_table.alloc(sym("_x"), a.clone(), Mutability::Let, None);
        let inner = self.nested_render(e_(IrExprKind::Var { id: x }, a.clone()));
        e_(IrExprKind::Match { subject: Box::new(pv), arms: vec![
            IrMatchArm { pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: x, ty: a.clone() }) }, guard: None,
                body: interp(vec![lit("some("), IrStringPart::Expr { expr: inner }, lit(")")]) },
            IrMatchArm { pattern: IrPattern::None, guard: None, body: e_(IrExprKind::LitStr { value: "none".into() }, Ty::String) },
        ] }, Ty::String)
    }

    /// `ok(<x>)` / `err(<y>)`.
    fn result_repr_body(&mut self, pv: IrExpr, a: &Ty, b: &Ty) -> IrExpr {
        let x = self.var_table.alloc(sym("_x"), a.clone(), Mutability::Let, None);
        let y = self.var_table.alloc(sym("_y"), b.clone(), Mutability::Let, None);
        let rx = self.nested_render(e_(IrExprKind::Var { id: x }, a.clone()));
        let ry = self.nested_render(e_(IrExprKind::Var { id: y }, b.clone()));
        e_(IrExprKind::Match { subject: Box::new(pv), arms: vec![
            IrMatchArm { pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: x, ty: a.clone() }) }, guard: None,
                body: interp(vec![lit("ok("), IrStringPart::Expr { expr: rx }, lit(")")]) },
            IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: y, ty: b.clone() }) }, guard: None,
                body: interp(vec![lit("err("), IrStringPart::Expr { expr: ry }, lit(")")]) },
        ] }, Ty::String)
    }

    /// `(<e0>, <e1>, ...)`.
    fn tuple_repr_body(&mut self, pv: &IrExpr, ts: &[Ty]) -> IrExpr {
        let mut parts = vec![lit("(")];
        for (i, t) in ts.iter().enumerate() {
            if i > 0 { parts.push(lit(", ")); }
            let el = e_(IrExprKind::TupleIndex { object: Box::new(pv.clone()), index: i }, t.clone());
            parts.push(IrStringPart::Expr { expr: self.nested_render(el) });
        }
        parts.push(lit(")"));
        interp(parts)
    }

    /// `Name { f: <v>, ... }`.
    fn record_repr_body(&mut self, pv: &IrExpr, n: Sym, fields: &[(Sym, Ty)]) -> IrExpr {
        let mut parts = vec![lit(&format!("{} {{ ", declared_type_name(n.as_str())))];
        for (i, (f, t)) in fields.iter().enumerate() {
            if i > 0 { parts.push(lit(", ")); }
            parts.push(lit(&format!("{}: ", f)));
            let el = e_(IrExprKind::Member { object: Box::new(pv.clone()), field: *f }, t.clone());
            parts.push(IrStringPart::Expr { expr: self.nested_render(el) });
        }
        parts.push(lit(" }"));
        interp(parts)
    }

    /// One match arm per case.
    fn variant_repr_body(&mut self, pv: IrExpr, cases: &[VariantCase]) -> IrExpr {
        let arms = cases.iter().map(|c| self.variant_repr_arm(c)).collect();
        e_(IrExprKind::Match { subject: Box::new(pv), arms }, Ty::String)
    }

    /// `Case`, `Case(<x>, ...)` or `Case { f: <v>, ... }`.
    fn variant_repr_arm(&mut self, c: &VariantCase) -> IrMatchArm {
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
        IrMatchArm { pattern, guard: None, body }
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
            target: CallTarget::Module { module: sym("list"), func: sym("len"), def_id: self.facts.def_id("list.len") },
            args: vec![var(xs, list_ty)], type_args: vec![],
        }, Ty::Int);
        let cond = e_(IrExprKind::BinOp { op: BinOp::Lt, left: Box::new(var(i, &Ty::Int)), right: Box::new(len) }, Ty::Bool);
        let next_i = e_(IrExprKind::BinOp { op: BinOp::AddInt, left: Box::new(var(i, &Ty::Int)), right: Box::new(e_(IrExprKind::LitInt { value: 1 }, Ty::Int)) }, Ty::Int);
        let go_body = e_(IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(e_(IrExprKind::Call { target: CallTarget::Named { name: go }, args: vec![var(xs, list_ty), next_i, next_acc], type_args: vec![] }, Ty::String)),
            else_: Box::new(var(acc, &Ty::String)),
        }, Ty::String);
        self.out.push(mk_fn(self.var_table, name, vec![(xs_a, list_ty.clone())], entry_body));
        self.out.push(mk_fn(self.var_table, go, vec![(xs, list_ty.clone()), (i, Ty::Int), (acc, Ty::String)], go_body));
        name
    }

    /// `["k": v, …]` (`[:]` when empty), over `map.to_list` — the form and
    /// order the legs print a map in.
    fn map_walker(&mut self, name: Sym, map_ty: &Ty, k: &Ty, v: &Ty) -> Sym {
        let pair = Ty::Tuple(vec![k.clone(), v.clone()]);
        let pairs_ty = Ty::list(pair.clone());
        let pairs_name = sym(&format!("{}_pairs", name));
        let m = self.var_table.alloc(sym("_m"), map_ty.clone(), Mutability::Let, None);
        let to_list = e_(IrExprKind::Call {
            target: CallTarget::Module { module: sym("map"), func: sym("to_list"), def_id: self.facts.def_id("map.to_list") },
            args: vec![e_(IrExprKind::Var { id: m }, map_ty.clone())], type_args: vec![],
        }, pairs_ty.clone());
        let is_empty = e_(IrExprKind::Call {
            target: CallTarget::Module { module: sym("map"), func: sym("is_empty"), def_id: self.facts.def_id("map.is_empty") },
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
            target: CallTarget::Module { module: sym("list"), func: sym("len"), def_id: self.facts.def_id("list.len") },
            args: vec![var(xs, &pairs_ty)], type_args: vec![],
        }, Ty::Int);
        let cond = e_(IrExprKind::BinOp { op: BinOp::Lt, left: Box::new(var(i, &Ty::Int)), right: Box::new(len) }, Ty::Bool);
        let next_i = e_(IrExprKind::BinOp { op: BinOp::AddInt, left: Box::new(var(i, &Ty::Int)), right: Box::new(e_(IrExprKind::LitInt { value: 1 }, Ty::Int)) }, Ty::Int);
        let go_body = e_(IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(e_(IrExprKind::Call { target: CallTarget::Named { name: pairs_name }, args: vec![var(xs, &pairs_ty), next_i, next_acc], type_args: vec![] }, Ty::String)),
            else_: Box::new(var(acc, &Ty::String)),
        }, Ty::String);
        self.out.push(mk_fn(self.var_table, name, vec![(m, map_ty.clone())], entry));
        self.out.push(mk_fn(self.var_table, pairs_name, vec![(xs, pairs_ty), (i, Ty::Int), (acc, Ty::String)], go_body));
        name
    }
}

/// Does a value of `ty` contain, anywhere, a value whose type declares its
/// repr? `seen` breaks recursive types.
pub fn holds_declared_repr(facts: &dyn ReprFacts, ty: &Ty, seen: &mut Vec<Ty>) -> bool {
    if seen.contains(ty) {
        return false;
    }
    match ty {
        Ty::Named(..) => {
            if facts.repr_target(ty).is_some() {
                return true;
            }
            seen.push(ty.clone());
            let found = match facts.unfold_named(ty) {
                Some(Ty::Record { fields }) | Some(Ty::OpenRecord { fields }) =>
                    fields.iter().any(|(_, t)| holds_declared_repr(facts, t, seen)),
                Some(Ty::Variant { cases, .. }) => cases.iter().any(|c| match &c.payload {
                    VariantPayload::Unit => false,
                    VariantPayload::Tuple(ts) => ts.iter().any(|t| holds_declared_repr(facts, t, seen)),
                    VariantPayload::Record(fs) => fs.iter().any(|(_, t)| holds_declared_repr(facts, t, seen)),
                }),
                _ => false,
            };
            seen.pop();
            found
        }
        Ty::Applied(_, args) => args.iter().any(|a| holds_declared_repr(facts, a, seen)),
        Ty::Tuple(ts) => ts.iter().any(|t| holds_declared_repr(facts, t, seen)),
        Ty::Record { fields } | Ty::OpenRecord { fields } =>
            fields.iter().any(|(_, t)| holds_declared_repr(facts, t, seen)),
        _ => false,
    }
}

fn mentions_type_var(ty: &Ty) -> bool {
    matches!(ty, Ty::TypeVar(_)) || ty.children().iter().any(|c| mentions_type_var(c))
}

/// Re-applies the renderer rule to every `${…}` segment that still carries
/// a value whose type holds a declared repr (#4239). Run after
/// monomorphization has specialized every generic body: a segment the
/// lowering saw as `A` is now `Secret[String]` (or `List[Secret[..]]`, a
/// tuple, an option, a record holding one), and it renders through the
/// declaration exactly as the same segment written with the concrete type
/// does. A segment the lowering already routed is a `String` call and is
/// left alone, so the pass is idempotent. Generic fns (about to be pruned)
/// and segments still naming a type variable are skipped.
///
/// Returns whether anything changed — the new calls name the declared reprs
/// (themselves generic module fns), which the caller then specializes.
pub fn route_instantiated_segments(program: &mut IrProgram) -> bool {
    let table = std::mem::take(&mut program.declared_reprs);
    if table.targets.is_empty() {
        program.declared_reprs = table;
        return false;
    }
    let mut changed = false;
    {
        let IrProgram { functions, var_table, modules, used_stdlib_modules, .. } = program;
        changed |= route_unit(&table, functions, var_table, String::new(), used_stdlib_modules);
        for m in modules.iter_mut() {
            let scope = format!("{}_", m.name.as_str().replace('.', "_"));
            changed |= route_unit(&table, &mut m.functions, &mut m.var_table, scope, used_stdlib_modules);
        }
    }
    program.declared_reprs = table;
    changed
}

fn route_unit(
    table: &DeclaredReprTable,
    functions: &mut Vec<IrFunction>,
    var_table: &mut VarTable,
    scope: String,
    used_stdlib_modules: &mut std::collections::HashSet<String>,
) -> bool {
    let mut pending: Vec<Sym> = functions.iter()
        .filter(|f| f.name.as_str().starts_with("__almd_repr_declared_"))
        .map(|f| f.name)
        .collect();
    let mut out: Vec<IrFunction> = Vec::new();
    let mut changed = false;
    for f in functions.iter_mut() {
        if f.generics.as_ref().is_some_and(|g| !g.is_empty()) {
            continue;
        }
        let mut router = SegmentRouter {
            synth: ReprSynth { facts: table, var_table, out: &mut out, pending: &mut pending, scope: scope.clone() },
            changed: false,
        };
        router.visit_expr_mut(&mut f.body);
        changed |= router.changed;
    }
    // The walkers call stdlib modules (`list.len`, `set.to_list`, …) the
    // program may not have used: read the modules off the calls they make.
    let mut used = ModulesCalled(std::collections::BTreeSet::new());
    for f in &out {
        used.visit_expr(&f.body);
    }
    used_stdlib_modules.extend(used.0.into_iter().map(|m| m.as_str().to_string()));
    functions.extend(out);
    changed
}

/// The modules an expression calls into.
struct ModulesCalled(std::collections::BTreeSet<Sym>);

impl IrVisitor for ModulesCalled {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExprKind::Call { target: CallTarget::Module { module, .. }, .. } = &expr.kind {
            self.0.insert(*module);
        }
        walk_expr(self, expr);
    }
}

struct SegmentRouter<'a> {
    synth: ReprSynth<'a>,
    changed: bool,
}

impl IrMutVisitor for SegmentRouter<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        walk_expr_mut(self, expr);
        let IrExprKind::StringInterp { parts } = &mut expr.kind else { return };
        for part in parts.iter_mut() {
            let IrStringPart::Expr { expr: seg } = part else { continue };
            if matches!(seg.ty, Ty::String) || mentions_type_var(&seg.ty) {
                continue;
            }
            if self.synth.facts.repr_target(&seg.ty).is_none() && !self.synth.holds_declared_repr(&seg.ty, &mut Vec::new()) {
                continue;
            }
            let taken = std::mem::replace(seg, e_(IrExprKind::Unit, Ty::Unit));
            *seg = self.synth.render_declared(taken);
            self.changed = true;
        }
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
