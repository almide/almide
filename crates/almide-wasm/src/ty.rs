//! Ty -> SliceTy classification + reason-string helpers — split from
//! lib.rs for the complexity budget.

use almide_types::types::{Ty, TypeConstructorId};

use crate::types_table::TypeTable;
use crate::*;

pub(crate) fn scalar_of(ty: &Ty) -> Option<Scalar> {
    match ty {
        // Sized integers share the ONE i64 slot (the interp's doctrine):
        // the declared width lives on the expression types and re-wraps
        // arithmetic (C-180), reads division/ordering unsigned for
        // UInt64 (C-179), and traps the narrow MIN/-1 (C-002) — all in
        // the binop lowering, none in the layout.
        Ty::Int
        | Ty::Int64
        | Ty::Int8
        | Ty::Int16
        | Ty::Int32
        | Ty::UInt8
        | Ty::UInt16
        | Ty::UInt32
        | Ty::UInt64 => Some(Scalar::Int),
        Ty::Float | Ty::Float64 => Some(Scalar::Float),
        Ty::Bool => Some(Scalar::Bool),
        Ty::String => Some(Scalar::Str),
        Ty::Bytes => Some(Scalar::Bytes),
        _ => None,
    }
}

pub(crate) fn slice_ty_of(ty: &Ty, types: &TypeTable) -> Option<SliceTy> {
    if let Some(s) = scalar_of(ty) {
        return Some(SliceTy::Scalar(s));
    }
    match ty {
        Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => {
            let e = slice_ty_of(&args[0], types)?;
            Some(SliceTy::Option(types.intern(e)))
        }
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
            let o = slice_ty_of(&args[0], types)?;
            let e = slice_ty_of(&args[1], types)?;
            Some(SliceTy::Result(types.intern(o), types.intern(e)))
        }
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            let e = slice_ty_of(&args[0], types)?;
            Some(SliceTy::List(types.intern(e)))
        }
        Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => {
            let k = slice_ty_of(&args[0], types)?;
            // Keys need defined equality — scalars only.
            let SliceTy::Scalar(_) = k else { return None };
            let v = slice_ty_of(&args[1], types)?;
            Some(SliceTy::Map(types.intern(k), types.intern(v)))
        }
        Ty::Applied(TypeConstructorId::Set, args) if args.len() == 1 => {
            let e = slice_ty_of(&args[0], types)?;
            let SliceTy::Scalar(_) = e else { return None };
            Some(SliceTy::Set(types.intern(e)))
        }
        Ty::Tuple(args) => {
            let mut elems = Vec::new();
            for a in args {
                elems.push(slice_ty_of(a, types)?);
            }
            Some(SliceTy::Tuple(types.tuple(elems)))
        }
        Ty::Record { fields } => types.anon_record(fields).map(SliceTy::Named),
        Ty::Unit => Some(SliceTy::Unit),
        Ty::Matrix => Some(SliceTy::Matrix),
        Ty::Fn { params, ret, is_effect } => {
            let mut ps = Vec::new();
            for p in params {
                ps.push(slice_ty_of(p, types)?);
            }
            let r = match (&**ret, *is_effect) {
                (Ty::Unit, false) => None,
                // An effect-Unit slot needs a Unit repr — not yet.
                (Ty::Unit, true) => return None,
                (t, eff) => {
                    let sty = slice_ty_of(t, types)?;
                    Some(match (sty, eff) {
                        // Declared-Result slots are single-layer (probe-
                        // settled, same rule as effect fns).
                        (rs @ SliceTy::Result(..), _) => rs,
                        (sty, true) => {
                            SliceTy::Result(types.intern(sty), types.intern(STR))
                        }
                        (sty, false) => sty,
                    })
                }
            };
            Some(SliceTy::Fn(types.fn_sig(crate::types_table::FnSig {
                params: ps,
                ret: r,
                effect: *is_effect,
            })))
        }
        Ty::Named(name, args) if args.is_empty() => {
            // A user declaration wins; the builtin dynamic Value is the
            // fallback for the undeclared opaque name.
            types.by_name.get(name.as_str()).map(|&i| SliceTy::Named(i)).or_else(|| {
                (name.as_str() == "Value").then_some(SliceTy::Value)
            })
        }
        Ty::Named(name, args) => types.instance(name.as_str(), args).map(SliceTy::Named),
        _ => None,
    }
}
