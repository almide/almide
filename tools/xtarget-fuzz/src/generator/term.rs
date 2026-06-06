//! The typed term generator — builds an Almide expression of a *given*
//! goal type, well-typed by construction.
//!
//! ## Why a statement builder, not pure expressions
//!
//! Several Almide values are *ambiguous in bare expression position* and
//! only type-check with a binding annotation: `[]`, `[:]`, `none`, and
//! the `ok`/`err`/`some` constructors all need a `let v: T = …` context
//! for the checker to fix their element/error type. Emitting them inline
//! inside a larger expression fails `almide check`.
//!
//! So the generator threads a [`Builder`] that can *hoist* such a value
//! into a fresh, annotated `let` statement and return a reference to it.
//! The whole generated `main` body becomes a sequence of typed `let`
//! bindings (each able to reference earlier ones, building real
//! data-flow) terminated by `println` observables. This is both more
//! robust and a more realistic program shape than deeply nested
//! one-liners.
//!
//! ## Fuel
//!
//! Generation is fuel-bounded: `Builder::fuel` decrements on each
//! structural step, and at zero fuel the generator falls back to the
//! cheapest leaf for the goal type. This bounds program size and
//! guarantees termination without a magic depth cap.

use std::collections::HashMap;

use crate::rng::SplitMix64;

use super::catalogue::Signature;
use super::pools;
use super::sig_type::SigType;
use super::types::GenType;

/// One variable currently in lexical scope.
#[derive(Clone)]
pub struct ScopeVar {
    pub name: String,
    pub ty: GenType,
}

/// Accumulates the statements of the program body as the term generator
/// runs, and owns all shared generation state.
pub struct Builder<'a> {
    pub rng: &'a mut SplitMix64,
    /// Catalogued stdlib signatures (immutable for the whole campaign).
    pub catalogue: &'a [Signature],
    /// Variables in scope, innermost last.
    pub scope: Vec<ScopeVar>,
    /// Statements emitted so far (rendered Almide source lines, no
    /// trailing newline). The driver wraps these in `fn main`.
    pub stmts: Vec<String>,
    /// Remaining structural fuel; productions that recurse spend it.
    pub fuel: i32,
    /// Monotonic counter for fresh binding names (`v0`, `v1`, …).
    pub fresh: u32,
}

/// The maximum nesting fuel a single top-level expression starts with.
/// Chosen so terms reach interesting depth (a HOF over a mapped list)
/// without exploding compile time.
pub const START_FUEL: i32 = 6;

/// Goal-type recursion cap for the random goal types the driver picks,
/// so we never aim for e.g. `List[Map[String, Option[List[Int]]]]`.
pub const MAX_GOAL_DEPTH: u32 = 3;

impl<'a> Builder<'a> {
    /// Mint a fresh, unique variable name from a random stem.
    fn fresh_name(&mut self) -> String {
        let stem = self.rng.pick(pools::VAR_STEMS);
        let name = format!("{stem}{}", self.fresh);
        self.fresh += 1;
        name
    }

    /// In-scope variables whose type exactly equals `goal`.
    fn vars_of_type(&self, goal: &GenType) -> Vec<usize> {
        self.scope
            .iter()
            .enumerate()
            .filter(|(_, v)| &v.ty == goal)
            .map(|(i, _)| i)
            .collect()
    }

    /// Hoist `value_src` into a fresh annotated `let v: ty = value_src`
    /// statement, register the binding in scope, and return the name.
    /// Annotation is what makes ambiguous literals (`[]`, `none`, …)
    /// type-check.
    fn hoist(&mut self, ty: &GenType, value_src: String) -> String {
        let name = self.fresh_name();
        self.stmts
            .push(format!("let {name}: {} = {value_src}", ty.render()));
        self.scope.push(ScopeVar {
            name: name.clone(),
            ty: ty.clone(),
        });
        name
    }
}

/// Produce an expression of `goal` type, valid in bare expression
/// position (i.e. never an un-annotated ambiguous literal — those are
/// hoisted to a `let` and referenced). Returns Almide source text.
pub fn gen_expr(b: &mut Builder, goal: &GenType) -> String {
    b.fuel -= 1;
    if b.fuel <= 0 {
        return gen_leaf(b, goal);
    }

    let has_var = !b.vars_of_type(goal).is_empty();

    // Production menu as a weighted table — named weights keep the
    // steering legible (call-heavy, divergence-prone terms favoured).
    const W_LEAF: u32 = 3;
    const W_VAR: u32 = 5;
    const W_CALL: u32 = 8;
    const W_IF: u32 = 3;
    const W_CONSTRUCT: u32 = 5;

    #[derive(Clone, Copy)]
    enum Prod {
        Leaf,
        Var,
        Call,
        If,
        Construct,
    }

    let mut menu: Vec<(u32, Prod)> = vec![
        (W_LEAF, Prod::Leaf),
        (W_CALL, Prod::Call),
        (W_IF, Prod::If),
        (W_CONSTRUCT, Prod::Construct),
    ];
    if has_var {
        menu.push((W_VAR, Prod::Var));
    }

    let weights: Vec<u32> = menu.iter().map(|(w, _)| *w).collect();
    let choice = menu[b.rng.pick_weighted(&weights)].1;

    match choice {
        Prod::Leaf => gen_leaf(b, goal),
        Prod::Var => gen_var(b, goal).unwrap_or_else(|| gen_leaf(b, goal)),
        Prod::Call => gen_call(b, goal).unwrap_or_else(|| gen_construct(b, goal)),
        Prod::If => gen_if(b, goal),
        Prod::Construct => gen_construct(b, goal),
    }
}

/// A reference to an in-scope variable of the goal type, if any.
fn gen_var(b: &mut Builder, goal: &GenType) -> Option<String> {
    let candidates = b.vars_of_type(goal);
    if candidates.is_empty() {
        return None;
    }
    let idx = candidates[b.rng.below(candidates.len() as u32) as usize];
    Some(b.scope[idx].name.clone())
}

/// The cheapest terminal expression for a goal type: a literal, an
/// in-scope variable, or a hoisted construction. Always succeeds and is
/// always valid in bare position.
fn gen_leaf(b: &mut Builder, goal: &GenType) -> String {
    // Prefer an in-scope variable half the time (cheap data reuse).
    if b.rng.chance(1, 2) {
        if let Some(v) = gen_var(b, goal) {
            return v;
        }
    }
    match goal {
        GenType::Int => format!("{}", b.rng.pick(pools::INT_POOL)),
        GenType::Float => render_float_literal(*b.rng.pick(pools::FLOAT_POOL)),
        GenType::String => render_string_literal(b.rng.pick(pools::STRING_POOL)),
        GenType::Bool => format!("{}", b.rng.pick(pools::BOOL_POOL)),
        GenType::Unit => "()".to_string(),
        // Compound goals: construct (and hoist if ambiguous).
        GenType::List(_)
        | GenType::Option(_)
        | GenType::Result(_)
        | GenType::Tuple2(_, _)
        | GenType::Map(_) => gen_construct(b, goal),
    }
}

/// Construct a compound value. Ambiguous shapes (empty list/map, `none`,
/// `ok`/`err`) are hoisted into an annotated `let` so they type-check;
/// the function returns the *reference* to that binding. Unambiguous
/// shapes (non-empty list, tuple, `some(x)`) are returned inline.
fn gen_construct(b: &mut Builder, goal: &GenType) -> String {
    match goal {
        GenType::Int | GenType::Float | GenType::String | GenType::Bool | GenType::Unit => {
            gen_leaf(b, goal)
        }
        GenType::List(elem) => {
            let n = *b.rng.pick(pools::SMALL_COUNT_POOL) as usize;
            if n == 0 {
                // Empty list is ambiguous ⇒ hoist with annotation.
                return b.hoist(goal, "[]".to_string());
            }
            let items: Vec<String> = (0..n).map(|_| gen_expr(b, elem)).collect();
            format!("[{}]", items.join(", "))
        }
        GenType::Option(inner) => {
            if b.rng.chance(2, 3) {
                // `some(x)` is unambiguous when x's type is known.
                format!("some({})", gen_expr(b, inner))
            } else {
                b.hoist(goal, "none".to_string())
            }
        }
        GenType::Result(ok) => {
            // `ok(x)`/`err(s)` need the *other* arm's type ⇒ always hoist.
            if b.rng.chance(2, 3) {
                let v = gen_expr(b, ok);
                b.hoist(goal, format!("ok({v})"))
            } else {
                let s = render_string_literal(b.rng.pick(pools::STRING_POOL));
                b.hoist(goal, format!("err({s})"))
            }
        }
        GenType::Tuple2(a, b2) => {
            // A tuple literal is unambiguous from its element exprs.
            format!("({}, {})", gen_expr(b, a), gen_expr(b, b2))
        }
        GenType::Map(v) => {
            let n = *b.rng.pick(pools::SMALL_COUNT_POOL) as usize;
            if n == 0 {
                return b.hoist(goal, "[:]".to_string());
            }
            let mut entries = Vec::with_capacity(n);
            for i in 0..n {
                let key = format!("\"k{i}\"");
                entries.push(format!("{}: {}", key, gen_expr(b, v)));
            }
            format!("[{}]", entries.join(", "))
        }
    }
}

/// `if cond then A else B`, both arms of the goal type.
fn gen_if(b: &mut Builder, goal: &GenType) -> String {
    let cond = gen_expr(b, &GenType::Bool);
    let then = gen_expr(b, goal);
    let els = gen_expr(b, goal);
    format!("(if {cond} then {then} else {els})")
}

/// A stdlib call whose return type unifies with `goal`. Returns `None`
/// if no catalogued signature fits — the caller falls back to
/// construction.
fn gen_call(b: &mut Builder, goal: &GenType) -> Option<String> {
    // Candidate signatures whose return type unifies with the goal,
    // plus the substitution that makes it so.
    //
    // Pinned-slot pre-seeding: a type variable that sits in a `Map` *key*
    // or `Result` *error* slot is fixed to `String` by the value grammar
    // (`instantiate` lowers both to String). Such a variable can also
    // appear in a non-pinned return position — e.g. `result.to_err_option`
    // returns `Option[E]` and `map.keys` returns `List[K]`, where `E`/`K`
    // are the very slots pinned to String. If unification were allowed to
    // bind them freely from the goal (E = Float for an `Option[Float]`
    // goal), the generator would build a `Result[_, String]` value yet
    // call a function it believes returns `Option[Float]` — an ill-typed
    // emission the checker rejects (the dominant rung-a reject class).
    //
    // Seeding the forced `String` bindings *before* unification makes the
    // `Var` case in `unify_with_goal` require those slots to equal String:
    // a candidate whose pinned variable the goal demands be non-String is
    // correctly pruned, instead of producing a mismatch downstream.
    let mut candidates: Vec<(usize, HashMap<String, GenType>, u32)> = Vec::new();
    for (i, sig) in b.catalogue.iter().enumerate() {
        let mut subst = sig.forced_bindings();
        if sig.ret.unify_with_goal(goal, &mut subst) {
            candidates.push((i, subst, sig.weight));
        }
    }
    if candidates.is_empty() {
        return None;
    }

    let weights: Vec<u32> = candidates.iter().map(|(_, _, w)| *w).collect();
    let pick = b.rng.pick_weighted(&weights);
    let (sig_idx, subst, _) = candidates[pick].clone();
    let sig = b.catalogue[sig_idx].clone();

    let mut subst = subst;
    bind_free_vars(b, &sig, &mut subst)?;

    let mut args = Vec::with_capacity(sig.params.len());
    for (i, p) in sig.params.iter().enumerate() {
        let name = sig.param_names.get(i).map(String::as_str).unwrap_or("");
        args.push(gen_arg(b, p, name, &subst)?);
    }

    Some(format!("{}.{}({})", sig.module, sig.func, args.join(", ")))
}

/// Parameter names that denote a count/size/index and so must receive a
/// SMALL value, never a boundary `INT_POOL` extreme: a `repeat`/`pad`
/// count of `u32::MAX` (or a negative one) would exhaust memory or panic
/// on the allocation, manufacturing a noise "hang"/"capacity overflow"
/// that buries real divergences. These are the count-like names the
/// stdlib actually uses.
const COUNT_LIKE_PARAM_NAMES: &[&str] = &[
    "n", "count", "len", "times", "size", "decimals", "width", "k", "i", "j", "index",
    "start", "end", "lo", "hi",
];

/// Concrete types a free (parameter-only) type variable may take. Kept
/// to printable scalars so any HOF over them stays observable and cheap.
const FREE_VAR_CHOICES: &[GenType] = &[GenType::Int, GenType::String, GenType::Bool];

/// Bind every type variable in `sig` not already bound by return-type
/// unification to a concrete scalar type.
fn bind_free_vars(
    b: &mut Builder,
    sig: &Signature,
    subst: &mut HashMap<String, GenType>,
) -> Option<()> {
    // First, force-bind variables pinned by Map-key / Result-error slots
    // to String (the type `instantiate` lowers those slots to), so the
    // closure parameter types the generator emits match the values it
    // actually constructs. `gen_call` already seeds these before goal
    // unification, but this path is also reached when no unification ran;
    // `or_insert` keeps it idempotent and never overwrites a binding the
    // (String-consistent) goal unification already chose.
    for (k, v) in sig.forced_bindings() {
        subst.entry(k).or_insert(v);
    }

    // Then choose any still-unbound variable freely.
    let mut vars = Vec::new();
    for p in &sig.params {
        p.collect_vars(&mut vars);
    }
    sig.ret.collect_vars(&mut vars);
    for v in vars {
        if !subst.contains_key(&v) {
            let chosen =
                FREE_VAR_CHOICES[b.rng.below(FREE_VAR_CHOICES.len() as u32) as usize].clone();
            subst.insert(v, chosen);
        }
    }
    Some(())
}

/// Generate an argument expression for a (possibly generic) parameter,
/// given its name and the binding substitution. HOF parameters become
/// inline lambdas; count-like `Int` parameters get a small, safe literal.
fn gen_arg(
    b: &mut Builder,
    param: &SigType,
    param_name: &str,
    subst: &HashMap<String, GenType>,
) -> Option<String> {
    match param {
        SigType::Fn(params, ret) => Some(gen_lambda(b, params, ret, subst)),
        // Count/size/index Int parameter ⇒ a small non-pathological value
        // (avoids the `u32::MAX`/negative allocation-bomb noise).
        SigType::Int if COUNT_LIKE_PARAM_NAMES.contains(&param_name) => {
            Some(format!("{}", b.rng.pick(pools::SMALL_COUNT_POOL)))
        }
        other => {
            let concrete = other.instantiate(subst)?;
            Some(gen_expr(b, &concrete))
        }
    }
}

/// Generate an inline lambda `(p0, p1) => body` for a HOF argument.
///
/// This is the `fan.map`-class divergence surface: the body frequently
/// wraps a parameter in `some`/`ok`/string interpolation, exactly the
/// shapes where closure boxing has historically broken.
///
/// Lambda bodies must stay *single-expression* (no hoisting into the
/// enclosing statement list mid-argument), so we generate the body with
/// a private sub-builder whose hoisted statements are discarded — the
/// body falls back to inline-only constructions. To keep that sound we
/// give the body fuel that only produces inline-valid shapes.
fn gen_lambda(
    b: &mut Builder,
    params: &[SigType],
    ret: &SigType,
    subst: &HashMap<String, GenType>,
) -> String {
    // Bind fresh lambda parameter names in scope.
    let mut names = Vec::with_capacity(params.len());
    let mut pushed = 0usize;
    for p in params {
        let cty = p.instantiate(subst).unwrap_or(GenType::Int);
        let name = b.fresh_name();
        b.scope.push(ScopeVar {
            name: name.clone(),
            ty: cty,
        });
        pushed += 1;
        names.push(name);
    }

    let ret_concrete = ret.instantiate(subst).unwrap_or(GenType::Int);
    let body = gen_lambda_body(b, &ret_concrete);

    for _ in 0..pushed {
        b.scope.pop();
    }

    format!("(({}) => {})", names.join(", "), body)
}

/// Generate a lambda body — a single inline expression. Because a lambda
/// body cannot host `let` statements, this path never hoists: it draws
/// from variables in scope (incl. the lambda params) or inline-safe
/// constructions, recursing only through unambiguous shapes.
fn gen_lambda_body(b: &mut Builder, goal: &GenType) -> String {
    // Prefer an in-scope value (a lambda param or outer binding).
    if let Some(v) = gen_var(b, goal) {
        // Sometimes wrap a scalar in a small transformation to exercise
        // the closure body, otherwise return it directly.
        if b.rng.chance(1, 2) {
            return v;
        }
    }
    // Inline-safe construction for the goal type. For `Result`/`Option`
    // we use `some`/`ok` *only* when the inner type is known and inline,
    // since those constructors are valid inline when their payload fixes
    // the type. `none`/`err`/empty collections are NOT inline-safe, so
    // for those goal shapes we fall back to a scope var or a default.
    match goal {
        GenType::Int => format!("{}", b.rng.pick(pools::INT_POOL)),
        GenType::Float => render_float_literal(*b.rng.pick(pools::FLOAT_POOL)),
        GenType::String => render_string_literal(b.rng.pick(pools::STRING_POOL)),
        GenType::Bool => format!("{}", b.rng.pick(pools::BOOL_POOL)),
        GenType::Unit => "()".to_string(),
        GenType::Option(inner) => {
            // `some(x)` is inline-safe; build x inline.
            format!("some({})", gen_lambda_body(b, inner))
        }
        GenType::Result(ok) => {
            // `ok(x)` alone cannot fix E inline, so only safe to use when
            // the HOF pins E (e.g. result.map's closure returns B, and
            // the surrounding Result[B, E] context fixes E). It does, so
            // `ok(x)` here is fine.
            format!("ok({})", gen_lambda_body(b, ok))
        }
        GenType::List(elem) => {
            // Non-empty list is inline-safe; size 1 keeps it tiny.
            format!("[{}]", gen_lambda_body(b, elem))
        }
        GenType::Tuple2(a, c) => {
            format!("({}, {})", gen_lambda_body(b, a), gen_lambda_body(b, c))
        }
        GenType::Map(v) => {
            // Single-entry map literal is inline-safe.
            format!("[\"k\": {}]", gen_lambda_body(b, v))
        }
    }
}

/// Render a float literal the Almide lexer accepts (it requires a
/// decimal point; `-0.0` and scientific forms pass through).
fn render_float_literal(f: f64) -> String {
    if f == 0.0 && f.is_sign_negative() {
        return "-0.0".to_string();
    }
    if f.fract() == 0.0 && f.is_finite() && f.abs() < 1e15 {
        return format!("{f:.1}");
    }
    let s = format!("{f}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
    }
}

/// Render a string literal with Almide-compatible escapes. Backslash,
/// double-quote, common control chars, and `$` (interpolation lead-in)
/// are escaped; all other bytes — including multibyte UTF-8 — pass
/// through literally, which is what we want for the Unicode pool.
fn render_string_literal(s: impl AsRef<str>) -> String {
    let s = s.as_ref();
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '$' => out.push_str("\\$"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}
