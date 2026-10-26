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
    /// `Result[ok, <nominal>]` — an error arm of a stdlib nominal type
    /// (`http.decode_json`'s `Result[T, HttpResponse]`). Built from the
    /// nominal's own constructors, observed with the error side taken through
    /// the nominal's own readers ([`super::nominal`]).
    ResultNom(Box<GenType>, &'static str),
    /// `(a, b)` — the pair the value grammar builds and consumes.
    Tuple2(Box<GenType>, Box<GenType>),
    /// `(a, b, c)` — only ever a RETURN type (the fused Qwen3 layers'
    /// `(Matrix, Matrix, Matrix)`, #1827): no goal is a triple, so the
    /// grammar never has to build one, but a function returning one must be
    /// catalogued for its inspection to run.
    Tuple3(Box<GenType>, Box<GenType>, Box<GenType>),
    /// `Map[String, value]` — keys fixed to `String` (the common,
    /// hashable, byte-comparable case).
    Map(Box<GenType>),
    /// A fixed-width number (`Int8` … `UInt64`, `Float32`, `Float64`), built
    /// from a small Int through its `<width>.from` conversion.
    Sized(&'static str),
    /// `Bytes`, built with `bytes.from_list` (or `bytes.zeros(n)` when sized).
    Bytes,
    /// `Matrix` (Float elements), built with `matrix.from_lists` (or
    /// `matrix.zeros(n, 2)` when sized).
    Matrix,
    /// `Value` (the dynamic JSON-shaped value), built with `value.int` /
    /// `str` / `bool` / `null` / `array` / `object` (an object of n keys when
    /// sized).
    Value,
    /// `Set[elem]`, built with `set.from_list`; the element is a hashable
    /// scalar ([`GenType::is_set_elem`]).
    Set(Box<GenType>),
    /// A stdlib nominal type (`HttpResponse`, `SafeHtml`, …), built and
    /// observed through its module's pure functions ([`super::nominal`]).
    Nominal(&'static str),
    /// `secret.Secret[inner]` (#4107): built with `secret.new`, observed
    /// through `secret.reveal` — its own rendering is always `Secret(***)`.
    Secret(Box<GenType>),
}

/// The fixed-width number types the value grammar builds.
pub const SIZED: &[&str] = &["Int8", "Int16", "Int32", "Int64", "UInt8", "UInt16", "UInt32", "UInt64", "Float32", "Float64"];

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
            GenType::ResultNom(e, k) => {
                out.push_str("Result[");
                e.render_into(out);
                out.push_str(", ");
                out.push_str(k);
                out.push(']');
            }
            GenType::Tuple2(a, b) => {
                out.push('(');
                a.render_into(out);
                out.push_str(", ");
                b.render_into(out);
                out.push(')');
            }
            GenType::Tuple3(a, b, c) => {
                out.push('(');
                a.render_into(out);
                out.push_str(", ");
                b.render_into(out);
                out.push_str(", ");
                c.render_into(out);
                out.push(')');
            }
            GenType::Map(v) => {
                out.push_str("Map[String, ");
                v.render_into(out);
                out.push(']');
            }
            GenType::Sized(name) => out.push_str(name),
            GenType::Set(e) => {
                out.push_str("Set[");
                e.render_into(out);
                out.push(']');
            }
            GenType::Bytes => out.push_str("Bytes"),
            GenType::Matrix => out.push_str("Matrix"),
            GenType::Value => out.push_str("Value"),
            GenType::Nominal(k) => out.push_str(k),
            GenType::Secret(e) => {
                out.push_str("Secret[");
                e.render_into(out);
                out.push(']');
            }
        }
    }

    /// A type a `Set` can hold: hashable and equal by value (no Float — a
    /// `Set[Float]` is refused by the checker).
    pub fn is_set_elem(&self) -> bool {
        match self {
            GenType::Int | GenType::String | GenType::Bool => true,
            GenType::Sized(name) => !name.starts_with("Float"),
            _ => false,
        }
    }

    /// The fixed-width type named `name`, if it is one the grammar builds.
    pub fn sized(name: &str) -> Option<GenType> {
        SIZED.iter().find(|s| **s == name).map(|s| GenType::Sized(s))
    }

    /// A value of the fixed-width type `name` from the Int expression `n`.
    pub fn sized_literal(name: &str, n: &str) -> String {
        format!("{}.from({n})", name.to_lowercase())
    }

    /// `true` if a value of this type can be compared with `==` and
    /// printed deterministically across both targets. Used to gate
    /// which goal types may appear as the program's *final observable*
    /// (we never make a bare closure the observable, for instance).
    /// Statements and an expression that turn a value of this type into one
    /// [`is_observable`](Self::is_observable) accepts: a Bytes becomes its
    /// byte list and a Matrix its row lists, through any list, option, result
    /// or tuple around them. `None` for a shape with no such image here (a Map
    /// holding Bytes). `depth` keeps nested lambda and binding names apart.
    pub fn observe(&self, v: &str, depth: usize) -> Option<(String, String)> {
        if self.is_observable() {
            return Some((String::new(), v.to_string()));
        }
        let x = format!("o{depth}");
        let inner = |e: &GenType| -> Option<String> {
            let (pre, expr) = e.observe(&x, depth + 1)?;
            pre.is_empty().then(|| format!("(({x}) => {expr})"))
        };
        // An observation that calls an effect closure (a handler) cannot sit
        // in a plain callback: an Option or Result is taken apart with a
        // `match`, a list mapped in its fallible form.
        if let GenType::List(e) | GenType::Option(e) | GenType::Result(e) = self {
            if e.observe_calls_effect() {
                let (pre, expr) = e.observe(&x, depth + 1)?;
                if !pre.is_empty() {
                    return None;
                }
                return Some((
                    String::new(),
                    match self {
                        GenType::List(_) => format!("list.map({v}, ({x}) => {expr})!"),
                        GenType::Option(_) => format!("match {v} {{ some({x}) => some({expr}), none => none }}"),
                        _ => format!("match {v} {{ ok({x}) => ok({expr}), err(e{depth}) => err(e{depth}) }}"),
                    },
                ));
            }
        }
        // The error side becomes the String of what the nominal's readers see
        // (bound first: a call interpolated in `${…}` can be E041), so the
        // image is an ordinary `Result[_, String]`.
        if let GenType::ResultNom(e, k) = self {
            let nom = super::nominal::get(k)?;
            if nom.observe_calls_effect {
                return None;
            }
            let (pre, expr) = e.observe(&x, depth + 1)?;
            if !pre.is_empty() {
                return None;
            }
            let ev = format!("e{depth}");
            let seen = (nom.observed)().render();
            let eo = super::nominal::fill(nom.observe, &ev, 0);
            return Some((
                String::new(),
                format!("match {v} {{ ok({x}) => ok({expr}), err({ev}) => {{\n    let {ev}o: {seen} = {eo}\n    err(\"${{{ev}o}}\")\n  }} }}"),
            ));
        }
        if let GenType::Secret(e) = self {
            return e.observe(&format!("secret.reveal({v})"), depth + 1);
        }
        match self {
            GenType::Bytes => Some((String::new(), format!("bytes.to_list({v})"))),
            GenType::Matrix => Some((String::new(), format!("matrix.to_lists({v})"))),
            GenType::Value => Some((String::new(), format!("json.stringify({v})"))),
            GenType::Nominal(k) => Some((String::new(), super::nominal::fill(super::nominal::get(k)?.observe, v, 0))),
            GenType::List(e) => Some((String::new(), format!("list.map({v}, {})", inner(e)?))),
            GenType::Option(e) => Some((String::new(), format!("option.map({v}, {})", inner(e)?))),
            GenType::Result(e) => Some((String::new(), format!("result.map({v}, {})", inner(e)?))),
            GenType::Tuple2(a, b) => {
                let (l, r) = (format!("{x}a"), format!("{x}b"));
                let (pa, ea) = a.observe(&l, depth + 1)?;
                let (pb, eb) = b.observe(&r, depth + 1)?;
                Some((format!("let ({l}, {r}) = {v}\n  {pa}{pb}"), format!("({ea}, {eb})")))
            }
            GenType::Tuple3(a, b, c) => {
                let (l, m, r) = (format!("{x}a"), format!("{x}b"), format!("{x}c"));
                let (pa, ea) = a.observe(&l, depth + 1)?;
                let (pb, eb) = b.observe(&m, depth + 1)?;
                let (pc, ec) = c.observe(&r, depth + 1)?;
                Some((format!("let ({l}, {m}, {r}) = {v}\n  {pa}{pb}{pc}"), format!("({ea}, {eb}, {ec})")))
            }
            _ => None,
        }
    }

    /// [`observe`](Self::observe) calls an effect closure somewhere inside.
    pub fn observe_calls_effect(&self) -> bool {
        match self {
            GenType::Nominal(k) => super::nominal::get(k).is_some_and(|n| n.observe_calls_effect),
            GenType::ResultNom(e, k) => e.observe_calls_effect() || GenType::Nominal(k).observe_calls_effect(),
            GenType::List(e) | GenType::Option(e) | GenType::Result(e) | GenType::Map(e) | GenType::Set(e) | GenType::Secret(e) => e.observe_calls_effect(),
            GenType::Tuple2(a, b) => a.observe_calls_effect() || b.observe_calls_effect(),
            GenType::Tuple3(a, b, c) => a.observe_calls_effect() || b.observe_calls_effect() || c.observe_calls_effect(),
            _ => false,
        }
    }

    /// A value of this type may not be bound with `var` ([`super::nominal::Nominal::refuses_var`]).
    pub fn refuses_var(&self) -> bool {
        match self {
            GenType::Nominal(k) => super::nominal::get(k).is_some_and(|n| n.refuses_var),
            GenType::ResultNom(e, k) => e.refuses_var() || GenType::Nominal(k).refuses_var(),
            GenType::List(e) | GenType::Option(e) | GenType::Result(e) | GenType::Map(e) | GenType::Set(e) | GenType::Secret(e) => e.refuses_var(),
            GenType::Tuple2(a, b) => a.refuses_var() || b.refuses_var(),
            GenType::Tuple3(a, b, c) => a.refuses_var() || b.refuses_var() || c.refuses_var(),
            _ => false,
        }
    }

    /// The type of [`observe`](Self::observe)'s expression.
    pub fn observed(&self) -> Option<GenType> {
        if self.is_observable() {
            return Some(self.clone());
        }
        let inner = |e: &GenType| e.observed().map(Box::new);
        Some(match self {
            GenType::Bytes => GenType::List(Box::new(GenType::Int)),
            GenType::Matrix => GenType::List(Box::new(GenType::List(Box::new(GenType::Float)))),
            GenType::Value => GenType::String,
            GenType::Nominal(k) => (super::nominal::get(k)?.observed)(),
            GenType::List(e) => GenType::List(inner(e)?),
            GenType::Option(e) => GenType::Option(inner(e)?),
            GenType::Secret(e) => return e.observed(),
            GenType::Result(e) | GenType::ResultNom(e, _) => GenType::Result(inner(e)?),
            GenType::Tuple2(a, b) => GenType::Tuple2(inner(a)?, inner(b)?),
            GenType::Tuple3(a, b, c) => GenType::Tuple3(inner(a)?, inner(b)?, inner(c)?),
            _ => return None,
        })
    }

    pub fn is_observable(&self) -> bool {
        match self {
            GenType::Int | GenType::String | GenType::Bool | GenType::Unit => true,
            // Float is observable but its *formatting* is a divergence
            // we WANT to surface — so it is allowed.
            GenType::Float => true,
            GenType::List(e) | GenType::Option(e) | GenType::Result(e) => e.is_observable(),
            GenType::Tuple2(a, b) => a.is_observable() && b.is_observable(),
            GenType::Tuple3(a, b, c) => a.is_observable() && b.is_observable() && c.is_observable(),
            GenType::Map(v) => v.is_observable(),
            GenType::Sized(_) => true,
            GenType::Set(e) => e.is_observable(),
            // A Bytes or Matrix value has no string form (E089); a Value is
            // observed through `json.stringify`.
            GenType::Bytes | GenType::Matrix | GenType::Value => false,
            // No string form of its own: observed through its readers.
            GenType::Nominal(_) | GenType::ResultNom(..) => false,
            // Prints `Secret(***)`: observed through `secret.reveal`.
            GenType::Secret(_) => false,
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
            | GenType::Unit
            | GenType::Sized(_)
            | GenType::Bytes
            | GenType::Matrix
            | GenType::Value => 1,
            | GenType::Nominal(_) => 1,
            GenType::List(e)
            | GenType::Option(e)
            | GenType::Result(e)
            | GenType::ResultNom(e, _)
            | GenType::Map(e)
            | GenType::Set(e)
            | GenType::Secret(e) => {
                1 + e.depth()
            }
            GenType::Tuple2(a, b) => 1 + a.depth().max(b.depth()),
            GenType::Tuple3(a, b, c) => 1 + a.depth().max(b.depth()).max(c.depth()),
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
pub fn render_print(name: &str, ty: &GenType) -> String {
    // A Bytes or Matrix has no string form (E089), at the top level or inside
    // a list, option, result or tuple: print its observable image instead.
    // The image is a call, bound to a typed `let` first: a call interpolated
    // in `${…}` whose type is a Result is E041 (ADR-0008), #3636.
    if !ty.is_observable() {
        if let (Some((pre, expr)), Some(seen)) = (ty.observe(name, 0), ty.observed()) {
            return format!("{pre}let {name}_o: {} = {expr}\n  println(\"{name} = ${{{name}_o}}\")", seen.render());
        }
    }
    // Every other printable shape interpolates via `${...}`, so the type is
    // not needed to render — the parameter documents the contract that only
    // observable types reach here (enforced by the caller). The label
    // keeps multi-statement programs' outputs aligned to their producing
    // binding for triage.
    let mut out = String::new();
    let _ = write!(out, "println(\"{name} = ${{{name}}}\")");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #3636: the observed image of a non-printable value is a call, and a
    /// call interpolated inside `${…}` whose type is a Result is E041
    /// (base64.decode, hex.decode, json.parse, value.as_array in
    /// the inspection diamond). It is bound to a typed `let` first.
    #[test]
    fn a_non_printable_value_is_bound_to_a_typed_let_before_it_is_printed() {
        let p = render_print("d", &GenType::Result(Box::new(GenType::Bytes)));
        assert_eq!(
            p,
            "let d_o: Result[List[Int], String] = result.map(d, ((o0) => bytes.to_list(o0)))\n  println(\"d = ${d_o}\")"
        );
        let p = render_print("v", &GenType::Option(Box::new(GenType::Matrix)));
        assert!(p.starts_with("let v_o: Option[List[List[Float]]] = option.map(v, "), "{p}");
        let p = render_print("w", &GenType::Value);
        assert_eq!(p, "let w_o: String = json.stringify(w)\n  println(\"w = ${w_o}\")");
        // A printable value is interpolated as it is.
        assert_eq!(render_print("x", &GenType::Int), "println(\"x = ${x}\")");
    }
}
