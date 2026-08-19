//! THE heap classification: which `Ty` is a refcounted heap block and which is a
//! Copy scalar, in the v1 ownership model (one i64/f64 slot, no `Alloc`/`Dup`/
//! `Drop`).
//!
//! This predicate used to exist twice — `almide-mir::lower::is_heap_ty` (the
//! ownership/layout source of truth) and a verbatim cross-crate copy in
//! `almide-optimize::branch_lift`, kept in step by nothing but a comment (#926).
//! It lives HERE, beside `Ty` itself, because both of those crates and anything
//! else that asks the question can depend on this one; the mir name re-exports it
//! so the SoT reading is unchanged.
//!
//! Distinct on purpose: `almide-codegen::perceus_verified::is_heap_type`. That is
//! the NATIVE-RUST model's classification (which values the emitted Rust manages
//! through the RC runtime), and it genuinely answers differently — a tuple there
//! is a Rust `(A, B)` whose fields carry their own counts, not one RC block. The
//! two predicates are different QUESTIONS about different runtimes, each defined
//! once; what #926 ended was five definitions of these two questions.

use super::Ty;

/// Heap-managed types (need refcount: `Alloc`/`Dup`/`Drop`) vs `Copy` scalars.
///
/// The match is exhaustive WITHOUT a wildcard, deliberately: a new `Ty` variant
/// must be classified here before the workspace compiles, so it can never be
/// classified by omission — in either direction. (The previous form was a
/// scalar blacklist, which silently called every new variant "heap"; safe, but
/// silent, and the silence is how the copies drifted.)
pub fn is_heap_ty(ty: &Ty) -> bool {
    match ty {
        // Copy scalars: one raw slot, no ownership events.
        Ty::Int
        | Ty::Int8
        | Ty::Int16
        | Ty::Int32
        | Ty::Int64
        | Ty::UInt8
        | Ty::UInt16
        | Ty::UInt32
        | Ty::UInt64
        | Ty::Float
        | Ty::Float32
        | Ty::Float64
        | Ty::Bool
        | Ty::Unit
        | Ty::Never
        | Ty::RawPtr
        | Ty::ConstParam { .. }
        | Ty::ConstValue { .. } => false,
        // Refcounted heap blocks — including the checker-internal shapes
        // (`TypeVar`, `Union`, `Unknown`): when one of those leaks past the
        // solver, treating it as heap is the conservative reading (a missed
        // scalar costs a refcount; a missed heap block is a leak or a
        // use-after-free).
        Ty::String
        | Ty::Bytes
        | Ty::Matrix
        | Ty::Applied(..)
        | Ty::Record { .. }
        | Ty::OpenRecord { .. }
        | Ty::Variant { .. }
        | Ty::Fn { .. }
        | Ty::Tuple(..)
        | Ty::Named(..)
        | Ty::Union(..)
        | Ty::TypeVar(..)
        | Ty::Unknown => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::sym;

    /// The classification, pinned VALUE by value (the exhaustive match already
    /// forces a decision at compile time; this pins which decision was made, so
    /// an edit to an arm is a red test and not a silent re-model).
    #[test]
    fn the_scalar_and_heap_classification_is_pinned() {
        use super::super::constructor::TypeConstructorId as TC;
        let scalars = [
            Ty::Int,
            Ty::Int8,
            Ty::Int16,
            Ty::Int32,
            Ty::Int64,
            Ty::UInt8,
            Ty::UInt16,
            Ty::UInt32,
            Ty::UInt64,
            Ty::Float,
            Ty::Float32,
            Ty::Float64,
            Ty::Bool,
            Ty::Unit,
            Ty::Never,
            Ty::RawPtr,
            Ty::ConstParam { name: sym("N"), ty: Box::new(Ty::Int) },
            Ty::ConstValue { ty: Box::new(Ty::Int), value: 3 },
        ];
        for t in &scalars {
            assert!(!is_heap_ty(t), "{t:?} is a Copy scalar");
        }
        let heaps = [
            Ty::String,
            Ty::Bytes,
            Ty::Matrix,
            Ty::Applied(TC::List, vec![Ty::Int]),
            Ty::Record { fields: vec![(sym("x"), Ty::Int)] },
            Ty::OpenRecord { fields: vec![(sym("x"), Ty::Int)] },
            Ty::Variant { name: sym("V"), cases: vec![] },
            Ty::Fn { is_effect: false, params: vec![], ret: Box::new(Ty::Unit) },
            Ty::Tuple(vec![Ty::Int, Ty::Bool]),
            Ty::Named(sym("P"), vec![]),
            Ty::Union(vec![Ty::Int, Ty::String]),
            Ty::TypeVar(sym("T")),
            Ty::Unknown,
        ];
        for t in &heaps {
            assert!(is_heap_ty(t), "{t:?} is a refcounted heap block");
        }
    }
}
