//! `secret.Secret[A]` (#4107): the type facts the checker and the lowering
//! share, read off one place.
//!
//! - Equality (R-secret-03): `==` on `Secret[String]` / `Secret[Bytes]`
//!   compares the revealed values in constant time
//!   (`string.is_equal_constant_time` / `bytes.is_equal_constant_time`); a
//!   `Secret` of any other type has no `==` (E103). A record, tuple, list,
//!   option or result holding a Secret compares through the same rule at the
//!   Secret's position; a variant or map holding one has no `==` (E103 —
//!   compare with a `match` / per key).
//! - Codec (R-secret-04): a Secret field decodes from its value's wire form
//!   and is never encoded (E023).

use crate::types::{Ty, TypeConstructorId as TC, TypeEnv};
use almide_lang::types::VariantPayload;

/// The stdlib-owned `(module, type)` this file is about.
pub const SECRET_TYPE: (&str, &str) = ("secret", "Secret");

/// The payload type when `ty` is `Secret[A]`.
pub fn secret_payload(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Named(n, args) if n.as_str() == SECRET_TYPE.1 && args.len() == 1 => Some(&args[0]),
        _ => None,
    }
}

/// The constant-time comparison `(module, fn)` for a Secret's payload type,
/// or None when that Secret has no `==`.
pub fn constant_time_eq_fn(payload: &Ty) -> Option<(&'static str, &'static str)> {
    match payload {
        Ty::String => Some(("string", "is_equal_constant_time")),
        Ty::Bytes => Some(("bytes", "is_equal_constant_time")),
        _ => None,
    }
}

/// One level of a nominal type, its arguments substituted.
pub fn unfold_named(env: &TypeEnv, ty: &Ty) -> Option<Ty> {
    let Ty::Named(name, args) = ty else { return None };
    let shape = env.types.get(name)?.clone();
    if args.is_empty() {
        return Some(shape);
    }
    let params_key = crate::canonicalize::resolve::type_params_key(name.as_str());
    let Some(Ty::Tuple(params)) = env.types.get(&params_key) else { return Some(shape) };
    let bindings: std::collections::HashMap<_, _> = params.iter().zip(args.iter())
        .filter_map(|(p, a)| match p { Ty::TypeVar(g) => Some((*g, a.clone())), _ => None })
        .collect();
    Some(almide_lang::types::substitute(&shape, &bindings))
}

/// Does a value of `ty` hold a Secret anywhere?
pub fn holds_secret(env: &TypeEnv, ty: &Ty) -> bool {
    fn go(env: &TypeEnv, ty: &Ty, seen: &mut Vec<Ty>) -> bool {
        if secret_payload(ty).is_some() {
            return true;
        }
        if seen.contains(ty) {
            return false;
        }
        match ty {
            Ty::Named(..) => {
                seen.push(ty.clone());
                let found = match unfold_named(env, ty) {
                    Some(Ty::Record { fields }) | Some(Ty::OpenRecord { fields }) => fields.iter().any(|(_, t)| go(env, t, seen)),
                    Some(Ty::Variant { cases, .. }) => cases.iter().any(|c| match &c.payload {
                        VariantPayload::Unit => false,
                        VariantPayload::Tuple(ts) => ts.iter().any(|t| go(env, t, seen)),
                        VariantPayload::Record(fs) => fs.iter().any(|(_, t)| go(env, t, seen)),
                    }),
                    Some(other) if !matches!(other, Ty::Named(..)) => go(env, &other, seen),
                    _ => false,
                };
                seen.pop();
                found
            }
            Ty::Applied(_, args) | Ty::Tuple(args) => args.iter().any(|a| go(env, a, seen)),
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| go(env, t, seen)),
            Ty::Variant { cases, .. } => cases.iter().any(|c| match &c.payload {
                VariantPayload::Unit => false,
                VariantPayload::Tuple(ts) => ts.iter().any(|t| go(env, t, seen)),
                VariantPayload::Record(fs) => fs.iter().any(|(_, t)| go(env, t, seen)),
            }),
            _ => false,
        }
    }
    go(env, ty, &mut Vec::new())
}

/// Why `==` on `ty` is refused (E103), or None when it may compare: every
/// Secret inside has a constant-time payload, and none sits in a variant or
/// a map.
pub fn secret_eq_refusal(env: &TypeEnv, ty: &Ty) -> Option<String> {
    fn go(env: &TypeEnv, ty: &Ty, seen: &mut Vec<Ty>) -> Option<String> {
        if let Some(p) = secret_payload(ty) {
            if matches!(p, Ty::TypeVar(_) | Ty::Unknown) || constant_time_eq_fn(p).is_some() {
                return None;
            }
            return Some("a Secret compares only when it holds a String or Bytes (in constant time)".to_string());
        }
        if seen.contains(ty) {
            return None;
        }
        match ty {
            Ty::Named(..) => {
                seen.push(ty.clone());
                let r = match unfold_named(env, ty) {
                    Some(Ty::Record { fields }) | Some(Ty::OpenRecord { fields }) => fields.iter().find_map(|(_, t)| go(env, t, seen)),
                    Some(v @ Ty::Variant { .. }) if holds_secret(env, &v) =>
                        Some("a variant holding a Secret has no `==`".to_string()),
                    _ => None,
                };
                seen.pop();
                r
            }
            Ty::Applied(TC::Map | TC::Set, args) if args.iter().any(|a| holds_secret(env, a)) =>
                Some("a map or set holding a Secret has no `==`".to_string()),
            Ty::Applied(_, args) | Ty::Tuple(args) => args.iter().find_map(|a| go(env, a, seen)),
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().find_map(|(_, t)| go(env, t, seen)),
            Ty::Variant { .. } if holds_secret(env, ty) => Some("a variant holding a Secret has no `==`".to_string()),
            _ => None,
        }
    }
    go(env, ty, &mut Vec::new())
}

/// The E103 diagnostic for `==` / `assert_eq` (`what`) on `ty`, or None when
/// [`secret_eq_refusal`] lets it compare. The caller places the span.
pub(crate) fn secret_eq_diagnostic(env: &TypeEnv, ty: &Ty, what: &str) -> Option<almide_base::diagnostic::Diagnostic> {
    let why = secret_eq_refusal(env, ty)?;
    Some(crate::check::err(
        format!("{} cannot compare '{}': {}", what, ty.display(), why),
        "Compare what the secrets hold, explicitly: `secret.reveal(a) == secret.reveal(b)`. \
         `==` on Secret[String] / Secret[Bytes] is constant time; a variant or map holding \
         a Secret compares by `match` / per key.",
        what,
    ).with_code("E103"))
}
