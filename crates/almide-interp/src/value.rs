//! The interpreter value model.
//!
//! `Value` is the dynamic runtime representation the tree-walker manipulates.
//! Heap kinds share interior state via `Rc` so closure-capture and `Assign`
//! stay cheap — observationally matching native `RcCow` semantics.
//!
//! Two display modes, both derived from inspecting codegen so they are
//! byte-identical to the native runtime:
//!   - [`Value::display_bare`]  — the `println` / Display form.
//!   - [`Value::almide_repr`]   — the compound / container form, replicating
//!     `almide_repr_prelude` (crates/almide-codegen/src/lib.rs) exactly.

use std::rc::Rc;
use almide_base::intern::Sym;

/// A closure value: a lambda body plus the environment it captured.
#[derive(Clone)]
pub struct Closure {
    /// Parameter VarIds in declaration order.
    pub params: Vec<almide_ir::VarId>,
    /// The lambda body to evaluate on application.
    pub body: Rc<almide_ir::IrExpr>,
    /// Captured environment snapshot (one frame holding all free vars).
    pub captured: crate::env::Scope,
}

impl std::fmt::Debug for Closure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<closure/{}>", self.params.len())
    }
}

/// The payload carried by an algebraic-data-type constructor value.
#[derive(Debug, Clone, PartialEq)]
pub enum VariantPayload {
    Unit,
    Tuple(Vec<Value>),
    Record(Vec<(Sym, Value)>),
}

#[derive(Debug, Clone)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Rc<String>),
    List(Rc<Vec<Value>>),
    Tuple(Rc<Vec<Value>>),
    /// Insertion-ordered dense entries — matches the WASM compact-ordered-dict
    /// and the native `AlmideMap`. Equality is order-INDEPENDENT (matching
    /// std HashMap and both backends, #556); iteration/repr preserve order.
    Map(Rc<Vec<(Value, Value)>>),
    /// Insertion-ordered, dedup-on-insert.
    Set(Rc<Vec<Value>>),
    Record {
        name: Option<Sym>,
        fields: Rc<Vec<(Sym, Value)>>,
    },
    Variant {
        ty: Option<Sym>,
        ctor: Sym,
        payload: VariantPayload,
    },
    Option(Option<Box<Value>>),
    Result(Result<Box<Value>, Box<Value>>),
    Closure(Rc<Closure>),
    /// A lazy integer range; materialized to a `List` by iteration / list ops.
    Range { start: i64, end: i64, inclusive: bool },
}

impl Value {
    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(Rc::new(s.into()))
    }
    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(items))
    }
    pub fn tuple(items: Vec<Value>) -> Value {
        Value::Tuple(Rc::new(items))
    }

    /// Materialize a `Range` (or pass through a `List`) to a concrete element
    /// vector for iteration / container ops. Returns `None` for non-iterables.
    pub fn as_iter_items(&self) -> Option<Vec<Value>> {
        match self {
            Value::List(xs) => Some((**xs).clone()),
            Value::Set(xs) => Some((**xs).clone()),
            Value::Range { start, end, inclusive } => {
                // #561: defensive cap — for-in iterates ranges LAZILY
                // (eval.rs::eval_for_in_range), so this materializing path is
                // only reached by repr/eq of a Range, where a multi-billion
                // range would OOM. Bound it; a range this large in a repr/eq
                // position is degenerate and the interp (an abstaining oracle)
                // need not be exact there.
                const MAX_RANGE_MATERIALIZE: i64 = 16 * 1024 * 1024;
                let last = if *inclusive { *end } else { *end - 1 };
                let count = (last - *start + 1).max(0).min(MAX_RANGE_MATERIALIZE);
                let mut out = Vec::with_capacity(count as usize);
                let mut i = *start;
                let stop = *start + count;
                while i < stop {
                    out.push(Value::Int(i));
                    i += 1;
                }
                Some(out)
            }
            // Map iterates as (k, v) tuples (matches native `for (k,v) in m`).
            Value::Map(entries) => Some(
                entries
                    .iter()
                    .map(|(k, v)| Value::tuple(vec![k.clone(), v.clone()]))
                    .collect(),
            ),
            _ => None,
        }
    }

    /// A human label for the value's kind — used only in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Unit => "Unit",
            Value::Bool(_) => "Bool",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Str(_) => "String",
            Value::List(_) => "List",
            Value::Tuple(_) => "Tuple",
            Value::Map(_) => "Map",
            Value::Set(_) => "Set",
            Value::Record { .. } => "Record",
            Value::Variant { .. } => "Variant",
            Value::Option(_) => "Option",
            Value::Result(_) => "Result",
            Value::Closure(_) => "Closure",
            Value::Range { .. } => "Range",
        }
    }
}

// ── Equality ────────────────────────────────────────────────────
//
// Mirrors `almide_eq!` (= `==`) which on the runtime types is a derived
// structural `PartialEq`. Closures are never equatable (matches native: a
// closure value has no PartialEq) — comparing them yields `false`.

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        use Value::*;
        match (self, other) {
            (Unit, Unit) => true,
            (Bool(a), Bool(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            // f64 PartialEq: NaN != NaN, matching Rust `==` semantics that
            // codegen lowers `almide_eq!` to.
            (Float(a), Float(b)) => a == b,
            (Str(a), Str(b)) => a == b,
            (List(a), List(b)) => a == b,
            (Tuple(a), Tuple(b)) => a == b,
            // #556: Map/Set `==` is order-INDEPENDENT on both backends
            // (runtime/rs/src/{map,set}.rs match std HashMap/HashSet); comparing
            // the ordered entry vecs cast a FALSE clean vote as the third judge
            // (`["a":1,"b":2] == ["b":2,"a":1]` → backends true, interp false).
            (Map(a), Map(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| {
                        b.iter().any(|(k2, v2)| k == k2 && v == v2)
                    })
            }
            (Set(a), Set(b)) => {
                a.len() == b.len()
                    && a.iter().all(|x| b.iter().any(|y| x == y))
            }
            (
                Record { name: n1, fields: f1 },
                Record { name: n2, fields: f2 },
            ) => n1 == n2 && f1 == f2,
            (
                Variant { ty: t1, ctor: c1, payload: p1 },
                Variant { ty: t2, ctor: c2, payload: p2 },
            ) => t1 == t2 && c1 == c2 && p1 == p2,
            (Option(a), Option(b)) => a == b,
            (Result(a), Result(b)) => a == b,
            (
                Range { start: s1, end: e1, inclusive: i1 },
                Range { start: s2, end: e2, inclusive: i2 },
            ) => s1 == s2 && e1 == e2 && i1 == i2,
            // A materialized range compares equal to the equivalent list.
            (Range { .. }, List(_)) | (List(_), Range { .. }) => {
                self.as_iter_items() == other.as_iter_items()
            }
            (Closure(_), Closure(_)) => false,
            _ => false,
        }
    }
}

// ── Ordering (for <, >, <=, >= and sort) ────────────────────────

impl Value {
    /// Total-ish ordering over comparable kinds. Returns `None` when the two
    /// values are of different/incomparable kinds (the caller turns that into
    /// a runtime abort, matching the type-checker's guarantee that codegen
    /// only ever compares same-typed operands).
    pub fn partial_cmp_val(&self, other: &Value) -> Option<std::cmp::Ordering> {
        use Value::*;
        match (self, other) {
            (Int(a), Int(b)) => Some(a.cmp(b)),
            (Float(a), Float(b)) => a.partial_cmp(b),
            (Str(a), Str(b)) => Some(a.cmp(b)),
            (Bool(a), Bool(b)) => Some(a.cmp(b)),
            (Unit, Unit) => Some(std::cmp::Ordering::Equal),
            (List(a), List(b)) => Self::cmp_seq(a, b),
            (Tuple(a), Tuple(b)) => Self::cmp_seq(a, b),
            _ => None,
        }
    }

    fn cmp_seq(a: &[Value], b: &[Value]) -> Option<std::cmp::Ordering> {
        for (x, y) in a.iter().zip(b.iter()) {
            match x.partial_cmp_val(y)? {
                std::cmp::Ordering::Equal => continue,
                ord => return Some(ord),
            }
        }
        Some(a.len().cmp(&b.len()))
    }

    /// TOTAL order used by the list ordering ops (`list.sort`/`min`/`max`/
    /// `sort_by`) — distinct from `partial_cmp_val`, which the SCALAR `<`/`>`
    /// operators use and which keeps IEEE partiality for NaN (C-049). Here
    /// Float compares by IEEE-754 totalOrder (`f64::total_cmp`: NaN at the top,
    /// `-0.0 < +0.0`), mirroring native's `_float` runtime variants and the
    /// wasm sign-magnitude bit trick, so the 3-way oracle agrees on
    /// `List[Float]` ordering (C-055). Returns `None` only for genuinely
    /// incomparable shapes (the sort then reports a non-comparable abort).
    pub fn total_cmp_val(&self, other: &Value) -> Option<std::cmp::Ordering> {
        use Value::*;
        match (self, other) {
            (Float(a), Float(b)) => Some(a.total_cmp(b)),
            (List(a), List(b)) => Self::total_cmp_seq(a, b),
            (Tuple(a), Tuple(b)) => Self::total_cmp_seq(a, b),
            _ => self.partial_cmp_val(other),
        }
    }

    fn total_cmp_seq(a: &[Value], b: &[Value]) -> Option<std::cmp::Ordering> {
        for (x, y) in a.iter().zip(b.iter()) {
            match x.total_cmp_val(y)? {
                std::cmp::Ordering::Equal => continue,
                ord => return Some(ord),
            }
        }
        Some(a.len().cmp(&b.len()))
    }
}

// ── Display: the bare `println` / Display form ──────────────────

impl Value {
    /// The bare top-level Display form (what `println(x)` / a bare `${x}`
    /// produces). Strings render raw (no quotes); compound values render via
    /// `almide_repr`. This is the `almide_rt_value_stringify` / Display
    /// contract.
    pub fn display_bare(&self) -> String {
        match self {
            Value::Str(s) => (**s).clone(),
            Value::Int(n) => n.to_string(),
            // EMPIRICAL (probe /tmp/float_probe.almd on almide 0.24.0): a bare
            // `${f}` of a Float renders via Rust `{}` Display (`3`, not `3.0`),
            // NOT via `almide_rt_float_to_string`. The `.0`-suffixed form is
            // produced ONLY by an explicit `float.to_string(f)` call. So both
            // the bare and the repr float paths use plain `{}`.
            Value::Float(f) => format!("{}", f),
            Value::Bool(b) => b.to_string(),
            Value::Unit => "()".to_string(),
            // Compound / container values use the repr form even at top level.
            _ => self.almide_repr(),
        }
    }

    /// The compound / container form, byte-identical to `almide_repr_prelude`
    /// (crates/almide-codegen/src/lib.rs:370). Used for compound string-interp
    /// parts and for any value nested inside a container.
    pub fn almide_repr(&self) -> String {
        match self {
            Value::Unit => "()".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            // Prelude: `impl AlmideRepr for f64 { format!("{}", self) }` — plain
            // Display, NO `.0` suffix (verified: `[3, 1.5]`, `some(3)`).
            Value::Float(f) => format!("{}", f),
            // A string inside a container is double-quoted + escaped with the
            // exact set `\\ \" \n \r \t`.
            Value::Str(s) => repr_str(s),
            Value::List(xs) => repr_seq("[", "]", xs),
            // A range renders as its materialized list.
            Value::Range { .. } => {
                let items = self.as_iter_items().unwrap_or_default();
                repr_seq("[", "]", &items)
            }
            Value::Tuple(xs) => repr_seq("(", ")", xs),
            Value::Set(xs) => repr_seq("[", "]", xs),
            Value::Map(entries) => {
                // Map: `["k": v]` insertion order (runtime/rs/src/map.rs:74),
                // empty map `[:]`.
                if entries.is_empty() {
                    return "[:]".to_string();
                }
                let mut o = String::from("[");
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        o.push_str(", ");
                    }
                    o.push_str(&k.almide_repr());
                    o.push_str(": ");
                    o.push_str(&v.almide_repr());
                }
                o.push(']');
                o
            }
            Value::Option(opt) => match opt {
                Some(v) => format!("some({})", v.almide_repr()),
                None => "none".to_string(),
            },
            Value::Result(res) => match res {
                Ok(v) => format!("ok({})", v.almide_repr()),
                Err(e) => format!("err({})", e.almide_repr()),
            },
            Value::Record { name, fields } => repr_record(*name, fields),
            Value::Variant { ctor, payload, .. } => match payload {
                VariantPayload::Unit => ctor.to_string(),
                VariantPayload::Tuple(items) => {
                    let inner = items
                        .iter()
                        .map(|v| v.almide_repr())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{}({})", ctor, inner)
                }
                VariantPayload::Record(fs) => {
                    repr_record(Some(*ctor), fs)
                }
            },
            Value::Closure(_) => "<closure>".to_string(),
        }
    }
}

/// `[a, b]` / `(a, b)` — comma-joined `almide_repr` of each element, empty
/// container renders as just the delimiters.
fn repr_seq(open: &str, close: &str, xs: &[Value]) -> String {
    let mut o = String::from(open);
    for (i, e) in xs.iter().enumerate() {
        if i > 0 {
            o.push_str(", ");
        }
        o.push_str(&e.almide_repr());
    }
    o.push_str(close);
    o
}

/// `Name { f: v, g: w }` in field-declaration order; anonymous records omit
/// the leading name (`{ f: v }`). Matches walker/declarations.rs.
fn repr_record(name: Option<Sym>, fields: &[(Sym, Value)]) -> String {
    let mut o = String::new();
    if let Some(n) = name {
        o.push_str(n.as_str());
        o.push(' ');
    }
    o.push('{');
    for (i, (k, v)) in fields.iter().enumerate() {
        o.push_str(if i == 0 { " " } else { ", " });
        o.push_str(k.as_str());
        o.push_str(": ");
        o.push_str(&v.almide_repr());
    }
    o.push_str(" }");
    o
}

/// Escape a string for container context — the exact replacement chain from
/// `almide_repr_str` (crates/almide-codegen/src/lib.rs:376).
fn repr_str(sv: &str) -> String {
    let escaped = sv
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{}\"", escaped)
}

/// Replicates `almide_rt_float_to_string` (runtime/rs/src/float.rs:3) exactly:
/// Rust `{}` Display, with a `.0` suffix appended for whole-number floats so
/// `3.0` does not print as `3`.
///
/// IMPORTANT: this is the formatter for an *explicit* `float.to_string(f)`
/// call ONLY. Display / `almide_repr` of a Float use plain `{}` (no `.0`) —
/// see [`Value::display_bare`] / [`Value::almide_repr`]. The known native↔WASM
/// float divergence (`0.30000000000000004` vs `0.3`) lives in the underlying
/// `{}` Display, so the interp inherits native's shortest-roundtrip here.
pub fn float_to_string(n: f64) -> String {
    let s = format!("{}", n);
    if n.fract() == 0.0 && !s.contains('.') && !s.contains("inf") && !s.contains("NaN") {
        format!("{}.0", s)
    } else {
        s
    }
}
