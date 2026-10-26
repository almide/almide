//! Affine-shaped functions (#4236).
//!
//! A lowered function is AFFINE-SHAPED when its body creates no second
//! reference to any heap object: it contains no [`Op::Dup`]. [`Op::Dup`] is the
//! single +1 that aliases a still-live value (the only other +1, [`Op::Alloc`],
//! mints a fresh object with one owner), so a Dup-free body gives every heap
//! object it touches exactly one owner at a time — the property an affine type
//! system proves statically, here observed on the ownership Perceus-style
//! lowering already decided. Nothing is asked of the writer: the shape is read
//! off the MIR, never declared in source.
//!
//! The definition lives here and only here. A consumer (the census, a later
//! counter-free rendering, a GPU leg's eligibility rule) calls [`affine_shape`]
//! instead of re-deciding it.

use crate::{MirFunction, Op, ValueId};

/// Where a function's `Dup`s come from — the reason it is not affine-shaped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AffineShape {
    /// `Dup`s of a borrowed heap PARAM. The v1 calling convention lends params,
    /// so a body that returns, stores or consumes one must acquire its own
    /// reference — a boundary cost, not sharing the body chose.
    pub param_dups: Vec<ValueId>,
    /// `Dup`s of a value the body itself owns: a heap value used again while
    /// still live (the sharing an affine type system would reject).
    pub local_dups: Vec<ValueId>,
}

impl AffineShape {
    /// No `Dup` at all: every heap object has one owner at every point.
    pub fn is_affine(&self) -> bool {
        self.param_dups.is_empty() && self.local_dups.is_empty()
    }
}

/// Classify one lowered function. Each `Dup` is attributed to its SOURCE value:
/// a param's value id is a param dup, anything else a local dup.
pub fn affine_shape(func: &MirFunction) -> AffineShape {
    let mut shape = AffineShape::default();
    for op in &func.ops {
        if let Op::Dup { src, .. } = op {
            if func.params.iter().any(|p| p.value == *src) {
                shape.param_dups.push(*src);
            } else {
                shape.local_dups.push(*src);
            }
        }
    }
    shape
}
