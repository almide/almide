/// Almide type checker: AST → TypeMap (constraint-based type inference).
///
/// Input:    &mut Program (with canonicalized TypeEnv)
/// Output:   TypeMap (ExprId→Ty), diagnostics
/// Owns:     type inference (constraint collect → solve), exhaustiveness, type errors
/// Does NOT: auto-unwrap (codegen's job), code generation, optimization
///
/// Architecture:
///   Pass 1: Walk AST, assign fresh type variables to TypeMap, collect constraints (infer.rs)
///   Pass 2: Solve constraints via unification (solving.rs)
///   Pass 3: Resolve TypeVars in TypeMap values (mod.rs)
///
/// Split into:
///   mod.rs          — Checker struct, public API, declaration checking
///   types.rs        — TyVarId, Constraint, resolve_vars
///   infer.rs        — Expression/statement inference
///   calls.rs        — Function call resolution
///   registration.rs — Function/type/protocol declaration registration
///   solving.rs      — Constraint solving (unification)
///   diagnostics.rs  — Error hint helpers

mod types;
mod fallible_user_hof;
mod infer;
pub(crate) mod calls;
mod builtin_calls;
mod static_dispatch;
mod solving;
mod diagnostics;
mod deprecation_warn;
mod exhaustiveness;

use almide_lang::ast;
use almide_base::diagnostic::Diagnostic;
use crate::import_table::{ImportTable, build_import_table};
use almide_base::intern::{Sym, sym};
use crate::types::{Ty, TypeEnv};
use types::{Constraint, FixHint, UnionFind, resolve_ty};

/// Print a compiler trace line when the named debug channel is switched on.
///
/// Cross-module top-let types are written in one pass and read in another, and
/// the failure mode (a `Ty::Unknown` reaching lowering, which then emits
/// `LazyLock<_>`) is invisible in the compiler's normal output — so the write
/// and the read each announce themselves under `ALMIDE_<CHANNEL>_DEBUG`.
///
/// Traces go to stderr because stdout is the compiler's data channel: `almide
/// compile --json` and `--target rust` write their real output there, and a
/// trace line mixed into it would corrupt a machine-read result.
pub(crate) fn debug_trace(channel: &str, line: impl FnOnce() -> String) {
    if std::env::var_os(format!("ALMIDE_{channel}_DEBUG")).is_some() {
        eprintln!("[{}-debug] {}", channel.to_lowercase(), line());
    }
}

/// The mutable parts of a `fn` declaration that checking walks.
///
/// Checking mutates the AST in place (inference writes resolved types back onto
/// params, body and generics), so this is a `&mut` view rather than a copy.
/// Grouping them keeps `check_fn_decl`'s own name parameter — the only piece the
/// checker reads without walking — visible at the call site.
pub(crate) struct FnToCheck<'a> {
    pub params: &'a mut [ast::Param],
    pub return_type: &'a ast::TypeExpr,
    pub body: &'a mut ast::Expr,
    pub effect: &'a Option<bool>,
    pub generics: &'a mut Option<Vec<ast::GenericParam>>,
}

pub(crate) fn err(msg: impl Into<String>, hint: impl Into<String>, ctx: impl Into<String>) -> Diagnostic {
    Diagnostic::error(msg, hint, ctx)
}

#[derive(Clone)]
pub struct Checker {
    pub env: TypeEnv,
    pub type_map: crate::types::TypeMap,
    pub diagnostics: Vec<Diagnostic>,
    pub source_file: Option<String>,
    pub source_text: Option<String>,
    /// #567 `--profile critical`: the bounded profile (ALS §B) applied to
    /// EVERY fn of the program under inference — no `@bounded` attribute
    /// needed — with capabilities starting deny-all. Set by the check CLI
    /// around the ENTRY program's inference only (imported modules,
    /// stdlib above all, are the trusted runtime below the profile).
    pub profile_critical: bool,
    /// The GRANTED module names under `--profile critical`, already
    /// expanded from capability names (`--allow Rand` → `random`) at the
    /// CLI boundary. Empty = deny-all.
    pub critical_allow: Vec<String>,
    pub(crate) current_span: Option<crate::ast::Span>,
    /// Span of the current call's callee expression (the identifier
    /// / member reference). Set by `check_named_call_spanned` so E002
    /// can emit a `try_replace` range pointing exactly at the name
    /// token rather than the whole call. Cleared after each callee.
    pub(crate) callee_span_hint: Option<crate::ast::Span>,
    /// Span of the enclosing Call expression (covers callee + args +
    /// parentheses). Set by `infer_call` before descending into
    /// `check_call_with_type_args`, so diagnostics that need to
    /// rewrite the whole call (UFCS `x.to_uppercase()` →
    /// `string.to_upper(x)`) can target the full range.
    pub(crate) call_span_hint: Option<crate::ast::Span>,
    /// `mut` parameter indices from the last resolved function signature.
    /// Set by `check_named_call_with_type_args`, consumed by callers
    /// that have access to argument expressions for mutability validation.
    pub(crate) last_mut_params: Vec<usize>,
    /// Argument spans for the current call. Set before `check_named_call_*`
    /// so E005 can point at the exact argument expression.
    pub(crate) arg_spans: Vec<Option<crate::ast::Span>>,
    /// #558: named-arg reordering metadata for the current call —
    /// `(named_start, names)` where `named_start` is the index in the
    /// flattened positional args at which named args begin (their values were
    /// appended in SOURCE order), and `names` is the parallel param-name list.
    /// `check_named_call` uses this to validate each value against the param it
    /// NAMES (lowering binds by name), not the positional slot it landed in.
    pub(crate) named_arg_meta: Option<(usize, Vec<almide_base::intern::Sym>)>,
    /// Expected-type hint for the NEXT lambda argument's parameters (#653).
    /// Set by `check_call_with_type_args` immediately before inferring a lambda
    /// arg whose call-parameter slot is a `Fn`; consumed (taken) by the
    /// `ExprKind::Lambda` inference arm to type unannotated params from the
    /// expected element type instead of a fresh var. `None` everywhere else.
    /// Per-slot `None` = no usable expectation for that param (the substituted
    /// slot still carried the CALLEE's own unbound generic — pinning a literal
    /// sig generic like `A` would disconnect the lambda param from the
    /// union-find and it would silently default to Int later).
    pub(crate) lambda_arg_hint: Option<Vec<Option<crate::types::Ty>>>,
    /// #1055: the enclosing call slot for the lambda being inferred is an
    /// `effect (A) -> B` fn type — the body gets effect-fn ergonomics and the
    /// lambda types as the effect carrier `(A) -> Result[B, String]`.
    pub(crate) lambda_slot_effect: bool,
    pub(crate) constraints: Vec<Constraint>,
    pub(crate) uf: UnionFind,
    /// Named-type pairs currently being unified structurally. Unifying two
    /// DIFFERENT-named nominal types expands both to their record forms and
    /// recurses into the fields; a RECURSIVE type (`El = { children: List[El] }`
    /// vs a module twin `lib.El`) re-reaches the same pair and would recurse
    /// forever (stack overflow — the svg cross-module render). Equi-recursive
    /// unification: a pair already in progress unifies coinductively (true).
    pub(crate) unify_named_in_progress: std::collections::HashSet<(almide_base::intern::Sym, almide_base::intern::Sym)>,
    /// Module-name prefix active during `infer_module`. `None` for the
    /// main program. Used by the `TopLet` inference branch to write
    /// back inferred types under the prefixed `env.top_lets` key
    /// (`util.ANON`) that `register_decls` seeded — otherwise module
    /// top_lets without explicit ascription regress to `Ty::Unknown`
    /// and codegen emits `LazyLock<_>`.
    pub(crate) current_module_prefix: Option<String>,
    /// Deferred resolution targets for expressions whose types depend on
    /// a yet-unbound TypeVar's structure (e.g. `p.1` on a fresh lambda
    /// param). Each entry is `(object_ty, index, result_var)`: once
    /// `object_ty` resolves to a `Tuple`, `result_var` is unified with
    /// `elems[index]`. Drained iteratively after `solve_constraints`
    /// to give the union-find a chance to propagate before resolution.
    pub(crate) deferred_tuple_indices: Vec<(Ty, usize, Ty)>,
    /// Deferred field accesses: `(object_ty, field_name, result_var)`.
    /// Registered when `obj.field` is inferred while `obj` is an unresolved
    /// inference var. After solving, `object_ty` should be concrete and the
    /// field type can be looked up and unified with `result_var`.
    pub(crate) deferred_field_accesses: Vec<(Ty, almide_base::intern::Sym, Ty, Option<crate::ast::Span>)>,
    /// Map literal key types to validate after constraint solving.
    /// Each entry: (key_type, span) — checked via `is_hash()` once types are resolved.
    pub(crate) deferred_map_key_checks: Vec<(Ty, Option<crate::ast::Span>)>,
    /// Interpolation segments whose value will NOT be auto-?'d, awaiting the
    /// post-solve Result check (#1051): a `${resp}` holding a Result prints
    /// its debug form (`ok(…)`/`err(…)`) — legal for debug output, but a
    /// silent surprise when the writer meant the payload (the http.serve
    /// handler trap). Each entry: (segment type, span).
    pub(crate) deferred_result_interp_checks: Vec<(Ty, Option<crate::ast::Span>)>,
    /// Order-sensitive combinator subjects/keys (list.sort/min/max, sort_by's
    /// key) awaiting the post-solve ORDERABLE-element check (E030).
    pub(crate) deferred_ord_elem_checks: Vec<(Ty, Option<crate::ast::Span>, String)>,
    /// Annotation-resolved types awaiting the post-solve UNKNOWN-NAME check
    /// (E029): a `Ty::Named` whose sym is not a declared type compiles to a
    /// nonexistent Rust type (E0412/E0422/E0425) after `check` accepted — the
    /// acceptance-parity gap differential-fuzz seed 20260718 index 940 hit
    /// with a mutated-away `type` declaration. Generic params are immune by
    /// construction: resolve_type_expr turns an in-scope generic into
    /// `Ty::TypeVar` at annotation time, never `Named`.
    pub(crate) deferred_unknown_type_checks: Vec<(Ty, Option<crate::ast::Span>, String)>,
    /// Empty-collection producers whose element type must be inferable from
    /// context. Each entry is the producer's result `Ty` (carrying the fresh
    /// element type var), the construct kind (for the diagnostic's wording), and
    /// its span. Validated post-solve by [`Checker::validate_empty_collection_elements`]:
    /// if a slot is STILL an unresolved var after the whole program is solved, the
    /// element type cannot be inferred and it is a compile error (E018) — the
    /// Rust/Swift rule, never silently defaulted. See `docs/contracts` C-058.
    pub(crate) deferred_empty_collection_checks: Vec<EmptyCollectionSite>,
    /// Integer literals whose magnitude exceeds `i64::MAX`, re-checked post-solve
    /// against their CONTEXT so the range is type-aware (#626). A bare literal in
    /// a default `Int` (i64) context that overflows would otherwise SILENTLY fold
    /// to 0 on both targets (`lower` + both codegens parse with `.unwrap_or(0)`).
    /// Two valid forms are exempted at registration time, not here: a literal
    /// bound to / annotated as a wider type (`let u: UInt64 = …`) and the negated
    /// `i64::MIN` magnitude (`-9223372036854775808`).
    pub(crate) deferred_int_overflow_checks: Vec<IntOverflowSite>,
    pub(crate) deferred_float_overflow_checks: Vec<FloatOverflowSite>,
    /// Un-annotated value bindings / discarded expression statements whose
    /// inferred type must be fully decidable. Each entry carries the binding's
    /// value `Ty` (with inference vars intact), an optional binding name (for the
    /// diagnostic's wording / fix), and the span. Validated post-solve by
    /// [`Checker::validate_unresolved_binding_types`]: if the resolved type still
    /// holds an unbound `?`-prefixed inference var ANYWHERE after the whole
    /// program is solved, that slot was never pinned by context (e.g. the error
    /// type of `result.or_else(r0, (e) => ok(0))`, only reachable through the
    /// un-exercised `err` branch). That is a compile error (E025) — the same
    /// Rust/Swift rule E018 enforces for empty collections, never silently
    /// defaulted. Without it the value passed `check` and then tripped the
    /// ConcretizeTypes COMPILER-BUG gate on BOTH targets (#662).
    pub(crate) deferred_unresolved_binding_checks: Vec<UnresolvedBindingSite>,
    /// #1123 / ADR-0008 N+1: sites where the pre-switch implementation
    /// inserted implicit propagation (auto-`?`). Post-solve, every site whose
    /// type resolved to Result is a hard error — E042 (must-use: a discarded
    /// statement-position Result, the 5th field true) or E041 (implicit
    /// propagation at every other position class) — with the mechanical `!`
    /// insertion where the span is a plain call.
    /// (ty, span, position label, mechanical, must_use)
    pub(crate) deferred_implicit_prop_checks: Vec<(Ty, Option<ast::Span>, &'static str, bool, bool)>,
    /// ADR-0006 D1 (#1108 Phase 2a): fns DECLARED with the `-> T!` marker.
    /// Resolution erases the marker into Result[T, String], so the 1-bit
    /// fallibility of a NAMED callback argument (`list.map(xs, parse)`) is
    /// recorded here at declaration time.
    pub(crate) fallible_marker_fns: std::collections::HashSet<Sym>,
    /// Call sites the fallible-HOF normalization REWROTE (`list.map` →
    /// `list.try_map`, keyed by the module Ident's ExprId): the try_
    /// deprecation warning (E043) must fire only on USER-SPELLED try_*.
    pub(crate) hof_rewritten_calls: std::collections::HashSet<almide_lang::ast::ExprId>,
    /// Annotated `let`/`var` bindings, re-checked post-solve for the numeric
    /// narrowing direction (#867). The solver joins numeric widths
    /// symmetrically — peer sites like list elements and `assert_eq` args
    /// must not depend on visit order — so the one-way rule (a sized value
    /// does not flow into an `Int`/`Float` slot; write `.to_int64()`) is
    /// enforced at the sites where expected/actual is real: call arguments
    /// (`types_mismatch`) and these annotation sites.
    pub(crate) deferred_numeric_narrowing_checks: Vec<NumericNarrowingSite>,
    /// Top-let `env.top_lets` writes awaiting the post-solve upgrade. The
    /// `TopLet` branch resolves its initializer type BEFORE `solve_constraints`
    /// runs, so a generic-ctor initializer (`let MAYBE = some(Cfg {…})`) stores
    /// `Option[Unknown]` — its payload constraint is still unsolved — and every
    /// cross-module reader then sees an Unknown payload (the
    /// option_record_toplet wasm wall). Each entry is `(top_lets key, ty with
    /// inference vars intact)`; [`Checker::flush_pending_toplet_tys`] re-resolves
    /// after solving and upgrades entries that are still partially Unknown.
    /// Drained by each inference flow AFTER its own solve and BEFORE its
    /// union-find is swapped back (a pending var resolved against a different
    /// UF would produce garbage).
    pub(crate) pending_toplet_tys: Vec<(almide_base::intern::Sym, Ty)>,
}

/// An integer literal that does not fit `i64`, pending a post-solve range check.
#[derive(Debug, Clone)]
pub(crate) struct IntOverflowSite {
    /// The literal's `ExprId` — used to drop the site if a wider annotation
    /// later exempts it (the value of `let u: UInt64 = …`).
    pub expr_id: crate::ast::ExprId,
    /// Raw lexed text (underscores / radix prefix intact).
    pub raw: String,
    /// True when the literal is the operand of a unary minus, so its negation
    /// (down to `i64::MIN`) is the value that must fit — `2^63` is then valid.
    pub negated: bool,
    /// The declared type the literal is bound/annotated to, when it is the direct
    /// value of `let x: T = …` / `var x: T = …`. `None` ⇒ a default `Int` (i64)
    /// context. A wider `T` (e.g. `UInt64`) makes a >i64 literal valid.
    pub context_ty: Option<Ty>,
    pub span: Option<crate::ast::Span>,
}

/// A float literal whose magnitude exceeds f32's finite range, pending the
/// post-solve check: an error ONLY if its effective type resolves to Float32
/// (rustc rejects the emitted `<lit>f32` — the float sibling of the E024
/// integer domain; Wave 4 L7). Unlike ints, no context threading is needed:
/// a float literal's Float32-ness IS its solved type (the C-182 context typing).
#[derive(Debug, Clone)]
pub(crate) struct FloatOverflowSite {
    pub expr_id: crate::ast::ExprId,
    pub value: f64,
    /// The declared type when the literal is the direct value of an annotated
    /// binding/field — a bare literal's own solved type stays `Float`, so the
    /// Float32 context lives on the binding and must be pinned here (the same
    /// reason `IntOverflowSite` carries it).
    pub context_ty: Option<Ty>,
    pub span: Option<crate::ast::Span>,
}

/// An annotated `let`/`var` binding, pending the post-solve numeric-narrowing
/// direction check (#867). See `deferred_numeric_narrowing_checks`.
#[derive(Debug, Clone)]
pub(crate) struct NumericNarrowingSite {
    /// The declared annotation type.
    pub expected: Ty,
    /// The initializer's inferred type, inference vars intact.
    pub actual: Ty,
    /// The binding, for the diagnostic ("let 'x'").
    pub context: String,
    pub span: Option<crate::ast::Span>,
}

// (signed, bit-width) for each sized integer type; None for non-integer types
// (not our diagnostic).
fn int_type_signed_bits(ty: &Ty) -> Option<(bool, u32)> {
    match ty {
        Ty::Int | Ty::Int64 => Some((true, 64)),
        Ty::Int8 => Some((true, 8)), Ty::Int16 => Some((true, 16)), Ty::Int32 => Some((true, 32)),
        Ty::UInt8 => Some((false, 8)), Ty::UInt16 => Some((false, 16)),
        Ty::UInt32 => Some((false, 32)), Ty::UInt64 => Some((false, 64)),
        _ => None,
    }
}

/// The signed type of the same width, for an unsigned integer type. Used to
/// name the concrete alternative in the "unsigned has no negative values"
/// diagnostic, so the hint points at a type instead of at a concept.
pub(crate) fn signed_counterpart(ty: &Ty) -> Option<&'static str> {
    match ty {
        Ty::UInt8 => Some("Int8"), Ty::UInt16 => Some("Int16"),
        Ty::UInt32 => Some("Int32"), Ty::UInt64 => Some("Int64"),
        _ => None,
    }
}

/// Why an int literal does not fit its context. Three deviations exist and they
/// take three different fixes, so the E024 hint has to know WHICH one it is —
/// telling a reader to shrink a literal that no magnitude could rescue, or to
/// widen to a type that does not exist, sends them somewhere there is nothing.
///
/// Deciding it once, here, is deliberate. This file used to answer "does it
/// fit" and the diagnostic used to answer "why not" separately, which is the
/// same hand-synced-copies shape `literals.rs` was extracted to end: two
/// derivations of one fact drift, and the drift shows up as a diagnostic that
/// contradicts the check that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiteralFit {
    /// Representable in this context.
    Fits,
    /// Too large for the type's own domain — would silently fold to 0.
    Magnitude,
    /// Negated in an UNSIGNED context. Not a magnitude question: an unsigned
    /// type has no negative values at ANY magnitude. `-0` is not this — it is
    /// `0`, which every unsigned type represents.
    Sign,
}

/// The int literal `value` ultimately denotes, seen through any chain of parens
/// and unary minus: its `ExprId`, its raw text, and the NET sign the source
/// applies to it. `None` when `value` is not such a chain.
///
/// The chain has to be walked to the END, and both facts this returns are why.
///
/// REACH: one level is not enough, because a literal is routinely written with
/// more than one node above it. `-(300)` parks a `Paren` between the minus and
/// the digits; `--300` parks a second `Unary`. Stopping at the first node left
/// those forms with no range context at all, so `let m: Int8 = -(300)` passed
/// `check` and rustc then rejected the emitted `-300i8` — the check-vs-build
/// gap E024 exists to close (differential fuzz, seed 1785217538023450905).
///
/// PARITY: the count matters as much as the reach, and in the opposite
/// direction. `-9223372036854775808` is `i64::MIN` and valid; `--9223372036854775808`
/// is +2^63, which NO signed type represents. A walk that recorded "there was a
/// minus somewhere" rather than how many would call the second one valid and let
/// it fold silently — the failure the negated bound was introduced to prevent.
///
/// Derived once, here, because two callers need it: the `Unary` inference marks
/// the site's sign, and the annotated-binding hook pins the site's declared type.
/// They used to peel separately and agree only on the single-minus case.
pub(crate) fn int_literal_chain(
    value: &crate::ast::Expr,
) -> Option<(crate::ast::ExprId, String, bool)> {
    use crate::ast::ExprKind;
    let mut cur = value;
    let mut negated = false;
    loop {
        match &cur.kind {
            ExprKind::Int { raw, .. } => return Some((cur.id, raw.clone(), negated)),
            ExprKind::Paren { expr } => cur = expr,
            ExprKind::Unary { op, operand, .. } if op.as_str() == "-" => {
                negated = !negated;
                cur = operand;
            }
            _ => return None,
        }
    }
}

/// The float sibling of [`int_literal_chain`]: reach a FLOAT literal through any
/// paren/unary-minus chain. Magnitude is sign-symmetric for the f32 range check,
/// so no negation flag is carried.
pub(crate) fn float_literal_chain(value: &crate::ast::Expr) -> Option<(crate::ast::ExprId, f64)> {
    use crate::ast::ExprKind;
    let mut cur = value;
    loop {
        match &cur.kind {
            ExprKind::Float { value: v, .. } => return Some((cur.id, *v)),
            ExprKind::Paren { expr } => cur = expr,
            ExprKind::Unary { op, operand, .. } if op.as_str() == "-" => cur = operand,
            _ => return None,
        }
    }
}

/// Classify `raw` against the range `ty` can represent in this position.
///
/// For a SIGNED type the magnitude bound is `MAX` (or `MAX+1` when `negated`,
/// reaching `MIN`). For an unsigned type it is the unsigned `MAX` — the FULL
/// declared domain: the i64 slot is a 64-bit PATTERN and the `UInt64` upper
/// half is carried in it, with `IntOp::DivU`/`ModU`/`LtU`… reading it
/// unsigned on both targets (#872). (The interim carrier cap that rejected
/// that band is gone with the lane that replaced it.)
///
/// A non-integer type `Fits` — the literal does not belong there, and saying so
/// is the normal type checker's job, not this diagnostic's.
pub(crate) fn classify_int_literal(raw: &str, ty: &Ty, negated: bool) -> LiteralFit {
    let Some((signed, bits)) = int_type_signed_bits(ty) else { return LiteralFit::Fits };
    let clean = raw.replace('_', "");
    let (radix, digits) = crate::literals::radix_and_digits(&clean);
    let Ok(mag) = u128::from_str_radix(digits, radix) else {
        // The digits do not fit even the `u128` this comparison is done in, so
        // no integer width could hold them. This used to return "fits", and
        // `int_value` then folded the literal to 0 — a 44-digit literal printed
        // `0` with no diagnostic, past the end of the very check that exists to
        // stop that. The magnitude is the failure whatever the sign is, unless
        // the sign is independently wrong.
        return if negated && !signed { LiteralFit::Sign } else { LiteralFit::Magnitude };
    };
    let declared: u128 = if signed {
        if negated { 1u128 << (bits - 1) } else { (1u128 << (bits - 1)) - 1 }
    } else {
        (1u128 << bits) - 1
    };
    match () {
        _ if !signed && negated && mag != 0 => LiteralFit::Sign,
        _ if mag > declared => LiteralFit::Magnitude,
        _ => LiteralFit::Fits,
    }
}

/// The inclusive range `ty` accepts, as written in a diagnostic — `0...255`
/// (the Swift-style inclusive spelling, #966 — a diagnostic quoting the
/// retired `..=` would teach the very syntax E031 rejects).
///
/// Rust states this range in a note on its out-of-range literal error, and it
/// is the difference between a hint a reader can act on and one they have to go
/// look something up for. Few readers, human or model, hold `u32::MAX` in mind.
/// `None` for a non-integer type, which has no range to state.
pub(crate) fn int_type_range(ty: &Ty) -> Option<String> {
    let (signed, bits) = int_type_signed_bits(ty)?;
    Some(if signed {
        format!("-{}...{}", 1u128 << (bits - 1), (1u128 << (bits - 1)) - 1)
    } else {
        format!("0...{}", (1u128 << bits) - 1)
    })
}

/// The construct that produced an empty collection whose element type the
/// checker must be able to infer from context. Carried by an
/// [`EmptyCollectionSite`] so the E018 diagnostic can name the exact form and
/// show a matching annotation example.
#[derive(Debug, Clone, Copy)]
pub(crate) enum EmptyCollectionKind {
    /// An empty list literal `[]`.
    ListLiteral,
    /// An empty map literal `[:]` / `{}` (or the desugared `EmptyMap`).
    MapLiteral,
    /// `set.new()` — a generic `Set[A]` constructor with no element argument.
    SetNew,
    /// `list.with_capacity(n)` — a generic `List[A]` constructor whose only
    /// argument is the capacity, not an element.
    ListWithCapacity,
    /// The iterable of a `for _ in []` loop (an empty list literal in iterable
    /// position). Distinguished so the hint can suggest annotating the iterable.
    ForInEmpty,
}

/// One un-annotated binding / discarded expression to re-check after constraint
/// solving for an undecidable (never-pinned) inference var (#662).
#[derive(Debug, Clone)]
pub(crate) struct UnresolvedBindingSite {
    /// The binding's / expression's value type, with inference vars intact.
    /// Resolved against the union-find post-solve; an unbound `?N` survivor means
    /// the type was never pinned by context.
    pub ty: Ty,
    /// The binding name (`let`/`var`), or `None` for a discarded expression
    /// statement. Drives the diagnostic's primary fix (annotate the binding).
    pub name: Option<String>,
    /// Source span of the offending expression.
    pub span: Option<crate::ast::Span>,
}

/// One empty-collection producer to re-check after constraint solving.
#[derive(Debug, Clone)]
pub(crate) struct EmptyCollectionSite {
    /// The producer's result type, e.g. `List[?A]` / `Set[?A]` / `Map[?K, ?V]`.
    /// Resolved against the union-find post-solve; if any element/key/value slot
    /// is still an unresolved var, the element type was never pinned by context.
    pub ty: Ty,
    /// Which construct produced it (drives the diagnostic wording + example).
    pub kind: EmptyCollectionKind,
    /// Source span of the offending expression.
    pub span: Option<crate::ast::Span>,
}

impl Checker {
    /// Create a Checker from a pre-populated TypeEnv (from canonicalize_program).
    pub fn from_env(env: TypeEnv) -> Self {
        Checker {
            env, type_map: crate::types::TypeMap::new(),
            diagnostics: Vec::new(),
            source_file: None, source_text: None,
            profile_critical: false,
            critical_allow: Vec::new(),
            current_span: None,
            callee_span_hint: None,
            call_span_hint: None,
            last_mut_params: Vec::new(),
            arg_spans: Vec::new(),
            named_arg_meta: None,
            lambda_arg_hint: None,
            lambda_slot_effect: false,
            constraints: Vec::new(), uf: UnionFind::new(),
            unify_named_in_progress: std::collections::HashSet::new(),
            current_module_prefix: None,
            deferred_tuple_indices: Vec::new(),
            deferred_field_accesses: Vec::new(),
            deferred_map_key_checks: Vec::new(),
            deferred_result_interp_checks: Vec::new(),
            deferred_ord_elem_checks: Vec::new(),
            deferred_empty_collection_checks: Vec::new(),
            deferred_int_overflow_checks: Vec::new(),
            deferred_float_overflow_checks: Vec::new(),
            deferred_numeric_narrowing_checks: Vec::new(),
            deferred_unresolved_binding_checks: Vec::new(),
            deferred_implicit_prop_checks: Vec::new(),
            fallible_marker_fns: std::collections::HashSet::new(),
            hof_rewritten_calls: std::collections::HashSet::new(),
            deferred_unknown_type_checks: Vec::new(),
            pending_toplet_tys: Vec::new(),
        }
    }

    /// Extract the source substring covered by a single-line span. Returns
    /// `None` when `source_text` is unset (IDE / playground contexts) or
    /// the span is out-of-bounds. Used by Phase 3 diagnostics that need
    /// to interpolate existing source (e.g. E002 method-UFCS rewrites
    /// `x.to_uppercase()` to `string.to_upper(x)` — `x` comes from the
    /// object's span).
    pub(crate) fn source_slice(&self, span: crate::ast::Span) -> Option<String> {
        let text = self.source_text.as_deref()?;
        let mut line_start = 0usize;
        let mut cur_line = 1usize;
        for (i, b) in text.bytes().enumerate() {
            if cur_line == span.line { break; }
            if b == b'\n' {
                cur_line += 1;
                line_start = i + 1;
            }
        }
        if cur_line != span.line { return None; }
        let line_end = text[line_start..].find('\n').map(|i| line_start + i).unwrap_or(text.len());
        let line_slice = &text[line_start..line_end];
        let col_to_byte = |target: usize| -> Option<usize> {
            match line_slice.char_indices().nth(target - 1) {
                Some((b, _)) => Some(b),
                None => {
                    let n = line_slice.chars().count();
                    if target == n + 1 { Some(line_slice.len()) } else { None }
                }
            }
        };
        let start = col_to_byte(span.col)?;
        let end_col = if span.end_col > span.col { span.end_col } else { span.col + 1 };
        let end = col_to_byte(end_col)?;
        if end < start || end > line_slice.len() { return None; }
        Some(line_slice[start..end].to_string())
    }

    /// Push a diagnostic, automatically attaching the current expression's span.
    pub(crate) fn emit(&mut self, mut diag: Diagnostic) {
        if diag.line.is_none() {
            if let Some(span) = &self.current_span {
                if let Some(file) = &self.source_file {
                    diag.file = Some(file.clone());
                }
                diag.line = Some(span.line);
                diag.col = Some(span.col);
                if span.end_col > span.col {
                    diag.end_col = Some(span.end_col);
                }
            }
        }
        self.diagnostics.push(diag);
    }

    pub(crate) fn fresh_var(&mut self) -> Ty {
        let id = self.uf.fresh();
        Ty::TypeVar(sym(&format!("?{}", id)))
    }

    pub(crate) fn constrain(&mut self, expected: Ty, actual: Ty, context: impl Into<String>) {
        self.constrain_with_hint(expected, actual, context, None);
    }

    /// Reject arithmetic that mixes a SIZED numeric type with a canonical
    /// `Int` / `Float` VALUE, and return the sized type so inference carries on.
    ///
    /// The mixed-width rule (`Int32 + Int16`) was enforced on the sized types
    /// only, which let the same mistake through whenever the wide side was
    /// spelled `Int`: `fn add32(a: Int32, b: Int) -> Int32 = a + b` passed
    /// `check`, failed the native build with a rustc E0308, and on wasm — where
    /// every scalar rides one i64 — computed a value outside the declared width
    /// (#902). Spelling the same parameter `Int64` WAS caught, so the rule was
    /// really enforcing a spelling rather than a type.
    ///
    /// The canonical side is exempt only when it is a LITERAL-ONLY expression,
    /// which is the case the permissive pair exists for: `let x: Int32 = 1;
    /// x + 2` must keep working, because the literal adopts the sized width at
    /// lowering. A literal-only tree is the same shape lowering retypes whole —
    /// literals, negation, and arithmetic over them, nothing that could have
    /// chosen a width of its own.
    pub(crate) fn check_mixed_canonical_width(
        &mut self,
        op: &str,
        lc: &Ty,
        rc: &Ty,
        left: &ast::Expr,
        right: &ast::Expr,
    ) -> Option<Ty> {
        let canonical_peer = |sized: &Ty, canon: &Ty| match (sized, canon) {
            (s, Ty::Int) if is_sized_int(s) => true,
            (s, Ty::Float) if matches!(s, Ty::Float32 | Ty::Float64) => true,
            _ => false,
        };
        let (sized, canon_ty, canon_expr) = if canonical_peer(lc, rc) {
            (lc, rc, right)
        } else if canonical_peer(rc, lc) {
            (rc, lc, left)
        } else {
            return None;
        };
        // A LITERAL canonical side is the case the permissive pair exists for: it
        // adopts the sized width at lowering, so it is not an error. But the
        // RESULT is still the SIZED type — returning `None` here let the caller
        // fall through to its `lt.clone()` default and type `0 - int8v` as
        // canonical `Int`, while lowering had already stamped the whole
        // literal-only tree Int8. The checker then said `i64` over an `i8`
        // expression and the generated Rust would not build (the
        // `self_hosted_float_convert` shape: `let nm = 0 - float.to_int8(f)`).
        // Same rule as the peer join below — the sized member wins.
        if is_literal_numeric_ast(canon_expr) {
            return Some(sized.clone());
        }
        self.emit(err(
            format!(
                "operator '{}' mixes sized numeric type {} with a canonical {} value — \
                 explicit conversion required (e.g. `.to_{}()`)",
                op,
                sized.display(),
                canon_ty.display(),
                sized.display().to_lowercase()
            ),
            "Convert one side with `.to_intN()` / `.to_floatN()` before the op. A literal \
             adopts the sized width automatically; a VALUE does not.",
            format!("operator {}", op),
        ));
        Some(sized.clone())
    }

    /// The PEER-JOIN half of the same width rule (#880).
    ///
    /// `check_mixed_canonical_width` above governs the operator sites, where the
    /// two operands really are symmetric. A peer join — list elements, `if` /
    /// `match` arms — is not: it took the FIRST peer's type as the join, so
    /// `[1, u8v]` typed the whole literal `List[Int]` while `[u8v, 1]` typed it
    /// `List[UInt8]`. The same program, elements swapped, two different types;
    /// the `Int` reading then emitted `vec![1i64, 3u8]`, which rustc rejects.
    ///
    /// The SIZED peer is the only one that ever chose a width, so it wins the
    /// join and the canonical peers coerce into it — but a canonical peer may
    /// coerce only when it is a LITERAL (the same exemption as the operator
    /// rule: lowering retypes a literal-only tree whole, and nothing else).
    /// A canonical VALUE keeps its own i64/f64 width and is an error.
    ///
    /// `peers` is `(type, span, is_literal_only)` per member, already inferred —
    /// the caller owns the AST walk, which keeps this free of the borrow that
    /// holding `&ast::Expr` across `&mut self` would need. Returns the joined
    /// (sized) type, or `None` when the peers are not a canonical/sized mix.
    pub(crate) fn join_sized_peers(
        &mut self,
        peers: &[(Ty, Option<ast::Span>, bool)],
        context: &str,
    ) -> Option<Ty> {
        let resolved: Vec<Ty> = peers.iter().map(|(t, _, _)| resolve_ty(t, &self.uf)).collect();
        let sized = resolved.iter().find(|t| is_narrow_sized(t))?.clone();
        let saved = self.current_span;
        for (i, t) in resolved.iter().enumerate() {
            if !is_canonical_peer_of(&sized, t) || peers[i].2 {
                continue;
            }
            self.current_span = peers[i].1.or(saved);
            self.emit(err(
                format!(
                    "{} mixes sized numeric type {} with a canonical {} value — \
                     explicit conversion required (e.g. `.to_{}()`)",
                    context,
                    sized.display(),
                    t.display(),
                    sized.display().to_lowercase()
                ),
                "Convert the value with `.to_intN()` / `.to_floatN()` so every peer has \
                 the same width. A literal adopts the sized width automatically; a VALUE \
                 does not.",
                context.to_string(),
            ).with_code("E001"));
        }
        self.current_span = saved;
        Some(sized)
    }

    /// `if` / `while` take a `Bool` condition, full stop — Almide has no
    /// truthiness, and until this constraint existed the checker accepted every
    /// condition type (#896). `if 1 then …` ran with C-style truthiness and
    /// `if "s" then …` passed `check` and then died in codegen as an ICE; both
    /// are now the ordinary E001 the language always claimed they were.
    ///
    /// In an effect fn a condition may still be wearing its `Result[Bool, E]`
    /// wrapper (the same auto-unwrap asymmetry the if-branch comparison below
    /// handles), so compare at the unwrapped level when `auto_unwrap` is on.
    pub(crate) fn constrain_condition(&mut self, cond: &ast::Expr, cond_ty: Ty, keyword: &str) {
        let actual = if self.env.auto_unwrap {
            match resolve_ty(&cond_ty, &self.uf) {
                Ty::Applied(crate::types::TypeConstructorId::Result, ref args) if args.len() == 2 => {
                    // #1123: the condition's Result is stripped implicitly.
                    let mech = matches!(cond.kind, ast::ExprKind::Call { .. });
                    self.deferred_implicit_prop_checks.push((
                        cond_ty.clone(), cond.span, "of this condition", mech, false,
                    ));
                    args[0].clone()
                }
                _ => cond_ty,
            }
        } else {
            cond_ty
        };
        let saved = self.current_span;
        self.current_span = cond.span.or(saved);
        self.constrain(Ty::Bool, actual, format!("{keyword} condition"));
        self.current_span = saved;
    }

    pub(crate) fn constrain_with_hint(
        &mut self,
        expected: Ty,
        actual: Ty,
        context: impl Into<String>,
        fix_hint: Option<FixHint>,
    ) {
        let ctx = context.into();
        self.unify_infer(&expected, &actual);
        self.constraints.push(Constraint {
            expected, actual, context: ctx,
            span: self.current_span,
            fix_hint,
        });
    }

    pub fn set_source(&mut self, file: &str, text: &str) { self.source_file = Some(file.into()); self.source_text = Some(text.into()); }

    /// Drain pending TupleIndex deferrals to a fixed point. A deferral
    /// is registered when `obj.N` is inferred while `obj` is a fresh
    /// inference var — there's no Tuple to index into yet, so the
    /// result is bound to a fresh var and the resolution is parked.
    /// Once the union-find binds `obj_ty` to a concrete `Tuple`, we
    /// unify the parked result with the indexed element. We loop
    /// because a successful unify may unblock another deferral whose
    /// `obj_ty` was itself the parked result of an earlier one.
    pub(crate) fn resolve_deferred_tuple_indices(&mut self) {
        self.drain_deferred_tuple_indices();
        self.drain_deferred_field_accesses();
    }

    // Fixpoint-drain `self.deferred_tuple_indices`: retries each pending
    // `(obj_ty, index, result_ty)` until either the queue is empty or a full
    // pass makes no progress.
    fn drain_deferred_tuple_indices(&mut self) {
        loop {
            let pending = std::mem::take(&mut self.deferred_tuple_indices);
            if pending.is_empty() { break; }
            let mut still_pending = Vec::new();
            let mut progressed = false;
            for (obj_ty, index, result_ty) in pending {
                let resolved = resolve_ty(&obj_ty, &self.uf);
                match &resolved {
                    Ty::Tuple(elems) if index < elems.len() => {
                        self.unify_infer(&result_ty, &elems[index]);
                        progressed = true;
                    }
                    _ => still_pending.push((obj_ty, index, result_ty)),
                }
            }
            self.deferred_tuple_indices = still_pending;
            if !progressed { break; }
        }
    }

    // Drain deferred field accesses: `obj.field` where `obj` was an
    // unresolved inference var at inference time. Now that constraints
    // are solved, resolve the field type and unify.
    fn drain_deferred_field_accesses(&mut self) {
        loop {
            let pending = std::mem::take(&mut self.deferred_field_accesses);
            if pending.is_empty() { break; }
            let mut still_pending = Vec::new();
            let mut progressed = false;
            for (obj_ty, field, result_ty, span) in pending {
                let resolved = resolve_ty(&obj_ty, &self.uf);
                let field_ty = self.resolve_field_type(&resolved, field.as_str());
                if !matches!(field_ty, Ty::Unknown) {
                    self.unify_infer(&result_ty, &field_ty);
                    progressed = true;
                } else {
                    still_pending.push((obj_ty, field, result_ty, span));
                }
            }
            self.deferred_field_accesses = still_pending;
            if !progressed { break; }
        }
        // #847: leftovers whose object DID resolve to a closed record are
        // definitively missing fields — silently dropping them let the
        // Unknown flow to codegen (postcondition ICE / leaked rustc E0609).
        for (obj_ty, field, _result_ty, span) in std::mem::take(&mut self.deferred_field_accesses) {
            let resolved = resolve_ty(&obj_ty, &self.uf);
            let shape = self.env.resolve_named(&resolved);
            if let Ty::Record { fields } = &shape {
                let available = fields.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
                let suggestion = almide_base::diagnostic::suggest(
                    field.as_str(), fields.iter().map(|(n, _)| n.as_str()));
                let hint = match &suggestion {
                    Some(close) => format!("Did you mean `{}`? Available fields: {}", close, available),
                    None => format!("Available fields: {}", available),
                };
                let mut diag = err(
                    format!("no field '{}' on {}", field, resolved.display()),
                    hint,
                    format!("field access .{}", field),
                ).with_code("E013");
                if let Some(s) = span {
                    diag.file = self.source_file.clone();
                    diag.line = Some(s.line);
                    diag.col = Some(s.col);
                }
                self.emit(diag);
            }
        }
    }

    // ── Main entry point ──

    /// Type-check a program whose environment was pre-populated by `canonicalize_program`.
    /// Skips import table building and declaration registration — inference only.
    pub fn infer_program(&mut self, program: &mut ast::Program) -> Vec<Diagnostic> {
        // #1311 front-end phase accounting (no-op unless `--timings`).
        let _phase = almide_base::profile::phase_scope(almide_base::profile::Phase::Check);
        // E012 for DUPLICATE top-level lets: registration is idempotent by
        // design (it re-runs per driver leg), so the seed insert cannot
        // detect a second declaration — the last one silently won and the
        // native build died on rustc's duplicate definition (diagnostic
        // sweep 2026-08-18). One scan over the source decls, here, where a
        // program is seen exactly once.
        {
            let mut seen: std::collections::HashMap<almide_base::intern::Sym, Option<ast::Span>> =
                std::collections::HashMap::new();
            for decl in &program.decls {
                if let ast::Decl::TopLet { name, span, .. } = decl {
                    if let Some(first) = seen.get(name) {
                        let mut d = err(
                            format!("duplicate top-level binding '{}'", name),
                            format!("'{}' is already declared at module scope — rename one, or merge the two initializers", name),
                            format!("let {}", name),
                        ).with_code("E012");
                        if let Some(sp) = span {
                            d.line = Some(sp.line);
                            d.col = Some(sp.col);
                        }
                        if let Some(fsp) = first {
                            d = d.with_secondary(fsp.line, Some(fsp.col), format!("'{}' first declared here", name));
                        }
                        self.diagnostics.push(d);
                    } else {
                        seen.insert(*name, *span);
                    }
                }
            }
        }
        // A top-level `let` bound to a LAMBDA is rejected with the fn spelling
        // as the fix (#1540): codegen placed the closure in a non-Sync
        // `Rc<dyn Fn>` static (rustc E0277) AND the binding was uncallable
        // (call position resolved E002) — accepted-but-unusable in every
        // spelling, so the honest answer is a check-time diagnostic. Inside a
        // fn/test body both uses work and stay untouched.
        for decl in &program.decls {
            if let ast::Decl::TopLet { name, value, span, .. } = decl {
                if matches!(&value.kind, ast::ExprKind::Lambda { .. }) {
                    let mut d = err(
                        format!("top-level 'let {}' cannot be bound to a function value", name),
                        format!(
                            "a module-level closure has no home in the compiled output and the                              binding is not callable — declare it as a function instead:                              `fn {}(…) = …` (a `let`-bound lambda works inside a fn or test body)",
                            name
                        ),
                        format!("let {} = (…) => …", name),
                    ).with_code("E061");
                    if let Some(sp) = span {
                        d.line = Some(sp.line);
                        d.col = Some(sp.col);
                    }
                    self.diagnostics.push(d);
                }
            }
        }
        // ADR-0006 D1 (#1108): record every fn DECLARED `-> T!` before
        // resolution erases the marker, so a named callback argument's
        // fallibility bit is known at HOF call sites.
        for decl in &program.decls {
            if let ast::Decl::Fn { name, return_type, .. } = decl {
                if matches!(return_type, ast::TypeExpr::Generic { name: g, .. } if g.as_str() == "!") {
                    self.fallible_marker_fns.insert(*name);
                }
            }
        }
        // #1108 Phase 2b-iii (D2, Cell 1): a fallible callback in a USER HOF's
        // bare `(A) -> B` slot routes the call to a GENERATED `__fallible__`
        // twin — same pre-inference name-swap discipline as the list HOFs'
        // hand-written twins, so everything downstream is the proven D3
        // explicit-slot path. Runs before any inference so the twins register
        // like ordinary decls.
        let n_twins =
            fallible_user_hof::normalize_fallible_user_hofs(program, &self.fallible_marker_fns);
        // The twins were appended AFTER `register_decls` ran (registration is
        // part of canonicalize, upstream of this checker entry) — register
        // their signatures through the same path so call resolution sees them
        // like any parsed decl.
        if n_twins > 0 {
            let start = program.decls.len() - n_twins;
            for decl in &program.decls[start..] {
                let ast::Decl::Fn {
                    name, effect, visibility, generics, params, return_type, span, ..
                } = decl
                else {
                    continue;
                };
                crate::canonicalize::registration::register_fn_sig(
                    &mut self.env,
                    &crate::canonicalize::registration::FnSigToRegister {
                        name: name.as_str(),
                        params,
                        return_type,
                        effect,
                        generics,
                        prefix: None,
                        span: span.as_ref(),
                        visibility: *visibility,
                        attrs: &[],
                    },
                );
            }
        }
        // `main` takes NO parameters (#789): the parameter form typechecked but no
        // codegen leg wires the argument — native emitted an uncallable driver
        // ("codegen produced invalid Rust — this is an Almide bug") and the v1 wasm
        // `_start` glue a structurally invalid module. Reject it HERE with the
        // documented convention (`env.args()`) instead of blaming the compiler
        // downstream.
        for decl in &program.decls {
            let ast::Decl::Fn { name, params, span, .. } = decl else { continue };
            if name.as_str() != "main" || params.is_empty() {
                continue;
            }
            let mut diag = err(
                "main() takes no parameters",
                "program arguments are read with `env.args()` inside the body \
                 (add `import env`): `effect fn main() { let args = env.args() ... }`",
                "fn main",
            )
            .with_code("E028");
            if let Some(s) = span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            self.diagnostics.push(diag);
        }
        // A PURE `main` returns Unit (#912 diagnostic-divergence lens, round 1):
        // the C/Go-style `fn main() -> Int` typechecked but the entry is emitted
        // verbatim — native produced `pub fn main() -> i64` (rustc E0277,
        // surfaced as "codegen produced invalid Rust"). An EFFECT main may
        // declare any Ok type: its wrapper unwraps the carrier and discards the
        // payload, which every leg supports (e008-fan-captures-mut pins it).
        // Same #789 discipline as the parameter rule above: reject at the seam
        // with the documented convention instead of blaming the compiler
        // downstream.
        for decl in &program.decls {
            let ast::Decl::Fn { name, effect, return_type, span, .. } = decl else { continue };
            if name.as_str() != "main" || effect.unwrap_or(false) {
                continue;
            }
            if matches!(return_type, ast::TypeExpr::Simple { name: t } if t.as_str() == "Unit") {
                continue;
            }
            let mut diag = err(
                "main() returns Unit",
                "a program's result is its output, not a return value — print it, \
                 or set the exit code with `process.exit(n)` (import process). \
                 Declare the entry `fn main() -> Unit` (or `effect fn main() -> Unit`)",
                "fn main",
            )
            .with_code("E044");
            if let Some(s) = span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            self.diagnostics.push(diag);
        }
        // #785 for the ENTRY program itself: a generic-ctor top-let (`let
        // MAYBE = some(Cfg {…})`) seeds `Option[Unknown]`, and a same-file
        // reader consumes that seed DURING constraint collection — before the
        // post-solve flush below can upgrade it. Pre-solve the entry's
        // top-lets in the same isolated bracket the module refresh uses; the
        // real pass right after re-checks them and owns all reporting.
        self.refresh_module_top_lets(program, "__entry");
        for decl in program.decls.iter_mut() { self.check_decl(decl); }
        self.solve_constraints();
        self.resolve_deferred_tuple_indices();
        self.flush_pending_toplet_tys();
        resolve_type_map(&mut self.type_map, &self.uf);
        self.validate_map_key_types();
        self.validate_result_interpolations();
        self.validate_ord_elem_types();
        self.validate_unknown_named_types();
        self.validate_empty_collection_elements();
        self.validate_int_overflow_literals();
        self.validate_float_overflow_literals();
        self.validate_numeric_narrowing();
        self.validate_unresolved_binding_types();
        self.validate_implicit_propagation();
        self.lint_error_surface(program);
        self.check_bounded_profile(program);
        // Unused import warnings. Usage is judged SYNTACTICALLY first
        // (#1783): every `alias.x` spelling in the file — call targets,
        // record/variant constructors, type annotations, patterns — marks
        // its alias, so a self-module import used only through a type
        // or a `m.Box { .. }` literal is used; the call-site marks the
        // checker recorded along the way are the second source.
        for head in qualified_import_heads(program) {
            if self.env.import_table.aliases.contains_key(&head) {
                self.env.import_table.used.insert(head);
            }
        }
        // Third source: a BARE protocol name in a generic bound or a
        // conformance list (`[S: Store]`, `type Mem: Store`). Protocols
        // resolve unqualified from any module, so the spelling names no
        // alias — the declaring module's canonical name (the protocol's
        // origin) is matched against each import's canonical instead.
        let bound_origins: std::collections::HashSet<Sym> = bare_protocol_refs(program).iter()
            .filter_map(|p| self.env.protocols.get(p).and_then(|d| d.origin))
            .collect();
        // The verdict needs a fully parsed file: recovery drops the text that
        // may have been the use, and the parse error is already the diagnosis.
        let judge_unused = !program.parse_recovered;
        for imp in program.imports.iter().filter(|_| judge_unused) {
            let (path, alias, span) = match imp {
                ast::Decl::Import { path, alias, span, .. } => (path, alias, span),
                _ => continue,
            };
            let import_name = alias.as_ref().cloned()
                .unwrap_or_else(|| path.last().cloned().unwrap_or_default());
            // A self-module import is judged like any other (#1783): its
            // alias enters `used` on qualified calls AND type positions, so
            // an unreferenced `import self.x as h` warns with the same
            // removal fix.
            let used_via_bound = self.env.import_table.aliases.get(&sym(&import_name))
                .is_some_and(|canon| bound_origins.contains(canon));
            if import_name.is_empty()
                || self.env.import_table.used.contains(&sym(&import_name))
                || import_name.starts_with('_')
                || used_via_bound
            { continue; }
            let line = span.as_ref().map(|s| s.line).unwrap_or(0);
            let mut diag = Diagnostic::warning(
                format!("unused import '{}'", import_name),
                format!("Remove the import or prefix with '_' to suppress: _{}", import_name),
                format!("import at line {}", line),
            ).with_code("E060");
            // Machine-applicable (#1486): NOTHING references the name, so
            // blanking the import line is a pure deletion — nothing is decided
            // for the author. (The duplicate-import sibling in import_table.rs
            // stays display-only: deleting a duplicate can strand references
            // to its alias, which needs a reference rewrite.)
            if let Some(s) = span {
                diag = diag.with_machine_fix(s.line, s.col, s.end_col, "");
                diag.line = Some(s.line);
                diag.col = Some(s.col);
                diag.end_col = Some(s.end_col);
            }
            self.diagnostics.push(diag);
        }
        self.check_reimpl_lint(program);
        std::mem::take(&mut self.diagnostics)
    }

    /// Reimpl lint — detect top-level user fns whose name is close to a
    /// stdlib fn AND whose signature matches exactly. Emits a Warning
    /// with a `try:` delegation shim so LLM retries can converge on
    /// the idiomatic one-liner. Opt-in strictness: a miss on any of
    /// (name distance ≤ 2, param count, param types, return type)
    /// suppresses the suggestion.
    ///
    /// Scope: top-level, non-monomorphized, non-derive, non-test fns.
    /// Roadmap: `docs/roadmap/active/reimpl-lint.md`.
    pub(crate) fn check_reimpl_lint(&mut self, program: &ast::Program) {
        for decl in &program.decls {
            let ast::Decl::Fn { name, params, return_type, span, .. } = decl else { continue };
            let user_name = name.as_str();
            if user_name.starts_with("__") { continue; }
            if user_name.contains('.') { continue; } // convention method like `Type.encode`
            let user_param_tys: Vec<Ty> = params.iter()
                .map(|p| self.resolve_type_expr(&p.ty))
                .collect();
            let user_ret = self.resolve_type_expr(return_type);
            if user_param_tys.iter().any(|t| matches!(t, Ty::Unknown)) { continue; }
            if matches!(user_ret, Ty::Unknown) { continue; }
            let Some((module, stdlib_fn)) = self.find_stdlib_reimpl(user_name, &user_param_tys, &user_ret)
                else { continue };
            let try_shim = format!(
                "fn {name}({params}) -> {ret} =\n    {module}.{fn}({args})",
                name = user_name,
                params = params.iter()
                    .map(|p| format!("{}: {}", p.name, self.resolve_type_expr(&p.ty).display()))
                    .collect::<Vec<_>>()
                    .join(", "),
                ret = user_ret.display(),
                module = module,
                fn = stdlib_fn,
                args = params.iter().map(|p| p.name.to_string()).collect::<Vec<_>>().join(", "),
            );
            let mut diag = Diagnostic::warning(
                format!("fn '{}' has the same signature as stdlib `{}.{}`", user_name, module, stdlib_fn),
                format!(
                    "If this is the standard algorithm, delegate to stdlib. \
                     Keep the local impl only if you need the specific behaviour that differs from `{}.{}`.",
                    module, stdlib_fn
                ),
                format!("fn {}", user_name),
            ).with_code("E015").with_try(try_shim);
            if let Some(s) = span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
                if s.end_col > s.col {
                    diag.end_col = Some(s.end_col);
                }
            }
            self.diagnostics.push(diag);
        }
    }

    /// Structural type-equality for reimpl-lint: `TypeVar` at the
    /// stdlib side matches any concrete Ty at the user side (a
    /// monomorphic `List[Int]` fn should match the generic
    /// `list.binary_search[T]`). Nested `Applied` compares
    /// element-wise, everything else is exact match.
    fn find_stdlib_reimpl(
        &self,
        user_name: &str,
        user_param_tys: &[Ty],
        user_ret: &Ty,
    ) -> Option<(&'static str, &'static str)> {
        let user_lc = user_name.to_ascii_lowercase();
        // Codepoint count, not `len()`: a byte-length gap overstates the
        // edit distance for non-ASCII identifiers and would drop a real
        // candidate.
        let user_chars = user_name.chars().count();
        // Best match, not first match. `atan` passes the gates against
        // both `math.atan` (distance 0) and `math.tan` (distance 1), so
        // taking the first candidate named whichever of the two the
        // module's fn list happened to yield first. Ranking by distance
        // makes the suggestion the closest name, and `module_fn_names`
        // is sorted, so the enumeration order breaks ties the same way
        // on every run.
        let mut best: Option<(usize, &'static str, &'static str)> = Option::None;
        for &module in almide_lang::stdlib_info::BUNDLED_MODULES {
            for fn_name in crate::stdlib::module_functions_all(module) {
                // Name-similarity filter, cheapest gate first. Edit
                // distance is never below the length difference, so a
                // gap over the `≤ 2` cap rules a candidate out without
                // running the O(len²) matrix. Then a substring gate (one
                // scan) so that common-shape collisions like
                // `fn add(Int, Int) -> Int` don't false-positive
                // against `int.band`. Require one name to contain
                // the other (case-insensitive) — catches typos
                // (`maps` ⊃ `map`), qualified renames
                // (`my_binary_search` ⊃ `binary_search`), and exact
                // matches, while excluding short stdlib names with
                // unrelated user fns. Only survivors reach `levenshtein`,
                // which used to run on every stdlib fn for every user fn
                // and dominated `almide check` (12.8% of self time on a
                // 3200-fn program that emits no diagnostics at all).
                if user_chars.abs_diff(fn_name.chars().count()) > 2 {
                    continue;
                }
                let fn_lc = fn_name.to_ascii_lowercase();
                if !(user_lc.contains(&fn_lc) || fn_lc.contains(&user_lc)) {
                    continue;
                }
                let dist = almide_base::diagnostic::levenshtein(user_name, fn_name);
                if dist > 2 {
                    continue;
                }
                if best.as_ref().is_some_and(|(d, _, _)| *d <= dist) {
                    continue;
                }
                let Some(sig) = crate::stdlib::lookup_sig(module, fn_name) else { continue };
                if sig.params.len() != user_param_tys.len() { continue; }
                if !sigs_match_structurally(&sig.params, &sig.ret, user_param_tys, user_ret) {
                    continue;
                }
                if dist == 0 {
                    return Some((module, fn_name));
                }
                best = Some((dist, module, fn_name));
            }
        }
        best.map(|(_, module, fn_name)| (module, fn_name))
    }

}

/// The sized INT widths (the sized floats are matched separately, since their
/// canonical peer is `Float`, not `Int`).
fn is_sized_int(t: &Ty) -> bool {
    matches!(
        t,
        Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
    )
}

/// The sized widths that are NOT the canonical runtime type. `Int64` and
/// `Float64` are deliberately absent: they are `Int` / `Float` at runtime and
/// `compatible` bridges them in BOTH directions, so a join that mixes them can
/// neither lose a width nor emit a rustc mismatch — rejecting it would be a
/// false rejection. Everything here is reachable only through the one-way
/// literal coercion, which is what #880 makes conditional on being a literal.
pub(crate) fn is_narrow_sized(t: &Ty) -> bool {
    matches!(
        t,
        Ty::Int8 | Ty::Int16 | Ty::Int32
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
            | Ty::Float32
    )
}

/// Whether `other` is the canonical type whose values coerce into `sized`
/// (`Int` / `Int64` for the integer widths, `Float` / `Float64` for `Float32`) —
/// the exact pairs `compatible_numeric_coerce` accepts one-way.
pub(crate) fn is_canonical_peer_of(sized: &Ty, other: &Ty) -> bool {
    match sized {
        Ty::Float32 => matches!(other, Ty::Float | Ty::Float64),
        s if is_narrow_sized(s) => matches!(other, Ty::Int | Ty::Int64),
        _ => false,
    }
}

/// Whether this expression is built ONLY from numeric literals — literals,
/// parentheses, negation, and arithmetic over them. Such an expression chose no
/// width of its own, so it adopts its context's; anything else is a value with a
/// width already, and mixing it with a different width is an error.
pub(crate) fn is_literal_numeric_ast(e: &ast::Expr) -> bool {
    match &e.kind {
        ast::ExprKind::Int { .. } | ast::ExprKind::Float { .. } => true,
        ast::ExprKind::Paren { expr } => is_literal_numeric_ast(expr),
        ast::ExprKind::Unary { op, operand } if op.as_str() == "-" => is_literal_numeric_ast(operand),
        ast::ExprKind::Binary { op, left, right } if matches!(op.as_str(), "+" | "-" | "*" | "/" | "%" | "^") => {
            is_literal_numeric_ast(left) && is_literal_numeric_ast(right)
        }
        _ => false,
    }
}

include!("post_solve_validation.rs");
include!("lint_error_surface.rs");
include!("bounded.rs");
include!("module_inference.rs");

/// Every import-alias head spelled in the program (#1783): the `h` of
/// `h.open()`, `m.Box { .. }`, `g.Point` in any type position, `c.Red` in a
/// pattern. Purely syntactic, so it is deterministic under the spine's
/// checker clones and complete over every usage form the resolver knows.
fn qualified_import_heads(program: &mut ast::Program) -> std::collections::HashSet<Sym> {
    use std::collections::HashSet;
    let mut heads: HashSet<Sym> = HashSet::new();
    fn head_of(name: Sym, heads: &mut HashSet<Sym>) {
        if let Some((h, _)) = name.as_str().split_once('.') {
            heads.insert(sym(h));
        }
    }
    fn walk_ty(te: &ast::TypeExpr, heads: &mut HashSet<Sym>) {
        match te {
            ast::TypeExpr::Simple { name } => head_of(*name, heads),
            ast::TypeExpr::Generic { name, args } => {
                head_of(*name, heads);
                for a in args { walk_ty(a, heads); }
            }
            ast::TypeExpr::Record { fields } | ast::TypeExpr::OpenRecord { fields } => {
                for f in fields { walk_ty(&f.ty, heads); }
            }
            ast::TypeExpr::Fn { params, ret, .. } => {
                for p in params { walk_ty(p, heads); }
                walk_ty(ret, heads);
            }
            ast::TypeExpr::Tuple { elements } | ast::TypeExpr::Union { members: elements } => {
                for e in elements { walk_ty(e, heads); }
            }
            ast::TypeExpr::Variant { cases } => {
                for c in cases {
                    match c {
                        ast::VariantCase::Tuple { fields, .. } => for t in fields { walk_ty(t, heads); },
                        ast::VariantCase::Record { fields, .. } => for f in fields { walk_ty(&f.ty, heads); },
                        ast::VariantCase::Unit { .. } => {}
                    }
                }
            }
            ast::TypeExpr::ConstLit { .. } => {}
        }
    }
    fn walk_pat(p: &ast::Pattern, heads: &mut HashSet<Sym>) {
        match p {
            ast::Pattern::Constructor { name, args } => {
                head_of(*name, heads);
                for a in args { walk_pat(a, heads); }
            }
            ast::Pattern::RecordPattern { name, fields, .. } => {
                head_of(*name, heads);
                for f in fields { if let Some(fp) = &f.pattern { walk_pat(fp, heads); } }
            }
            ast::Pattern::Tuple { elements } | ast::Pattern::List { elements, .. } => {
                for e in elements { walk_pat(e, heads); }
            }
            ast::Pattern::Some { inner } | ast::Pattern::Ok { inner } | ast::Pattern::Err { inner } => {
                walk_pat(inner, heads)
            }
            ast::Pattern::Or { alts } => for a in alts { walk_pat(a, heads); },
            ast::Pattern::Wildcard | ast::Pattern::Ident { .. } | ast::Pattern::None
            | ast::Pattern::Literal { .. } => {}
            // A pattern form this walker does not know (the #1461 as-pattern
            // lands separately) contributes no heads — an unmarked alias
            // there surfaces as a false E060, which the multi-file corpus
            // would catch; extend the walk when the form lands.
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }
    fn walk_generics(gs: &Option<Vec<ast::GenericParam>>, heads: &mut HashSet<Sym>) {
        for g in gs.iter().flatten() {
            if let Some(b) = &g.structural_bound { walk_ty(b, heads); }
        }
    }
    fn walk_where(wc: &ast::TestWhere, heads: &mut HashSet<Sym>) {
        match wc {
            ast::TestWhere::Override { path, .. } | ast::TestWhere::CallResponse { target: path, .. } => {
                if path.len() > 1 { heads.insert(path[0]); }
                if let ast::TestWhere::CallResponse { params, .. } = wc {
                    for p in params { walk_pat(p, heads); }
                }
            }
            ast::TestWhere::Case { bindings, .. } => for b in bindings { walk_where(b, heads); },
            ast::TestWhere::Bind { .. } => {}
        }
    }
    for decl in &program.decls {
        match decl {
            ast::Decl::Fn { params, return_type, generics, .. } => {
                walk_generics(generics, &mut heads);
                for p in params { walk_ty(&p.ty, &mut heads); }
                walk_ty(return_type, &mut heads);
            }
            ast::Decl::TopLet { ty: Some(t), .. } => walk_ty(t, &mut heads),
            ast::Decl::Type { ty, generics, .. } => {
                walk_generics(generics, &mut heads);
                walk_ty(ty, &mut heads);
            }
            ast::Decl::Protocol { generics, methods, .. } => {
                walk_generics(generics, &mut heads);
                for m in methods {
                    for p in &m.params { walk_ty(&p.ty, &mut heads); }
                    walk_ty(&m.return_type, &mut heads);
                }
            }
            ast::Decl::Test { where_clauses: wcs, .. } | ast::Decl::TestWhereDef { clauses: wcs, .. } => {
                for wc in wcs { walk_where(wc, &mut heads); }
            }
            ast::Decl::TopLet { ty: None, .. } | ast::Decl::Module { .. } | ast::Decl::Import { .. } => {}
        }
    }
    ast::visit_exprs_mut(program, &mut |e: &mut ast::Expr| {
        match &e.kind {
            ast::ExprKind::Ident { name } | ast::ExprKind::TypeName { name } => head_of(*name, &mut heads),
            ast::ExprKind::Record { name: Some(n), .. } => head_of(*n, &mut heads),
            ast::ExprKind::Call { type_args: Some(tas), .. } => {
                for t in tas { walk_ty(t, &mut heads); }
            }
            ast::ExprKind::TypeAscription { ty, .. } => walk_ty(ty, &mut heads),
            ast::ExprKind::Lambda { params, .. } => {
                for p in params { if let Some(t) = &p.ty { walk_ty(t, &mut heads); } }
            }
            ast::ExprKind::Block { stmts, .. } => {
                for st in stmts {
                    match st {
                        ast::Stmt::Let { ty: Some(t), .. } | ast::Stmt::Var { ty: Some(t), .. } => walk_ty(t, &mut heads),
                        ast::Stmt::LetDestructure { pattern, .. } => walk_pat(pattern, &mut heads),
                        _ => {}
                    }
                }
            }
            ast::ExprKind::Match { arms, .. } => {
                for arm in arms { walk_pat(&arm.pattern, &mut heads); }
            }
            _ => {}
        }
    });
    heads
}

/// Every protocol name spelled BARE in a bound or a conformance list
/// (#1783): `[S: Store]` on a fn, type or protocol, and `type Mem: Store`.
/// These resolve without a module alias, so `qualified_import_heads` cannot
/// see them; the E060 pass maps each through the protocol's origin instead.
fn bare_protocol_refs(program: &ast::Program) -> std::collections::HashSet<Sym> {
    fn take(gs: &Option<Vec<ast::GenericParam>>, refs: &mut std::collections::HashSet<Sym>) {
        for g in gs.iter().flatten() {
            for b in g.bounds.iter().flatten() { refs.insert(*b); }
        }
    }
    let mut refs = std::collections::HashSet::new();
    for decl in &program.decls {
        match decl {
            ast::Decl::Fn { generics, .. } | ast::Decl::Protocol { generics, .. } => take(generics, &mut refs),
            ast::Decl::Type { generics, deriving, .. } => {
                take(generics, &mut refs);
                for d in deriving.iter().flatten() { refs.insert(*d); }
            }
            _ => {}
        }
    }
    refs
}
