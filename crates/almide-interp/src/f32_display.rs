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
use almide_ir::{IrProgram, IrTypeDecl, IrTypeDeclKind, IrVariantKind};
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

/// The declaration named `name`, in the program or any of its modules.
fn decl_named(program: &IrProgram, name: Sym) -> Option<&IrTypeDecl> {
    program
        .type_decls
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
        .find(|d| d.name == name)
}

/// The parameter environment of the generic instance `Named(n, args)`
/// (empty for a non-generic declaration).
fn instance_env(decl: &IrTypeDecl, args: &[Ty]) -> HashMap<Sym, Ty> {
    match &decl.generics {
        Some(ps) if ps.len() == args.len() => ps.iter().map(|p| p.name).zip(args.iter().cloned()).collect(),
        _ => HashMap::new(),
    }
}

/// Does `ty` hold a leaf `is_leaf` accepts anywhere a display would reach
/// it? `depth` bounds a walk through recursive declarations.
fn mentions(program: &IrProgram, ty: &Ty, depth: u32, is_leaf: &dyn Fn(&Ty) -> bool) -> bool {
    if depth > 8 {
        return false;
    }
    if is_leaf(ty) {
        return true;
    }
    let deeper = |t: &Ty| mentions(program, t, depth + 1, is_leaf);
    match ty {
        Ty::Applied(_, args) => args.iter().any(deeper),
        Ty::Tuple(ts) => ts.iter().any(deeper),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| deeper(t)),
        Ty::Named(n, args) => {
            args.iter().any(deeper)
                || match decl_named(program, *n).map(|d| &d.kind) {
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

/// A display-only leaf hook: `Some(view)` replaces the value at a leaf,
/// `None` walks on into it.
type Leaf<'l> = &'l mut dyn FnMut(&Value, &Ty) -> Option<Value>;

/// `v` with every leaf `leaf` answers (per `ty`) replaced by its view. A
/// shape that does not line up with the type is left as it is.
fn display_view(program: &IrProgram, v: &Value, ty: &Ty, leaf: Leaf<'_>) -> Value {
    if let Some(view) = leaf(v, ty) {
        return view;
    }
    match (v, ty) {
        (Value::List(xs), Ty::Applied(..)) | (Value::Set(xs), Ty::Applied(..)) => {
            let Some(t) = arg(ty, 0) else { return v.clone() };
            let out: Vec<Value> = xs.iter().map(|x| display_view(program, x, t, leaf)).collect();
            if matches!(v, Value::Set(_)) {
                Value::Set(out.into())
            } else {
                Value::List(out.into())
            }
        }
        (Value::Option(Some(x)), Ty::Applied(..)) => match arg(ty, 0) {
            Some(t) => Value::Option(Some(Box::new(display_view(program, x, t, leaf)))),
            None => v.clone(),
        },
        (Value::Result(r), Ty::Applied(..)) => match r {
            Ok(x) => match arg(ty, 0) {
                Some(t) => Value::Result(Ok(Box::new(display_view(program, x, t, leaf)))),
                None => v.clone(),
            },
            Err(x) => match arg(ty, 1) {
                Some(t) => Value::Result(Err(Box::new(display_view(program, x, t, leaf)))),
                None => v.clone(),
            },
        },
        (Value::Map(kvs), Ty::Applied(..)) => match (arg(ty, 0), arg(ty, 1)) {
            (Some(kt), Some(vt)) => Value::Map(
                kvs.iter()
                    .map(|(k, x)| (display_view(program, k, kt, leaf), display_view(program, x, vt, leaf)))
                    .collect::<Vec<_>>()
                    .into(),
            ),
            _ => v.clone(),
        },
        (Value::Tuple(xs), Ty::Tuple(ts)) if xs.len() == ts.len() => Value::Tuple(
            xs.iter().zip(ts).map(|(x, t)| display_view(program, x, t, leaf)).collect::<Vec<_>>().into(),
        ),
        (Value::Record { name, fields }, Ty::Record { fields: fts } | Ty::OpenRecord { fields: fts }) => Value::Record {
            name: *name,
            fields: view_fields(program, fields, |n| fts.iter().find(|(f, _)| *f == n).map(|(_, t)| t.clone()), leaf),
        },
        (Value::Record { name, fields }, Ty::Named(n, args)) => match decl_named(program, *n) {
            Some(d @ IrTypeDecl { kind: IrTypeDeclKind::Record { fields: decl }, .. }) => {
                let env = instance_env(d, args);
                Value::Record {
                    name: *name,
                    fields: view_fields(
                        program,
                        fields,
                        |f| decl.iter().find(|d| d.name == f).map(|d| subst(&d.ty, &env)),
                        leaf,
                    ),
                }
            }
            _ => v.clone(),
        },
        (Value::Variant { ty: vty, ctor, payload }, Ty::Named(n, args)) => {
            let Some(d @ IrTypeDecl { kind: IrTypeDeclKind::Variant { cases, .. }, .. }) = decl_named(program, *n)
            else {
                return v.clone();
            };
            let env = instance_env(d, args);
            let Some(case) = cases.iter().find(|c| c.name == *ctor) else { return v.clone() };
            let payload = match (payload, &case.kind) {
                (VariantPayload::Tuple(xs), IrVariantKind::Tuple { fields }) if xs.len() == fields.len() => {
                    VariantPayload::Tuple(
                        xs.iter().zip(fields).map(|(x, t)| display_view(program, x, &subst(t, &env), leaf)).collect(),
                    )
                }
                (VariantPayload::Record(fs), IrVariantKind::Record { fields }) => VariantPayload::Record(
                    fs.iter()
                        .map(|(f, x)| match fields.iter().find(|d| d.name == *f) {
                            Some(d) => (*f, display_view(program, x, &subst(&d.ty, &env), leaf)),
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
    program: &IrProgram,
    fields: &std::rc::Rc<Vec<(Sym, Value)>>,
    field_ty: impl Fn(Sym) -> Option<Ty>,
    leaf: Leaf<'_>,
) -> std::rc::Rc<Vec<(Sym, Value)>> {
    fields
        .iter()
        .map(|(n, x)| match field_ty(*n) {
            Some(t) => (*n, display_view(program, x, &t, leaf)),
            None => (*n, x.clone()),
        })
        .collect::<Vec<_>>()
        .into()
}

/// The Float32 / UInt64 leaves (C-372, C-179).
fn sized_leaf(v: &Value, ty: &Ty) -> Option<Value> {
    match (v, ty) {
        (Value::Float(f), Ty::Float32) => Some(Value::Float(f32_leaf(*f))),
        (Value::Int(n), Ty::UInt64) => Some(u64_leaf(*n).unwrap_or_else(|| v.clone())),
        _ => None,
    }
}

/// The declared repr surface of a runtime-backed leaf type (#4204).
fn declared_repr_of(program: &IrProgram, ty: &Ty) -> Option<String> {
    let Ty::Named(n, args) = ty else { return None };
    if !args.is_empty() || decl_named(program, *n).is_some() {
        return None;
    }
    almide_lang::stdlib_info::runtime_backed_repr_surface(n.as_str())
}

impl Interpreter<'_> {
    /// Does `ty` hold a Float32 or a UInt64 anywhere a display would reach
    /// it? `depth` bounds a walk through recursive declarations.
    pub(crate) fn mentions_sized_display(&self, ty: &Ty, depth: u32) -> bool {
        mentions(self.program, ty, depth, &|t| matches!(t, Ty::Float32 | Ty::UInt64))
    }

    /// `v` with every Float32 leaf (per `ty`) replaced by the f64 that
    /// displays as the binary32's digits, and every upper-half UInt64 leaf by
    /// its unsigned digits. A shape that does not line up with the type is
    /// left as it is.
    pub(crate) fn sized_display_view(&self, v: &Value, ty: &Ty) -> Value {
        display_view(self.program, v, ty, &mut sized_leaf)
    }

    /// Does `ty` hold a runtime-backed value whose module declares its repr
    /// (`http.__http_request_repr`, #4204) anywhere a display would reach it?
    pub(crate) fn mentions_declared_repr(&self, ty: &Ty) -> bool {
        let program = self.program;
        mentions(program, ty, 0, &|t| declared_repr_of(program, t).is_some())
    }

    /// `v` with every runtime-backed leaf replaced by its declared repr's
    /// text, as a display-only atom (the UInt64 leaf's form: a unit variant
    /// whose name is the text, so it prints the same bare and nested). The
    /// repr is the module's own function, run as any call to it would be —
    /// the self-hosted body the wasm leg links. A repr call that does not
    /// answer a String ends the display with its flow (an abstention stays
    /// an abstention).
    pub(crate) fn declared_repr_view(&mut self, v: &Value, ty: &Ty) -> Result<Value, crate::Flow> {
        let program = self.program;
        let mut failed: Option<crate::Flow> = None;
        let mut leaf = |x: &Value, t: &Ty| -> Option<Value> {
            let surface = declared_repr_of(program, t)?;
            if failed.is_some() {
                return Some(x.clone());
            }
            let (m, f) = surface.split_once('.')?;
            match self.dispatch_module_resolved(sym(m), sym(f), vec![x.clone()]) {
                crate::Flow::Value(Value::Str(s)) => Some(Value::Variant {
                    ty: Some(*n_of(t)?),
                    ctor: sym(&s),
                    payload: VariantPayload::Unit,
                }),
                crate::Flow::Value(_) => {
                    failed = Some(crate::Flow::Unsupported(format!("{surface} answered a non-String")));
                    Some(x.clone())
                }
                other => {
                    failed = Some(other);
                    Some(x.clone())
                }
            }
        };
        let out = display_view(program, v, ty, &mut leaf);
        match failed {
            Some(flow) => Err(flow),
            None => Ok(out),
        }
    }
}

fn n_of(ty: &Ty) -> Option<&Sym> {
    match ty {
        Ty::Named(n, _) => Some(n),
        _ => None,
    }
}
