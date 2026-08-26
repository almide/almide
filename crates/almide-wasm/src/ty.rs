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
        // RawPtr is an ADDRESS scalar (an i64-carried linear-memory
        // offset; the identity prim casts move it to/from Int).
        Ty::RawPtr => Some(Scalar::Int),
        // Float32 rides the WIDENED f64 carrier (the interp's doctrine):
        // literals narrow at birth; the slot always holds an
        // f32-representable f64.
        Ty::Float | Ty::Float64 | Ty::Float32 => Some(Scalar::Float),
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
    if let Ty::Applied(ctor, args) = ty
        && !matches!(ctor, TypeConstructorId::UserDefined(_))
    {
        return applied_builtin_of(ctor, args, types);
    }
    match ty {
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
            // fallback for the undeclared opaque name; the PUBLISHED
            // newtype erasures (self-host-owned reps) come last.
            types.by_name.get(name.as_str()).map(|&i| SliceTy::Named(i)).or_else(|| {
                (name.as_str() == "Value").then_some(SliceTy::Value)
            }).or_else(|| match name.as_str() {
                // stdlib/http_response.almd / json_path.almd own these
                // reps; the eraser publishes them as List[String].
                "HttpResponse" | "JsonPath" => {
                    Some(SliceTy::List(types.intern(SliceTy::Scalar(Scalar::Str))))
                }
                _ => None,
            }).or_else(|| {
                // A BARE spelling of a module-declared type (the front
                // qualifies the decl `m.Box` but a convention method's
                // receiver says `Box`): unique-suffix match, ambiguity
                // stays None — the same unique-or-wall doctrine as the
                // cross-module method resolver.
                named_suffix_unique(types, name.as_str())
            })
        }
        Ty::Named(name, args) => types.instance(name.as_str(), args).map(SliceTy::Named),
        // Generic user types arrive from the checker as Applied(UserDefined)
        // — the same instance machinery the Named spelling routes through.
        Ty::Applied(TypeConstructorId::UserDefined(name), args) => {
            if args.is_empty() {
                types.by_name.get(name.as_str()).map(|&i| SliceTy::Named(i)).or_else(|| {
                    (name.as_str() == "Value").then_some(SliceTy::Value)
                })
            } else {
                types.instance(name.as_str(), args).map(SliceTy::Named)
            }
        }
        // A TypeVar/Unknown SURVIVING inference is unconstrained — the
        // checker's #1428 doctrine defaults it to Unit (native spells
        // the same resolution as a `::<(), _>` turbofish).
        Ty::TypeVar(_) | Ty::Unknown => Some(SliceTy::Unit),
        _ => None,
    }
}


/// The builtin container constructors (Option/Result/List/Map/Set) —
/// split from slice_ty_of for the complexity budget.
fn applied_builtin_of(
    ctor: &TypeConstructorId,
    args: &[Ty],
    types: &TypeTable,
) -> Option<SliceTy> {
    match (ctor, args) {
        (TypeConstructorId::Option, [a]) => {
            let e = slice_ty_of(a, types)?;
            Some(SliceTy::Option(types.intern(e)))
        }
        (TypeConstructorId::Result, [a, b]) => {
            let o = slice_ty_of(a, types)?;
            let e = slice_ty_of(b, types)?;
            Some(SliceTy::Result(types.intern(o), types.intern(e)))
        }
        (TypeConstructorId::List, [a]) => {
            let e = slice_ty_of(a, types)?;
            Some(SliceTy::List(types.intern(e)))
        }
        (TypeConstructorId::Map, [a, b]) => {
            let k = slice_ty_of(a, types)?;
            // Keys need defined equality: scalars via the word/byte
            // scans, tuples/records via the deep-scan lane.
            if !matches!(k, SliceTy::Scalar(_) | SliceTy::Tuple(_) | SliceTy::Named(_)) {
                return None;
            }
            let v = slice_ty_of(b, types)?;
            Some(SliceTy::Map(types.intern(k), types.intern(v)))
        }
        (TypeConstructorId::Set, [a]) => {
            let e = slice_ty_of(a, types)?;
            if !matches!(e, SliceTy::Scalar(_) | SliceTy::Tuple(_) | SliceTy::Named(_)) {
                return None;
            }
            Some(SliceTy::Set(types.intern(e)))
        }
        _ => None,
    }
}

// Reason-string name helpers (moved from lib.rs for the file budget).
pub(crate) fn expr_kind_name(k: &IrExprKind) -> String {
    let dbg = format!("{k:?}");
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}

pub(crate) fn stmt_kind_name(k: &IrStmtKind) -> String {
    let dbg = format!("{k:?}");
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}

pub(crate) fn pattern_name(p: &IrPattern) -> String {
    let dbg = format!("{p:?}");
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}

pub(crate) fn ty_name(t: &Ty) -> String {
    let dbg = format!("{t:?}");
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}


/// The unique module-qualified type whose key ends `.{name}` — or None
/// (missing OR ambiguous; both refuse downstream, never guess).
fn named_suffix_unique(types: &TypeTable, name: &str) -> Option<SliceTy> {
    if name.contains('.') {
        return None;
    }
    let suffix = format!(".{name}");
    let mut hits = types.by_name.iter().filter(|(k, _)| k.ends_with(&suffix));
    let first = hits.next()?;
    if hits.next().is_some() {
        return None;
    }
    Some(SliceTy::Named(*first.1))
}
