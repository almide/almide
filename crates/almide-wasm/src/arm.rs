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

/// What an arm does with an ARGUMENT block — declared at the site that
/// lowers it (the Koka / Lean borrow summary, per parameter, in code).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArgMode {
    /// The arm only reads the block: a temporary handed to it (a call
    /// result, a born-here literal) is released after the op by the
    /// wrapper — nobody else will.
    Borrow,
    /// The arm keeps the block (stores it into a container, returns it):
    /// a temporary's credit moves into the arm's result; a block that
    /// already has a holder takes the share +1 here.
    Retain,
    /// The `prim` floor: the arm reads a raw address out of the block
    /// (`prim.handle(buf)`) and the frame's raw-address rule owns every
    /// release on the epilogue — no share, no release here. Only prim.rs
    /// may declare it.
    Raw,
}

impl crate::emitter::Emitter<'_> {
    /// Lower one ARGUMENT of a module op under its declared mode. Under
    /// `Borrow`, an owned temporary (`rc_owned_result`) is parked in a
    /// borrow-pool local so the scope can release it once the op is done;
    /// its value stays on the stack for the arm exactly as `lower` left
    /// it.
    pub(crate) fn lower_arg(
        &mut self,
        a: &almide_ir::IrExpr,
        want: Option<SliceTy>,
        mode: ArgMode,
    ) -> Result<SliceTy, crate::EmitError> {
        let got = self.lower(a, want)?;
        // Retain IS the share: a block the arm stores that already has a
        // holder (a Var, a funnel over borrows) takes +1 here — the
        // declaration carries the guard, no arm repeats it. (value.str
        // stored a Var's string bare and only an over-borrow elsewhere kept
        // it alive — heap_result_tuple_return, 2026-09-07.) An owned
        // temporary moves in: `rc_share_guard` incs nothing fresh.
        if mode == ArgMode::Retain {
            self.rc_share_guard(a, got);
        }
        // A string literal is a pool static (below the heap floor: $dec is
        // a no-op on it) — nothing to release, and no reason to ship the
        // rc core for a program that never allocates (#1962).
        let is_static = matches!(a.kind, almide_ir::IrExprKind::LitStr { .. });
        if mode == ArgMode::Borrow && !is_static && self.rc_droppable(got) && self.rc_owned_result(a) {
            if self.borrowed_temps.len() as u32 >= crate::emitter::BORROW_POOL {
                return Err(crate::EmitError::Unsupported("borrow-depth".into()));
            }
            let h = self.borrow_base + self.borrowed_temps.len() as u32;
            self.f.instructions().local_tee(h);
            self.borrowed_temps.push(h);
        }
        Ok(got)
    }

    /// The scope that pairs with `lower_arg`: run one arm entry (the
    /// module-call wrapper, or a direct entry such as the slice / map-access
    /// syntax and the print builtins) and release every temporary its arms
    /// borrowed once its result is on the stack. Every route that reaches
    /// an arm goes through here — a borrow with no scope is a hold-balance
    /// BUG wall at the frame's end (func.rs), never a silent leak.
    pub(crate) fn arm_scope<T>(
        &mut self,
        body: impl FnOnce(&mut Self) -> Result<T, crate::EmitError>,
    ) -> Result<T, crate::EmitError> {
        let depth = self.borrowed_temps.len();
        let out = body(self)?;
        self.release_borrowed_temps(depth);
        Ok(out)
    }

    /// The scope's half: release every temporary the arms borrowed since
    /// `depth`, in reverse order (the holds are the top of the pool now —
    /// every hold an arm took inside has been released).
    fn release_borrowed_temps(&mut self, depth: usize) {
        while self.borrowed_temps.len() > depth {
            let h = self.borrowed_temps.pop().unwrap();
            self.f.instructions().local_get(h).call(crate::F_DEC_FLAT);
        }
    }
}
