//! Float32 display (C-372, #3081).
//!
//! The interpreter carries a Float32 on the widened f64 carrier, so a
//! `Value::Float` cannot say whether it prints as a Float or as a Float32 —
//! and the two differ (`0.1f32` is `0.1`, its widened f64 `0.10000000149011612`).
//! The IR type of the displayed expression can, at any depth: this walk takes
//! the value and its type and replaces every Float32 leaf with the f64 whose
//! own shortest digits ARE the binary32's shortest digits — the f64 nearest to
//! the f32 Display text. The display path (`display_bare` / the repr) is then
//! unchanged. That substitution is exact: for every one of the 2^32 binary32
//! patterns, f64 Display of `f32_display.parse::<f64>()` equals the f32
//! Display (checked exhaustively, 2026-09-30). Non-finite values print their
//! names either way.
//!
//! A UInt64 has the same problem on the i64 carrier (C-179, #3187): a value
//! in the upper half is a negative `Value::Int`, and only the type says it
//! reads unsigned. Its leaf becomes a display-only atom that prints the
//! unsigned digits bare at the top and nested alike (a unit-shaped variant
//! named by the digits — the one `Value` whose repr is its own name). The
//! view is built for one display and dropped; nothing compares or stores it.
//!
//! A GENERIC declaration's fields are spelled with its parameters (`T`), so
//! the walk substitutes the instance's arguments before it reads them
//! (`Box[UInt64]`, `Tree[Float32]`).

use std::collections::HashMap;

use almide_base::intern::{sym, Sym};
use almide_ir::{IrTypeDecl, IrTypeDeclKind, IrVariantKind};
use almide_lang::types::Ty;

use crate::value::{Value, VariantPayload};
use crate::Interpreter;

/// The f64 that prints as `g`'s binary32 Display (see the module doc).
fn f32_leaf(f: f64) -> f64 {
    let g = f as f32;
    if !g.is_finite() {
        return g as f64;
    }
    format!("{g}").parse::<f64>().unwrap_or(g as f64)
}

/// A UInt64 leaf's display: the slot's bits read unsigned. A non-negative
/// slot prints as it is; only the upper half needs the atom.
fn u64_leaf(n: i64) -> Option<Value> {
    (n < 0).then(|| Value::Variant {
        ty: Some(sym("UInt64")),
        ctor: sym(&(n as u64).to_string()),
        payload: VariantPayload::Unit,
    })
}

/// `ty` with the declaration's parameters replaced by the instance's
/// arguments. A parameter arrives as a bare `Named("T", [])` (or a TypeVar).
fn subst(ty: &Ty, env: &HashMap<Sym, Ty>) -> Ty {
    match ty {
        Ty::TypeVar(n) if env.contains_key(n) => env[n].clone(),
        Ty::Named(n, a) if a.is_empty() && env.contains_key(n) => env[n].clone(),
        _ => ty.map_children(&|c| subst(c, env)),
    }
}

fn arg(ty: &Ty, i: usize) -> Option<&Ty> {
    match ty {
        Ty::Applied(_, args) => args.get(i),
        _ => None,
    }
}

impl Interpreter<'_> {
    fn decl_named(&self, name: almide_base::intern::Sym) -> Option<&IrTypeDecl> {
        self.program
            .type_decls
            .iter()
            .chain(self.program.modules.iter().flat_map(|m| m.type_decls.iter()))
            .find(|d| d.name == name)
    }

    /// The parameter environment of the generic instance `Named(n, args)`
    /// (empty for a non-generic declaration).
    fn instance_env(&self, decl: &IrTypeDecl, args: &[Ty]) -> HashMap<Sym, Ty> {
        match &decl.generics {
            Some(ps) if ps.len() == args.len() => ps.iter().map(|p| p.name).zip(args.iter().cloned()).collect(),
            _ => HashMap::new(),
        }
    }

    /// Does `ty` hold a Float32 or a UInt64 anywhere a display would reach
    /// it? `depth` bounds a walk through recursive declarations.
    pub(crate) fn mentions_sized_display(&self, ty: &Ty, depth: u32) -> bool {
        if depth > 8 {
            return false;
        }
        let deeper = |t: &Ty| self.mentions_sized_display(t, depth + 1);
        match ty {
            Ty::Float32 | Ty::UInt64 => true,
            Ty::Applied(_, args) => args.iter().any(deeper),
            Ty::Tuple(ts) => ts.iter().any(deeper),
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| deeper(t)),
            Ty::Named(n, args) => {
                args.iter().any(deeper)
                    || match self.decl_named(*n).map(|d| &d.kind) {
                        Some(IrTypeDeclKind::Record { fields }) => fields.iter().any(|f| deeper(&f.ty)),
                        Some(IrTypeDeclKind::Variant { cases, .. }) => cases.iter().any(|c| match &c.kind {
                            IrVariantKind::Unit => false,
                            IrVariantKind::Tuple { fields } => fields.iter().any(deeper),
                            IrVariantKind::Record { fields } => fields.iter().any(|f| deeper(&f.ty)),
                        }),
                        Some(IrTypeDeclKind::Alias { target }) => deeper(target),
                        None => false,
                    }
            }
            _ => false,
        }
    }

    /// `v` with every Float32 leaf (per `ty`) replaced by the f64 that
    /// displays as the binary32's digits, and every upper-half UInt64 leaf by
    /// its unsigned digits. A shape that does not line up with the type is
    /// left as it is.
    pub(crate) fn sized_display_view(&self, v: &Value, ty: &Ty) -> Value {
        match (v, ty) {
            (Value::Float(f), Ty::Float32) => Value::Float(f32_leaf(*f)),
            (Value::Int(n), Ty::UInt64) => u64_leaf(*n).unwrap_or_else(|| v.clone()),
            (Value::List(xs), Ty::Applied(..)) | (Value::Set(xs), Ty::Applied(..)) => {
                let Some(t) = arg(ty, 0) else { return v.clone() };
                let out: Vec<Value> = xs.iter().map(|x| self.sized_display_view(x, t)).collect();
                if matches!(v, Value::Set(_)) {
                    Value::Set(out.into())
                } else {
                    Value::List(out.into())
                }
            }
            (Value::Option(Some(x)), Ty::Applied(..)) => match arg(ty, 0) {
                Some(t) => Value::Option(Some(Box::new(self.sized_display_view(x, t)))),
                None => v.clone(),
            },
            (Value::Result(r), Ty::Applied(..)) => match r {
                Ok(x) => match arg(ty, 0) {
                    Some(t) => Value::Result(Ok(Box::new(self.sized_display_view(x, t)))),
                    None => v.clone(),
                },
                Err(x) => match arg(ty, 1) {
                    Some(t) => Value::Result(Err(Box::new(self.sized_display_view(x, t)))),
                    None => v.clone(),
                },
            },
            (Value::Map(kvs), Ty::Applied(..)) => match (arg(ty, 0), arg(ty, 1)) {
                (Some(kt), Some(vt)) => Value::Map(
                    kvs.iter()
                        .map(|(k, x)| (self.sized_display_view(k, kt), self.sized_display_view(x, vt)))
                        .collect::<Vec<_>>()
                        .into(),
                ),
                _ => v.clone(),
            },
            (Value::Tuple(xs), Ty::Tuple(ts)) if xs.len() == ts.len() => Value::Tuple(
                xs.iter().zip(ts).map(|(x, t)| self.sized_display_view(x, t)).collect::<Vec<_>>().into(),
            ),
            (Value::Record { name, fields }, Ty::Record { fields: fts } | Ty::OpenRecord { fields: fts }) => {
                Value::Record { name: *name, fields: self.view_fields(fields, |n| fts.iter().find(|(f, _)| *f == n).map(|(_, t)| t.clone())) }
            }
            (Value::Record { name, fields }, Ty::Named(n, args)) => match self.decl_named(*n) {
                Some(d @ IrTypeDecl { kind: IrTypeDeclKind::Record { fields: decl }, .. }) => {
                    let env = self.instance_env(d, args);
                    Value::Record {
                        name: *name,
                        fields: self.view_fields(fields, |f| {
                            decl.iter().find(|d| d.name == f).map(|d| subst(&d.ty, &env))
                        }),
                    }
                }
                _ => v.clone(),
            },
            (Value::Variant { ty: vty, ctor, payload }, Ty::Named(n, args)) => {
                let Some(d @ IrTypeDecl { kind: IrTypeDeclKind::Variant { cases, .. }, .. }) = self.decl_named(*n)
                else {
                    return v.clone();
                };
                let env = self.instance_env(d, args);
                let Some(case) = cases.iter().find(|c| c.name == *ctor) else { return v.clone() };
                let payload = match (payload, &case.kind) {
                    (VariantPayload::Tuple(xs), IrVariantKind::Tuple { fields }) if xs.len() == fields.len() => {
                        VariantPayload::Tuple(
                            xs.iter().zip(fields).map(|(x, t)| self.sized_display_view(x, &subst(t, &env))).collect(),
                        )
                    }
                    (VariantPayload::Record(fs), IrVariantKind::Record { fields }) => VariantPayload::Record(
                        fs.iter()
                            .map(|(f, x)| match fields.iter().find(|d| d.name == *f) {
                                Some(d) => (*f, self.sized_display_view(x, &subst(&d.ty, &env))),
                                None => (*f, x.clone()),
                            })
                            .collect(),
                    ),
                    (p, _) => p.clone(),
                };
                Value::Variant { ty: *vty, ctor: *ctor, payload }
            }
            _ => v.clone(),
        }
    }

    fn view_fields(
        &self,
        fields: &std::rc::Rc<Vec<(almide_base::intern::Sym, Value)>>,
        field_ty: impl Fn(almide_base::intern::Sym) -> Option<Ty>,
    ) -> std::rc::Rc<Vec<(almide_base::intern::Sym, Value)>> {
        fields
            .iter()
            .map(|(n, x)| match field_ty(*n) {
                Some(t) => (*n, self.sized_display_view(x, &t)),
                None => (*n, x.clone()),
            })
            .collect::<Vec<_>>()
            .into()
    }
}
