//! What a module-op arm hands back — DECLARED by the arm, per site, as
//! a type (#2004). No default, no list: an arm that does not say whether
//! its result is a fresh block or a view does not compile.
//!
//! The reference shape (Swift OSSA's `@owned` / `@guaranteed` results,
//! Lean's and Koka's owned-result convention with declared borrows): the
//! ownership of every value that crosses a call boundary is part of the
//! callee's signature and is checked, never inferred from a hand list.
//! Here the callee is a native arm of the structural emitter; its
//! signature is the `Lowered` it returns.
//!
//! * `Owned` — the arm allocated the block (or received it owned from
//!   the registry-table callee): the caller holds its ONE credit and
//!   releases it at the frame's exit plan, or moves it into a callee.
//! * `View` — the block already belongs to someone else (an element of
//!   a container, a map value, a parameter passed through): the caller
//!   that keeps it takes the borrow +1 the bind route pays.
//! * `Scalar` — no block at all (i64 / f64 / i32 flags, Unit-shaped
//!   carriers); the declaration asserts it.
//!
//! The wrapper (`lower_module_call`) is the one reader: it marks the
//! call node owned exactly when the arm declared `Owned` and the type is
//! droppable, and hands the plain type on to expression lowering.
//! The verification is the generated matrix
//! (tests/native_result_ownership.rs — every stdlib fn with a droppable
//! result from the signature index): an arm declaring `View` for a
//! block it allocated shows as a growing row; an arm declaring `Owned`
//! for a view shows as a cross-target divergence.

use crate::SliceTy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Own {
    Owned,
    View,
    Scalar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lowered {
    pub(crate) ty: SliceTy,
    pub(crate) own: Own,
}

impl Lowered {
    /// The arm allocated this block; the caller owns its one credit.
    pub(crate) const fn owned(ty: SliceTy) -> Self {
        Lowered { ty, own: Own::Owned }
    }
    /// The block belongs to an existing holder; the caller borrows it.
    pub(crate) const fn view(ty: SliceTy) -> Self {
        Lowered { ty, own: Own::View }
    }
    /// Not a block. Asserted in debug builds.
    pub(crate) fn scalar(ty: SliceTy) -> Self {
        debug_assert!(
            !matches!(ty, SliceTy::List(_) | SliceTy::Scalar(crate::Scalar::Str | crate::Scalar::Bytes)),
            "a droppable block declared scalar: {ty:?}"
        );
        Lowered { ty, own: Own::Scalar }
    }
}

/// The arm chain's return type: `None` is a Unit-position op.
pub(crate) type ArmResult = Result<Option<Lowered>, crate::EmitError>;
