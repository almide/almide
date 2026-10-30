//! The list `+` operator on the native render (#4006).
//!
//! The lowering spells `a + b` over lists as one call per element family
//! (`crate::lower::LIST_CONCAT` for scalar elements, `LIST_CONCAT_RC` for
//! owned heap elements). Their self-hosted bodies are raw block copies (prim
//! loads and stores), which the native render has no memory for, so the
//! operator renders as what it means on a `Vec`: a fresh list holding `a`'s
//! elements then `b`'s (a heap element clones, the reference the rc variant
//! takes). The rebind `xs = xs + ys` grows `xs` in place
//! (`render_native_append::list_self_append`), so an append loop stays
//! amortized linear (C-131).

use crate::render_native::NTy;

/// Is `name` the lowering's spelling of the list `+` operator?
pub(crate) fn is_list_concat(name: &str) -> bool {
    name == crate::lower::LIST_CONCAT || name == crate::lower::LIST_CONCAT_RC
}

/// A list operand's slice form and the owned list type it shares, or `None`
/// when `t` is not a list the render carries.
pub(crate) fn as_slice(code: &str, t: NTy) -> Option<(String, String)> {
    let owned = match t {
        NTy::Vec | NTy::VecStr => true,
        NTy::VecRef | NTy::VecStrRef => false,
        NTy::Rec(i) if crate::render_native_rec::list_elem(i).is_some() => true,
        NTy::RecRef(i) if crate::render_native_rec::list_elem(i).is_some() => false,
        _ => return None,
    };
    let slice = if owned { format!("&{code}[..]") } else { code.to_string() };
    Some((slice, crate::render_native_rec::owned_ty(t)))
}

/// The owned NTy of a list operand's kind.
pub(crate) fn owned(t: NTy) -> NTy {
    match t {
        NTy::VecRef => NTy::Vec,
        NTy::VecStrRef => NTy::VecStr,
        NTy::RecRef(i) => NTy::Rec(i),
        other => other,
    }
}
