//! `Ty::compatible` and its helpers, split out of `types/mod.rs` to keep
//! that file under the max-lines budget. See `types/unify.rs` for the
//! sibling TypeVar-binding unification pass — `compatible` is the
//! non-binding structural/nominal check used everywhere else (arg checking,
//! Union membership, etc).

use super::{names_match, Ty, TypeConstructorId};

impl Ty {
    /// Check if two types are compatible (Unknown and Never match everything)
    pub fn compatible(&self, other: &Ty) -> bool {
        if *self == Ty::Unknown || *other == Ty::Unknown
            || *self == Ty::Never || *other == Ty::Never {
            return true;
        }
        // TypeVars are compatible with anything (they represent polymorphic types)
        if matches!(self, Ty::TypeVar(_)) || matches!(other, Ty::TypeVar(_)) {
            return true;
        }
        if let Some(r) = Self::compatible_numeric(self, other) {
            return r;
        }
        match (self, other) {
            (Ty::String, Ty::String) => true,
            (Ty::Bool, Ty::Bool) => true,
            (Ty::Unit, Ty::Unit) => true,
            (Ty::Bytes, Ty::Bytes) => true,
            (Ty::RawPtr, Ty::RawPtr) => true,
            (Ty::Matrix, Ty::Matrix) => true,
            (Ty::Matrix, Ty::Applied(TypeConstructorId::Matrix, _))
            | (Ty::Applied(TypeConstructorId::Matrix, _), Ty::Matrix) => {
                Self::compatible_matrix_bridge(self, other)
            }
            (Ty::Applied(id1, args1), Ty::Applied(id2, args2)) if id1 == id2 && args1.len() == args2.len() => {
                args1.iter().zip(args2.iter()).all(|(a, b)| a.compatible(b))
            }
            _ => Self::compatible_structural(self, other),
        }
    }

    /// `compatible()` case for bare `Matrix` vs `Matrix[T]` — asymmetric
    /// discrimination:
    ///   - `Matrix.compatible(Matrix[T])` = true for all T (bare param
    ///     accepts any typed value — pre-P4 stdlib `matrix.shape(m: Matrix)`
    ///     stays usable with `matrix.zeros_f32` results).
    ///   - `Matrix[Ty::Float].compatible(Matrix)` = true only for the
    ///     `Float` dtype (`Matrix` is the alias for `Matrix[Float]`; bare
    ///     runtime repr is f64).
    ///   - `Matrix[Ty::Float32].compatible(Matrix)` etc = false: a
    ///     typed-arity fn like `mul_f32(a: Matrix[Float32])` REJECTS a bare
    ///     `Matrix` value, since the bare form carries no f32 guarantee.
    ///
    /// `types_mismatch` is single-directional so this asymmetry reaches
    /// call-site diagnostics unaltered.
    fn compatible_matrix_bridge(a: &Ty, b: &Ty) -> bool {
        match (a, b) {
            (Ty::Matrix, Ty::Applied(TypeConstructorId::Matrix, _)) => true,
            (Ty::Applied(TypeConstructorId::Matrix, args), Ty::Matrix) => {
                args.len() == 1 && matches!(args[0], Ty::Float | Ty::Float64)
            }
            _ => false,
        }
    }

    /// `compatible()` case for the structural/nominal types: Named/Variant
    /// cross-matching, Fn, Record/OpenRecord/Tuple, and Union. Split out
    /// because it's the largest remaining semantic group once the scalar
    /// and numeric arms are handled by the router and `compatible_numeric`.
    fn compatible_structural(a: &Ty, b: &Ty) -> bool {
        match (a, b) {
            (Ty::Named(a, _), Ty::Named(b, _)) => names_match(*a, *b),
            (Ty::Variant { name: a, .. }, Ty::Variant { name: b, .. }) => names_match(*a, *b),
            (Ty::Named(a, _), Ty::Variant { name: b, .. }) => names_match(*a, *b),
            (Ty::Variant { name: a, .. }, Ty::Named(b, _)) => names_match(*a, *b),
            (Ty::Fn { params: p1, ret: r1 }, Ty::Fn { params: p2, ret: r2 }) => {
                p1.len() == p2.len()
                    && p1.iter().zip(p2.iter()).all(|(a, b)| a.compatible(b))
                    && r1.compatible(r2)
            }
            (Ty::Record { fields: f1 }, Ty::Record { fields: f2 }) => {
                // Both closed: same field SET. Order-independent (by name):
                // a record's identity is its named fields — source order,
                // declaration order, and the sorted canonical order all occur
                // in Ty values, and the solver's unify_structural already
                // compares by name. A positional zip here rejected
                // `mix({ g: .., r: .. })` against `{ r, g }` (E005).
                f1.len() == f2.len()
                    && f1.iter().all(|(n1, t1)| f2.iter().any(|(n2, t2)| n1 == n2 && t1.compatible(t2)))
            }
            (Ty::OpenRecord { fields: required }, Ty::Record { fields: actual })
            | (Ty::OpenRecord { fields: required }, Ty::OpenRecord { fields: actual }) => {
                // Open parameter: all required fields must exist in the argument (by name, order-independent)
                required.iter().all(|(n1, t1)| actual.iter().any(|(n2, t2)| n1 == n2 && t1.compatible(t2)))
            }
            (Ty::Record { .. }, Ty::OpenRecord { .. }) => {
                // Closed parameter × open argument: not allowed
                false
            }
            (Ty::Tuple(a), Ty::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.compatible(y))
            }
            // Named ↔ Record: Named types are compatible with their structural expansion
            // (this handles the case where one side is resolve_named'd and the other isn't)
            (Ty::Named(_, _), Ty::Record { .. }) | (Ty::Record { .. }, Ty::Named(_, _)) => true,
            // Union: a concrete type is compatible with a union if it matches any member
            (Ty::Union(members), other) => members.iter().any(|m| m.compatible(other)),
            (other, Ty::Union(members)) => members.iter().any(|m| other.compatible(m)),
            _ => false,
        }
    }

    /// `compatible()` case for numeric-type pairs: canonical Int/Float
    /// against each other and against the sized ints/floats (exact-width
    /// match, Int64/Float64 bridging, and literal coercion). Returns `None`
    /// for any pair not covered here so `compatible` falls through to the
    /// rest of its match.
    fn compatible_numeric(a: &Ty, b: &Ty) -> Option<bool> {
        Self::compatible_numeric_exact(a, b).or_else(|| Self::compatible_numeric_coerce(a, b))
    }

    /// `compatible_numeric` sub-case: exact same-width match (Int/Int,
    /// Int8/Int8, ... Float64/Float64). Cross-width ops require explicit
    /// conversion (Stage 1c will enforce in the arithmetic dispatch).
    fn compatible_numeric_exact(a: &Ty, b: &Ty) -> Option<bool> {
        Some(match (a, b) {
            (Ty::Int, Ty::Int) => true,
            (Ty::Float, Ty::Float) => true,
            (Ty::Int8, Ty::Int8) => true,
            (Ty::Int16, Ty::Int16) => true,
            (Ty::Int32, Ty::Int32) => true,
            (Ty::Int64, Ty::Int64) => true,
            (Ty::UInt8, Ty::UInt8) => true,
            (Ty::UInt16, Ty::UInt16) => true,
            (Ty::UInt32, Ty::UInt32) => true,
            (Ty::UInt64, Ty::UInt64) => true,
            (Ty::Float32, Ty::Float32) => true,
            (Ty::Float64, Ty::Float64) => true,
            _ => return None,
        })
    }

    /// `compatible_numeric` sub-case: Int64/Float64 bridging to the
    /// canonical `Int`/`Float` slots, and literal coercion of `Int`/`Int64`
    /// (resp. `Float`/`Float64`) into any narrower sized width.
    fn compatible_numeric_coerce(a: &Ty, b: &Ty) -> Option<bool> {
        Some(match (a, b) {
            // `Int` (canonical, literal slot) ↔ `Int64` (explicit
            // width). Same 64-bit runtime repr so they freely interop.
            // The binop `is_sized_scalar` rule still catches
            // `Int32 + Int64` because `Int64` is *sized*; `Int` is not.
            (Ty::Int, Ty::Int64) | (Ty::Int64, Ty::Int) => true,
            (Ty::Float, Ty::Float64) | (Ty::Float64, Ty::Float) => true,
            // Int64 / Float64 literal-coerce to narrower sized widths,
            // same as `Int` / `Float`.
            (Ty::Int64, Ty::Int8 | Ty::Int16 | Ty::Int32
                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64)
            | (Ty::Int8 | Ty::Int16 | Ty::Int32
                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64, Ty::Int64) => true,
            (Ty::Float64, Ty::Float32) | (Ty::Float32, Ty::Float64) => true,
            // Literal coercion (Sized Numeric Types Stage 1b): an
            // integer literal inferred as `Ty::Int` is accepted in a
            // context that expects any sized integer type. Same for
            // `Ty::Float` ↔ `Ty::Float32`. The coercion is symmetric
            // in `compatible` because this pass runs before range
            // checking; the subsequent arithmetic-dispatch sub-phase
            // will enforce same-type binary ops, and an explicit
            // range-check pass (Stage 1b polish) catches `UInt8 = 300`.
            // Keeping it here (rather than threading an "expected
            // type" through infer) is a deliberate minimum-viable
            // choice: it gets `let x: Int32 = 42` working today with
            // a tight, auditable one-line rule per pairing.
            (Ty::Int, Ty::Int8 | Ty::Int16 | Ty::Int32
                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64)
            | (Ty::Int8 | Ty::Int16 | Ty::Int32
                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64, Ty::Int) => true,
            (Ty::Float, Ty::Float32) | (Ty::Float32, Ty::Float) => true,
            _ => return None,
        })
    }
}
