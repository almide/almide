//! Signature types — the type language of catalogued stdlib functions.
//!
//! A [`SigType`] is like a [`GenType`](super::types::GenType) but may
//! contain *type variables* (`A`, `B`, `K`, …) standing for a generic
//! parameter. The generator instantiates a signature by choosing a
//! concrete `GenType` for each variable, consistent with whatever the
//! goal return type demands — a one-pass unification.
//!
//! Conversion from the AST `TypeExpr` is partial on purpose: returning
//! `None` for any shape the generator cannot synthesize a value of
//! (records, variants, bytes/matrix, the fixed-width integer family) is
//! how the catalogue filters its admissible surface.

use std::collections::HashMap;

use almide::ast::TypeExpr;

use super::types::GenType;

/// If `slot` is a bare type variable, force-bind it to `String` in
/// `out` (the concrete type `instantiate` uses for Map keys / Result
/// errors). A non-variable slot needs no forcing.
fn bind_slot_to_string(slot: &SigType, out: &mut HashMap<String, GenType>) {
    if let SigType::Var(n) = slot {
        out.entry(n.clone()).or_insert(GenType::String);
    }
}

/// A type that may be generic over single-letter type variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigType {
    Int,
    Float,
    String,
    Bool,
    Unit,
    /// A generic type variable, e.g. `A` in `list.map[A, B]`.
    Var(String),
    List(Box<SigType>),
    Option(Box<SigType>),
    /// `Result[ok, E]` — the error type is erased to its own slot and
    /// concretizes to `String`, or to a nominal when the signature names one
    /// (`Result[T, HttpResponse]`); see [`SigType::instantiate`].
    Result(Box<SigType>, Box<SigType>),
    Tuple2(Box<SigType>, Box<SigType>),
    /// `(a, b, c)` — see [`GenType::Tuple3`].
    Tuple3(Box<SigType>, Box<SigType>, Box<SigType>),
    Map(Box<SigType>, Box<SigType>),
    Set(Box<SigType>),
    /// `(p...) -> r` — a function/closure parameter (HOF position).
    Fn(Vec<SigType>, Box<SigType>),
    /// A fixed-width number type (`Int8` … `Float64`).
    Sized(&'static str),
    Bytes,
    Matrix,
    Value,
    /// A stdlib nominal type the grammar builds ([`super::nominal`]).
    Nominal(&'static str),
    /// `secret.Secret[A]` (#4107): built with `secret.new`, read with `secret.reveal`.
    Secret(Box<SigType>),
    /// `Never`: the call does not return. It has no value, so no goal
    /// unifies with it; a program calls such a function only as its last
    /// statement ([`super::catalogue::NEVER_RETURNING`]).
    Never,
}

/// Single-letter (or short) names the stdlib uses for generic type
/// parameters. We treat a `TypeExpr::Simple` whose name is one of these
/// as a type variable rather than a nominal type.
const TYPE_VAR_NAMES: &[&str] = &["A", "B", "C", "K", "V", "T", "U", "E", "F"];

impl SigType {
    /// Convert an AST type expression into a `SigType`, or `None` if the
    /// shape is outside the generator's universe.
    pub fn from_type_expr(t: &TypeExpr) -> Option<SigType> {
        match t {
            TypeExpr::Simple { name } => {
                let n = name.as_str();
                if TYPE_VAR_NAMES.contains(&n) {
                    return Some(SigType::Var(n.to_string()));
                }
                match n {
                    "Int" => Some(SigType::Int),
                    "Float" => Some(SigType::Float),
                    "String" => Some(SigType::String),
                    "Bool" => Some(SigType::Bool),
                    "Unit" => Some(SigType::Unit),
                    "Never" => Some(SigType::Never),
                    "Bytes" => Some(SigType::Bytes),
                    "Matrix" => Some(SigType::Matrix),
                    "Value" => Some(SigType::Value),
                    // The fixed-width numbers the value grammar builds; other
                    // nominal types enter via mutation.
                    other => match GenType::sized(other) {
                        Some(GenType::Sized(s)) => Some(SigType::Sized(s)),
                        _ => super::nominal::get(other).map(|k| SigType::Nominal(k.name)),
                    },
                }
            }
            TypeExpr::Generic { name, args } => {
                let n = name.as_str();
                match (n, args.len()) {
                    ("List", 1) => {
                        Some(SigType::List(Box::new(Self::from_type_expr(&args[0])?)))
                    }
                    // `A?` parses as `Generic { name: "?" }`: without this arm
                    // every `option.*` and every `A?`-returning `list.get` /
                    // `first` / `find` was outside the catalogue.
                    ("Option" | "?", 1) => {
                        Some(SigType::Option(Box::new(Self::from_type_expr(&args[0])?)))
                    }
                    ("Result", 2) => Some(SigType::Result(
                        Box::new(Self::from_type_expr(&args[0])?),
                        Box::new(Self::from_type_expr(&args[1])?),
                    )),
                    ("Map", 2) => Some(SigType::Map(
                        Box::new(Self::from_type_expr(&args[0])?),
                        Box::new(Self::from_type_expr(&args[1])?),
                    )),
                    // A Set is built with `set.from_list`; its element
                    // must be hashable (checked at `instantiate`).
                    ("Set", 1) => Some(SigType::Set(Box::new(Self::from_type_expr(&args[0])?))),
                    ("Secret", 1) => Some(SigType::Secret(Box::new(Self::from_type_expr(&args[0])?))),
                    // A nominal spelled with arguments (`Matrix[Float32]`).
                    (_, 1) => match &args[0] {
                        TypeExpr::Simple { name: arg } => {
                            super::nominal::get(&format!("{n}[{}]", arg.as_str())).map(|k| SigType::Nominal(k.name))
                        }
                        _ => None,
                    },
                    _ => None,
                }
            }
            TypeExpr::Fn { params, ret, .. } => {
                let mut ps = Vec::with_capacity(params.len());
                for p in params {
                    ps.push(Self::from_type_expr(p)?);
                }
                Some(SigType::Fn(ps, Box::new(Self::from_type_expr(ret)?)))
            }
            TypeExpr::Tuple { elements } if elements.len() == 2 => Some(SigType::Tuple2(
                Box::new(Self::from_type_expr(&elements[0])?),
                Box::new(Self::from_type_expr(&elements[1])?),
            )),
            TypeExpr::Tuple { elements } if elements.len() == 3 => Some(SigType::Tuple3(
                Box::new(Self::from_type_expr(&elements[0])?),
                Box::new(Self::from_type_expr(&elements[1])?),
                Box::new(Self::from_type_expr(&elements[2])?),
            )),
            // Records, variants, unions, wider tuples, const-literals:
            // outside the universe.
            _ => None,
        }
    }

    /// Collect the names of all type variables appearing in this type.
    pub fn collect_vars(&self, out: &mut Vec<String>) {
        match self {
            SigType::Var(n) => {
                if !out.contains(n) {
                    out.push(n.clone());
                }
            }
            SigType::List(e) | SigType::Option(e) | SigType::Set(e) | SigType::Secret(e) => e.collect_vars(out),
            SigType::Result(a, b) | SigType::Tuple2(a, b) | SigType::Map(a, b) => {
                a.collect_vars(out);
                b.collect_vars(out);
            }
            SigType::Tuple3(a, b, c) => {
                a.collect_vars(out);
                b.collect_vars(out);
                c.collect_vars(out);
            }
            SigType::Fn(ps, r) => {
                for p in ps {
                    p.collect_vars(out);
                }
                r.collect_vars(out);
            }
            _ => {}
        }
    }

    /// Record forced bindings for type variables that sit in a position
    /// `instantiate` pins to a concrete type regardless of the chosen
    /// substitution — namely a `Map` *key* slot and a `Result` *error*
    /// slot, both of which `instantiate` lowers to `String`.
    ///
    /// Without this, the generator can pick (say) `K = Bool` for a free
    /// key variable, then emit a `Map[String, …]` literal AND a closure
    /// whose first parameter is typed `Bool` — an inconsistency the
    /// checker rejects. Pre-binding these vars to `String` keeps the
    /// closure parameter types aligned with the value the generator
    /// actually builds.
    pub fn collect_forced_bindings(&self, out: &mut HashMap<String, GenType>) {
        match self {
            // Map key slot ⇒ String (mirrors `instantiate`'s GenType::Map).
            SigType::Map(k, v) => {
                bind_slot_to_string(k, out);
                v.collect_forced_bindings(out);
            }
            // Result error slot ⇒ String (mirrors GenType::Result).
            SigType::Result(ok, err) => {
                bind_slot_to_string(err, out);
                ok.collect_forced_bindings(out);
            }
            SigType::List(e) | SigType::Option(e) | SigType::Set(e) | SigType::Secret(e) => e.collect_forced_bindings(out),
            SigType::Tuple2(a, b) => {
                a.collect_forced_bindings(out);
                b.collect_forced_bindings(out);
            }
            SigType::Tuple3(a, b, c) => {
                a.collect_forced_bindings(out);
                b.collect_forced_bindings(out);
                c.collect_forced_bindings(out);
            }
            SigType::Fn(ps, r) => {
                for p in ps {
                    p.collect_forced_bindings(out);
                }
                r.collect_forced_bindings(out);
            }
            _ => {}
        }
    }

    /// Substitute concrete `GenType`s for type variables, producing a
    /// fully concrete `GenType`. Returns `None` if a needed variable is
    /// unbound, or if the substituted shape is somehow not a valid
    /// `GenType` (e.g. a bare `Fn` in value position — functions are
    /// only valid as HOF *parameters*, handled by the generator, never
    /// as a goal value).
    pub fn instantiate(&self, subst: &HashMap<String, GenType>) -> Option<GenType> {
        match self {
            SigType::Int => Some(GenType::Int),
            SigType::Float => Some(GenType::Float),
            SigType::String => Some(GenType::String),
            SigType::Bool => Some(GenType::Bool),
            SigType::Unit => Some(GenType::Unit),
            SigType::Sized(s) => Some(GenType::Sized(s)),
            SigType::Bytes => Some(GenType::Bytes),
            SigType::Matrix => Some(GenType::Matrix),
            SigType::Value => Some(GenType::Value),
            SigType::Nominal(k) => Some(GenType::Nominal(k)),
            SigType::Var(n) => subst.get(n).cloned(),
            SigType::List(e) => Some(GenType::List(Box::new(e.instantiate(subst)?))),
            SigType::Option(e) => Some(GenType::Option(Box::new(e.instantiate(subst)?))),
            SigType::Secret(e) => Some(GenType::Secret(Box::new(e.instantiate(subst)?))),
            // The error slot is fixed to String regardless of the bound
            // variable, or is the nominal the signature names
            // (`Result[T, HttpResponse]`, GenType::ResultNom). Any other
            // concrete error type is outside the universe: rendered as
            // `Result[T, String]` it would be a type error, not a program.
            SigType::Result(ok, err) => match **err {
                SigType::Var(_) | SigType::String => Some(GenType::Result(Box::new(ok.instantiate(subst)?))),
                SigType::Nominal(k) => Some(GenType::ResultNom(Box::new(ok.instantiate(subst)?), k)),
                _ => None,
            },
            SigType::Tuple2(a, b) => Some(GenType::Tuple2(
                Box::new(a.instantiate(subst)?),
                Box::new(b.instantiate(subst)?),
            )),
            SigType::Tuple3(a, b, c) => Some(GenType::Tuple3(
                Box::new(a.instantiate(subst)?),
                Box::new(b.instantiate(subst)?),
                Box::new(c.instantiate(subst)?),
            )),
            SigType::Map(_k, v) => Some(GenType::Map(Box::new(v.instantiate(subst)?))),
            SigType::Set(e) => Some(e.instantiate(subst)?).filter(GenType::is_set_elem).map(|e| GenType::Set(Box::new(e))),
            // A function type is not a producible value, nor is Never.
            SigType::Fn(_, _) | SigType::Never => None,
        }
    }

    /// Try to unify this signature type against a concrete goal
    /// `GenType`, recording variable bindings into `subst`. Returns
    /// `false` on a structural mismatch or an inconsistent binding.
    ///
    /// Used to pin a function's return type to the goal type the
    /// generator is currently filling, which in turn constrains the
    /// type variables shared with the parameters.
    pub fn unify_with_goal(&self, goal: &GenType, subst: &mut HashMap<String, GenType>) -> bool {
        match (self, goal) {
            (SigType::Int, GenType::Int)
            | (SigType::Float, GenType::Float)
            | (SigType::String, GenType::String)
            | (SigType::Bool, GenType::Bool)
            | (SigType::Unit, GenType::Unit)
            | (SigType::Bytes, GenType::Bytes)
            | (SigType::Matrix, GenType::Matrix)
            | (SigType::Value, GenType::Value) => true,
            (SigType::Nominal(a), GenType::Nominal(b)) => a == b,
            (SigType::Sized(a), GenType::Sized(b)) => a == b,
            (SigType::Var(n), g) => match subst.get(n) {
                Some(bound) => bound == g,
                None => {
                    subst.insert(n.clone(), g.clone());
                    true
                }
            },
            (SigType::List(e), GenType::List(g)) => e.unify_with_goal(g, subst),
            (SigType::Option(e), GenType::Option(g)) => e.unify_with_goal(g, subst),
            (SigType::Secret(e), GenType::Secret(g)) => e.unify_with_goal(g, subst),
            // Result error slot: goal carries no error type (fixed
            // String), so only the ok arm is unified — unless the slot
            // names a nominal, whose goal is a ResultNom of that nominal.
            (SigType::Result(ok, err), GenType::Result(g)) => !matches!(**err, SigType::Nominal(_)) && ok.unify_with_goal(g, subst),
            (SigType::Result(ok, err), GenType::ResultNom(g, k)) => matches!(**err, SigType::Nominal(e) if e == *k) && ok.unify_with_goal(g, subst),
            (SigType::Tuple2(a, b), GenType::Tuple2(ga, gb)) => {
                a.unify_with_goal(ga, subst) && b.unify_with_goal(gb, subst)
            }
            (SigType::Tuple3(a, b, c), GenType::Tuple3(ga, gb, gc)) => {
                a.unify_with_goal(ga, subst) && b.unify_with_goal(gb, subst) && c.unify_with_goal(gc, subst)
            }
            (SigType::Map(_k, v), GenType::Map(gv)) => v.unify_with_goal(gv, subst),
            (SigType::Set(e), GenType::Set(g)) => e.unify_with_goal(g, subst),
            _ => false,
        }
    }
}
