//! The generator's type universe.
//!
//! `GenType` is a *closed* set of types the type-directed generator can
//! produce a value of. It is intentionally narrower than Almide's full
//! type system: we want every generated program to be well-typed *by
//! construction*, so the generator only commits to types whose value
//! grammar it fully understands. Exotic shapes (records, variants,
//! protocols) enter the corpus through the *mutation* path instead,
//! where the parser has already proven they type-check.
//!
//! Each `GenType` knows how to print itself as an Almide type
//! annotation (`render`), so the generator can emit `let v: T = ...`
//! and lambda parameter types without a separate formatter.

use std::fmt::Write;

/// A type the generator can synthesize an expression of.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenType {
    Int,
    Float,
    String,
    Bool,
    Unit,
    /// `List[elem]`.
    List(Box<GenType>),
    /// `Option[inner]`.
    Option(Box<GenType>),
    /// `Result[ok, String]` — the error arm is fixed to `String`, which
    /// is the overwhelmingly common stdlib shape (`int.parse`,
    /// `float.parse`, `value.as_*`), keeping the universe finite.
    Result(Box<GenType>),
    /// `(a, b)` — pairs only; wider tuples add little divergence value
    /// for a lot of grammar surface.
    Tuple2(Box<GenType>, Box<GenType>),
    /// `Map[String, value]` — keys fixed to `String` (the common,
    /// hashable, byte-comparable case).
    Map(Box<GenType>),
}

impl GenType {
    /// Render as an Almide type annotation.
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.render_into(&mut out);
        out
    }

    fn render_into(&self, out: &mut String) {
        match self {
            GenType::Int => out.push_str("Int"),
            GenType::Float => out.push_str("Float"),
            GenType::String => out.push_str("String"),
            GenType::Bool => out.push_str("Bool"),
            GenType::Unit => out.push_str("Unit"),
            GenType::List(e) => {
                out.push_str("List[");
                e.render_into(out);
                out.push(']');
            }
            GenType::Option(e) => {
                out.push_str("Option[");
                e.render_into(out);
                out.push(']');
            }
            GenType::Result(e) => {
                out.push_str("Result[");
                e.render_into(out);
                out.push_str(", String]");
            }
            GenType::Tuple2(a, b) => {
                out.push('(');
                a.render_into(out);
                out.push_str(", ");
                b.render_into(out);
                out.push(')');
            }
            GenType::Map(v) => {
                out.push_str("Map[String, ");
                v.render_into(out);
                out.push(']');
            }
        }
    }

    /// `true` if a value of this type can be compared with `==` and
    /// printed deterministically across both targets. Used to gate
    /// which goal types may appear as the program's *final observable*
    /// (we never make a bare closure the observable, for instance).
    pub fn is_observable(&self) -> bool {
        match self {
            GenType::Int | GenType::String | GenType::Bool | GenType::Unit => true,
            // Float is observable but its *formatting* is a divergence
            // we WANT to surface — so it is allowed.
            GenType::Float => true,
            GenType::List(e) | GenType::Option(e) | GenType::Result(e) => e.is_observable(),
            GenType::Tuple2(a, b) => a.is_observable() && b.is_observable(),
            GenType::Map(v) => v.is_observable(),
        }
    }

    /// Structural depth — bounds recursion when the generator picks a
    /// random goal type so it does not build `List[List[List[...]]]`.
    pub fn depth(&self) -> u32 {
        match self {
            GenType::Int
            | GenType::Float
            | GenType::String
            | GenType::Bool
            | GenType::Unit => 1,
            GenType::List(e) | GenType::Option(e) | GenType::Result(e) | GenType::Map(e) => {
                1 + e.depth()
            }
            GenType::Tuple2(a, b) => 1 + a.depth().max(b.depth()),
        }
    }
}

/// Emit a deterministic `println` for a value of `ty` bound to `name`.
///
/// The observable output is what the differential oracle byte-compares
/// across targets, so the printing must be *total* and identical-shape
/// on both. We lean on `${expr}` interpolation, which the compiler
/// lowers to each type's `Display`/`Debug` — exactly the surface we
/// want to differential-test. For non-directly-interpolable shapes the
/// generator wraps them so the printed form is still deterministic.
pub fn render_print(name: &str, _ty: &GenType) -> String {
    // All printable shapes interpolate via `${...}`, so the type is not
    // needed to render — the parameter documents the contract that only
    // observable types reach here (enforced by the caller). The label
    // keeps multi-statement programs' outputs aligned to their producing
    // binding for triage.
    let mut out = String::new();
    let _ = write!(out, "println(\"{name} = ${{{name}}}\")");
    out
}
