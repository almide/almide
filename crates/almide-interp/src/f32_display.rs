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

    /// Does `ty` hold a Float32 anywhere a display would reach it? `depth`
    /// bounds a walk through recursive declarations.
    pub(crate) fn mentions_f32(&self, ty: &Ty, depth: u32) -> bool {
        if depth > 8 {
            return false;
        }
        match ty {
            Ty::Float32 => true,
            Ty::Applied(_, args) => args.iter().any(|t| self.mentions_f32(t, depth + 1)),
            Ty::Tuple(ts) => ts.iter().any(|t| self.mentions_f32(t, depth + 1)),
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                fields.iter().any(|(_, t)| self.mentions_f32(t, depth + 1))
            }
            Ty::Named(n, _) => match self.decl_named(*n).map(|d| &d.kind) {
                Some(IrTypeDeclKind::Record { fields }) => {
                    fields.iter().any(|f| self.mentions_f32(&f.ty, depth + 1))
                }
                Some(IrTypeDeclKind::Variant { cases, .. }) => cases.iter().any(|c| match &c.kind {
                    IrVariantKind::Unit => false,
                    IrVariantKind::Tuple { fields } => fields.iter().any(|t| self.mentions_f32(t, depth + 1)),
                    IrVariantKind::Record { fields } => {
                        fields.iter().any(|f| self.mentions_f32(&f.ty, depth + 1))
                    }
                }),
                Some(IrTypeDeclKind::Alias { target }) => self.mentions_f32(target, depth + 1),
                None => false,
            },
            _ => false,
        }
    }

    /// `v` with every Float32 leaf (per `ty`) replaced by the f64 that
    /// displays as the binary32's digits. A shape that does not line up with
    /// the type is left as it is.
    pub(crate) fn f32_display_view(&self, v: &Value, ty: &Ty) -> Value {
        match (v, ty) {
            (Value::Float(f), Ty::Float32) => Value::Float(f32_leaf(*f)),
            (Value::List(xs), Ty::Applied(..)) | (Value::Set(xs), Ty::Applied(..)) => {
                let Some(t) = arg(ty, 0) else { return v.clone() };
                let out: Vec<Value> = xs.iter().map(|x| self.f32_display_view(x, t)).collect();
                if matches!(v, Value::Set(_)) {
                    Value::Set(out.into())
                } else {
                    Value::List(out.into())
                }
            }
            (Value::Option(Some(x)), Ty::Applied(..)) => match arg(ty, 0) {
                Some(t) => Value::Option(Some(Box::new(self.f32_display_view(x, t)))),
                None => v.clone(),
            },
            (Value::Result(r), Ty::Applied(..)) => match r {
                Ok(x) => match arg(ty, 0) {
                    Some(t) => Value::Result(Ok(Box::new(self.f32_display_view(x, t)))),
                    None => v.clone(),
                },
                Err(x) => match arg(ty, 1) {
                    Some(t) => Value::Result(Err(Box::new(self.f32_display_view(x, t)))),
                    None => v.clone(),
                },
            },
            (Value::Map(kvs), Ty::Applied(..)) => match (arg(ty, 0), arg(ty, 1)) {
                (Some(kt), Some(vt)) => Value::Map(
                    kvs.iter()
                        .map(|(k, x)| (self.f32_display_view(k, kt), self.f32_display_view(x, vt)))
                        .collect::<Vec<_>>()
                        .into(),
                ),
                _ => v.clone(),
            },
            (Value::Tuple(xs), Ty::Tuple(ts)) if xs.len() == ts.len() => Value::Tuple(
                xs.iter().zip(ts).map(|(x, t)| self.f32_display_view(x, t)).collect::<Vec<_>>().into(),
            ),
            (Value::Record { name, fields }, Ty::Record { fields: fts } | Ty::OpenRecord { fields: fts }) => {
                Value::Record { name: *name, fields: self.view_fields(fields, |n| fts.iter().find(|(f, _)| *f == n).map(|(_, t)| t.clone())) }
            }
            (Value::Record { name, fields }, Ty::Named(n, _)) => match self.decl_named(*n).map(|d| &d.kind) {
                Some(IrTypeDeclKind::Record { fields: decl }) => Value::Record {
                    name: *name,
                    fields: self.view_fields(fields, |f| decl.iter().find(|d| d.name == f).map(|d| d.ty.clone())),
                },
                _ => v.clone(),
            },
            (Value::Variant { ty: vty, ctor, payload }, Ty::Named(n, _)) => {
                let Some(IrTypeDeclKind::Variant { cases, .. }) = self.decl_named(*n).map(|d| &d.kind) else {
                    return v.clone();
                };
                let Some(case) = cases.iter().find(|c| c.name == *ctor) else { return v.clone() };
                let payload = match (payload, &case.kind) {
                    (VariantPayload::Tuple(xs), IrVariantKind::Tuple { fields }) if xs.len() == fields.len() => {
                        VariantPayload::Tuple(xs.iter().zip(fields).map(|(x, t)| self.f32_display_view(x, t)).collect())
                    }
                    (VariantPayload::Record(fs), IrVariantKind::Record { fields }) => VariantPayload::Record(
                        fs.iter()
                            .map(|(f, x)| match fields.iter().find(|d| d.name == *f) {
                                Some(d) => (*f, self.f32_display_view(x, &d.ty)),
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
                Some(t) => (*n, self.f32_display_view(x, &t)),
                None => (*n, x.clone()),
            })
            .collect::<Vec<_>>()
            .into()
    }
}
