//! `almide-verify` — the independent re-checker for Almide's flight-grade
//! certificates (#2152).
//!
//! The compiler is the UNTRUSTED producer: `almide-mir`'s certificate emitter
//! projects each function's MIR to witness bytes, one witness per property.
//! This crate is the other side of that seam. It reads witness bytes and
//! nothing else — no MIR, no IR, no compiler crate — and decides each one
//! with a transcription of the Coq checker that owns the property:
//!
//! | property          | mirrors (proofs/)                          | accepts iff |
//! |-------------------|--------------------------------------------|-------------|
//! | `ownership`       | `OwnershipChecker.check_xc`                | every object's refcount stream is fault-free and ends at 0 |
//! | `names`           | `NameTotality.check_names_cert`            | used ids ⊆ defined ids |
//! | `caps`            | `CapabilityBound.check_caps_cert`          | used capabilities ⊆ declared |
//! | `caps-transitive` | `CapabilityReach.check_prog_cert`          | every function's transitive reach ⊆ its declaration |
//! | `call-modes`      | `CallModes.check_modes_cert`               | every call site's modes equal its callee's signature |
//!
//! WHAT IS PROVEN AND WHAT IS NOT. The Coq functions carry the soundness
//! theorems; this crate does not — it is a second implementation of the same
//! definitions, written so that a binary distribution can re-check a build
//! without a Rocq toolchain. Its agreement with the extracted checker and the
//! Rocq kernel is gated, not assumed: `proofs/gate.sh` runs all three on
//! every row (accept and reject) plus a seeded random differential, and
//! `proofs/corpus-wall.sh` runs it over the whole corpus witness set.

pub mod bundle;
mod modes;
mod nat;
mod ownership;
mod reach;

/// The verifier's own version — independent of the compiler's.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The witness formats this version reads.
pub const FORMATS: &str =
    "ownership v5, names v1, caps v1, caps-transitive v1, call-modes v1, bundle v1";

/// A flight-grade property a witness certifies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Property {
    Ownership,
    Names,
    Caps,
    CapsTransitive,
    CallModes,
}

impl Property {
    pub const ALL: [Property; 5] = [
        Property::Ownership,
        Property::Names,
        Property::Caps,
        Property::CapsTransitive,
        Property::CallModes,
    ];

    /// The name used on the command line and in bundles — the same mode names
    /// the extracted checker (`proofs/checker`) takes.
    pub fn name(self) -> &'static str {
        match self {
            Property::Ownership => "ownership",
            Property::Names => "names",
            Property::Caps => "caps",
            Property::CapsTransitive => "caps-transitive",
            Property::CallModes => "call-modes",
        }
    }

    pub fn from_name(name: &str) -> Option<Property> {
        Property::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// Decide one witness. `true` = ACCEPT.
pub fn check(property: Property, witness: &[u8]) -> bool {
    match property {
        Property::Ownership => ownership::check_xc(witness),
        Property::Names | Property::Caps => nat::subset_cert(witness),
        Property::CapsTransitive => reach::check_prog_cert(witness),
        Property::CallModes => modes::check_modes_cert(witness),
    }
}
