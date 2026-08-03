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
mod infer;
pub(crate) mod calls;
mod retired_aliases;
mod builtin_calls;
mod static_dispatch;
mod solving;
mod diagnostics;
mod exhaustiveness;

use almide_lang::ast;
use almide_base::diagnostic::Diagnostic;
use crate::import_table::{ImportTable, build_import_table};
use almide_base::intern::{Sym, sym};
use crate::types::{Ty, TypeEnv};
use types::{TyVarId, Constraint, FixHint, UnionFind, resolve_ty};

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

pub struct Checker {
    pub env: TypeEnv,
    pub type_map: crate::types::TypeMap,
    pub diagnostics: Vec<Diagnostic>,
    pub source_file: Option<String>,
    pub source_text: Option<String>,
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
            ExprKind::Float { value: v } => return Some((cur.id, *v)),
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
            current_span: None,
            callee_span_hint: None,
            call_span_hint: None,
            last_mut_params: Vec::new(),
            arg_spans: Vec::new(),
            named_arg_meta: None,
            lambda_arg_hint: None,
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

    /// Let-polymorphism: instantiate で TypeVar("?N") を fresh var に置換
    /// 同じ let binding を2回参照する時、各参照で独立した型変数を使う
    pub(crate) fn instantiate_ty(&mut self, ty: &Ty) -> Ty {
        let mut mapping: std::collections::HashMap<u32, TyVarId> = std::collections::HashMap::new();
        self.instantiate_inner(ty, &mut mapping)
    }

    fn instantiate_inner(&mut self, ty: &Ty, mapping: &mut std::collections::HashMap<u32, TyVarId>) -> Ty {
        // Inference variables (?N) must NOT be freshened — they need to stay
        // linked to the original constraint.
        if matches!(ty, Ty::TypeVar(name) if name.starts_with('?')) {
            return ty.clone();
        }
        // Recursively instantiate all children
        ty.map_children_mut(&mut |child| self.instantiate_inner(child, mapping))
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
                Ty::Applied(crate::types::TypeConstructorId::Result, ref args) if args.len() == 2 => args[0].clone(),
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
        // Unused import warnings
        for imp in &program.imports {
            let (path, alias, span) = match imp {
                ast::Decl::Import { path, alias, span, .. } => (path, alias, span),
                _ => continue,
            };
            let import_name = alias.as_ref().cloned()
                .unwrap_or_else(|| path.last().cloned().unwrap_or_default());
            if import_name.is_empty()
                || self.env.import_table.used.contains(&sym(&import_name))
                || import_name.starts_with('_')
                || path.first().map(|s| s.as_str()) == Some("self")
            { continue; }
            let line = span.as_ref().map(|s| s.line).unwrap_or(0);
            self.diagnostics.push(Diagnostic::warning(
                format!("unused import '{}'", import_name),
                format!("Remove the import or prefix with '_' to suppress: _{}", import_name),
                format!("import at line {}", line),
            ));
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
                // Name-similarity filter: coarse `≤ 2` Levenshtein
                // gate (cheap), then a substring gate so that
                // common-shape collisions like
                // `fn add(Int, Int) -> Int` don't false-positive
                // against `int.band`. Require one name to contain
                // the other (case-insensitive) — catches typos
                // (`maps` ⊃ `map`), qualified renames
                // (`my_binary_search` ⊃ `binary_search`), and exact
                // matches, while excluding short stdlib names with
                // unrelated user fns.
                let dist = almide_base::diagnostic::levenshtein(user_name, fn_name);
                if dist > 2 {
                    continue;
                }
                if best.as_ref().is_some_and(|(d, _, _)| *d <= dist) {
                    continue;
                }
                let fn_lc = fn_name.to_ascii_lowercase();
                if !(user_lc.contains(&fn_lc) || fn_lc.contains(&user_lc)) {
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
include!("module_inference.rs");
