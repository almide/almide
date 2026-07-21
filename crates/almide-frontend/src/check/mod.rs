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
    pub(crate) deferred_field_accesses: Vec<(Ty, almide_base::intern::Sym, Ty)>,
    /// Map literal key types to validate after constraint solving.
    /// Each entry: (key_type, span) — checked via `is_hash()` once types are resolved.
    pub(crate) deferred_map_key_checks: Vec<(Ty, Option<crate::ast::Span>)>,
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

/// True when a bare (non-negative) integer literal does not fit in `i64`.
/// Mirrors the radix parsing in lowering so the check and the eventual value
/// agree. A malformed token the lexer would not produce is treated as
/// non-overflowing (not our error to report).
pub(crate) fn int_literal_overflows_i64(raw: &str) -> bool {
    let clean = raw.replace('_', "");
    let (radix, digits) = if let Some(r) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) { (16, r) }
        else if let Some(r) = clean.strip_prefix("0b").or_else(|| clean.strip_prefix("0B")) { (2, r) }
        else if let Some(r) = clean.strip_prefix("0o").or_else(|| clean.strip_prefix("0O")) { (8, r) }
        else { (10, clean.as_str()) };
    match i64::from_str_radix(digits, radix) {
        Ok(_) => false,
        Err(e) => matches!(e.kind(), std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow),
    }
}

/// True when `raw`'s magnitude fits the given type's range. For a SIGNED type
/// the magnitude bound is `MAX` (or `MAX+1` when `negated`, reaching `MIN`); for
/// an unsigned type it is the unsigned `MAX`. Non-integer types return false
/// (the literal does not belong there — left for the normal type checker).
pub(crate) fn int_literal_fits_type(raw: &str, ty: &Ty, negated: bool) -> bool {
    let clean = raw.replace('_', "");
    let (radix, digits) = if let Some(r) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) { (16, r) }
        else if let Some(r) = clean.strip_prefix("0b").or_else(|| clean.strip_prefix("0B")) { (2, r) }
        else if let Some(r) = clean.strip_prefix("0o").or_else(|| clean.strip_prefix("0O")) { (8, r) }
        else { (10, clean.as_str()) };
    let Ok(mag) = u128::from_str_radix(digits, radix) else { return true };
    // (signed, bit-width) for each integer type; None for non-integer.
    let limits: Option<(bool, u32)> = match ty {
        Ty::Int | Ty::Int64 => Some((true, 64)),
        Ty::Int8 => Some((true, 8)), Ty::Int16 => Some((true, 16)), Ty::Int32 => Some((true, 32)),
        Ty::UInt8 => Some((false, 8)), Ty::UInt16 => Some((false, 16)),
        Ty::UInt32 => Some((false, 32)), Ty::UInt64 => Some((false, 64)),
        _ => None,
    };
    match limits {
        None => true, // not an integer context — not our diagnostic
        Some((signed, bits)) => {
            let max: u128 = if signed {
                if negated { 1u128 << (bits - 1) } else { (1u128 << (bits - 1)) - 1 }
            } else {
                (1u128 << bits) - 1
            };
            mag <= max
        }
    }
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
            deferred_ord_elem_checks: Vec::new(),
            deferred_empty_collection_checks: Vec::new(),
            deferred_int_overflow_checks: Vec::new(),
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
        // Drain deferred field accesses: `obj.field` where `obj` was an
        // unresolved inference var at inference time. Now that constraints
        // are solved, resolve the field type and unify.
        loop {
            let pending = std::mem::take(&mut self.deferred_field_accesses);
            if pending.is_empty() { break; }
            let mut still_pending = Vec::new();
            let mut progressed = false;
            for (obj_ty, field, result_ty) in pending {
                let resolved = resolve_ty(&obj_ty, &self.uf);
                let field_ty = self.resolve_field_type(&resolved, field.as_str());
                if !matches!(field_ty, Ty::Unknown) {
                    self.unify_infer(&result_ty, &field_ty);
                    progressed = true;
                } else {
                    still_pending.push((obj_ty, field, result_ty));
                }
            }
            self.deferred_field_accesses = still_pending;
            if !progressed { break; }
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
        self.validate_ord_elem_types();
        self.validate_unknown_named_types();
        self.validate_empty_collection_elements();
        self.validate_int_overflow_literals();
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
                if almide_base::diagnostic::levenshtein(user_name, fn_name) > 2 {
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
                return Some((module, fn_name));
            }
        }
        Option::None
    }

    /// Type-check a module's declarations. Populates type_map for all expressions.
    /// Temporarily registers unprefixed declarations for intra-module resolution,
    /// then cleans them up.
    pub fn infer_module(&mut self, prog: &mut ast::Program, module_name: &str) {
        // Isolate module's constraint solving and type map from the main program
        let saved_constraints = std::mem::take(&mut self.constraints);
        let saved_uf = std::mem::replace(&mut self.uf, UnionFind::new());
        self.type_map.clear();

        // Build module's import table
        let self_name = self.env.self_module_name.map(|s| s.to_string());
        let import_table_name = self_name.as_deref().unwrap_or(module_name);
        let saved_import_table = std::mem::replace(&mut self.env.import_table, ImportTable::new());
        let (mod_table, diags) = build_import_table(prog, Some(import_table_name), &self.env.user_modules);
        self.env.import_table = mod_table;
        self.diagnostics.extend(diags);

        // Temporarily register unprefixed declarations for intra-module resolution
        let snapshot = self.env.snapshot_keys();
        crate::canonicalize::registration::register_decls(
            &mut self.env, &mut self.diagnostics, &prog.decls, None,
        );

        // Infer + solve + resolve
        let saved_prefix = std::mem::replace(
            &mut self.current_module_prefix,
            Some(module_name.to_string()),
        );
        for decl in prog.decls.iter_mut() { self.check_decl(decl); }
        self.solve_constraints();
        self.resolve_deferred_tuple_indices();
        self.flush_pending_toplet_tys();
        resolve_type_map(&mut self.type_map, &self.uf);
        self.validate_map_key_types();
        self.validate_empty_collection_elements();
        self.validate_int_overflow_literals();
        self.validate_unresolved_binding_types();
        self.current_module_prefix = saved_prefix;

        // Restore
        self.constraints = saved_constraints;
        self.uf = saved_uf;
        self.env.import_table = saved_import_table;
        self.env.restore_keys(&snapshot);
    }

    /// #785: re-infer ONLY this module's top-level `let`s so `env.top_lets`
    /// carries fully inferred types BEFORE the entry program is checked.
    /// The CLI drivers run `infer_program` (where every cross-module reader
    /// lives) before the `infer_module` loop, so without this pre-pass a
    /// reader sees the registration-time seed — `Unknown` for any
    /// non-literal initializer (`let K = neg_two()`). Same isolation
    /// bracket as `infer_module`; decls are cloned so the module AST stays
    /// pristine for the real inference later.
    pub fn refresh_module_top_lets(&mut self, prog: &ast::Program, module_name: &str) {
        if !prog.decls.iter().any(|d| matches!(d, ast::Decl::TopLet { .. })) {
            return;
        }
        let saved_constraints = std::mem::take(&mut self.constraints);
        let saved_uf = std::mem::replace(&mut self.uf, UnionFind::new());
        let saved_type_map = std::mem::take(&mut self.type_map);
        // This is a TYPE-EXTRACTION pre-pass, not the module's real check —
        // `infer_module` re-infers everything later and owns the reporting.
        // Emitting here would DOUBLE-report every real initializer error and
        // fire spurious ambiguity (the temp unprefixed decls coexist with the
        // canonical prefixed ones, so a bare ctor initializer like
        // `let MOOD = Happy` sees its own type twice). Discard everything
        // this pass emits.
        let saved_diag_len = self.diagnostics.len();
        // Deferred-check sites registered here carry TypeVars owned by THIS
        // pass's union-find; validating them later against the real pass's UF
        // would mis-resolve (false E018/E025 or silent misses). The real pass
        // re-registers every site, so truncate the pre-pass's additions away.
        let saved_deferred_lens = (
            self.deferred_tuple_indices.len(),
            self.deferred_field_accesses.len(),
            self.deferred_map_key_checks.len(),
            self.deferred_ord_elem_checks.len(),
            self.deferred_unknown_type_checks.len(),
            self.deferred_empty_collection_checks.len(),
            self.deferred_int_overflow_checks.len(),
            self.deferred_unresolved_binding_checks.len(),
        );

        let self_name = self.env.self_module_name.map(|s| s.to_string());
        let import_table_name = self_name.as_deref().unwrap_or(module_name);
        let saved_import_table = std::mem::replace(&mut self.env.import_table, ImportTable::new());
        let (mod_table, _diags) = build_import_table(prog, Some(import_table_name), &self.env.user_modules);
        self.env.import_table = mod_table;

        let snapshot = self.env.snapshot_keys();
        crate::canonicalize::registration::register_decls(
            &mut self.env, &mut Vec::new(), &prog.decls, None,
        );
        let saved_prefix = std::mem::replace(
            &mut self.current_module_prefix,
            Some(module_name.to_string()),
        );
        for decl in prog.decls.iter() {
            if matches!(decl, ast::Decl::TopLet { .. }) {
                let mut d = decl.clone();
                self.check_decl(&mut d);
            }
        }
        self.solve_constraints();
        self.flush_pending_toplet_tys();
        self.current_module_prefix = saved_prefix;

        self.constraints = saved_constraints;
        self.uf = saved_uf;
        self.type_map = saved_type_map;
        self.env.import_table = saved_import_table;
        self.env.restore_keys(&snapshot);
        self.diagnostics.truncate(saved_diag_len);
        self.deferred_tuple_indices.truncate(saved_deferred_lens.0);
        self.deferred_field_accesses.truncate(saved_deferred_lens.1);
        self.deferred_map_key_checks.truncate(saved_deferred_lens.2);
        self.deferred_ord_elem_checks.truncate(saved_deferred_lens.3);
        self.deferred_unknown_type_checks.truncate(saved_deferred_lens.4);
        self.deferred_empty_collection_checks.truncate(saved_deferred_lens.5);
        self.deferred_int_overflow_checks.truncate(saved_deferred_lens.6);
        self.deferred_unresolved_binding_checks.truncate(saved_deferred_lens.7);
    }

    /// Upgrade `env.top_lets` entries from the POST-solve resolution of their
    /// initializer types. The `TopLet` branch writes pre-solve (all it can do
    /// mid-pass), which leaves a generic-ctor initializer's payload Unknown;
    /// this pass replaces any still-partially-Unknown entry once the
    /// union-find actually knows the answer. Only a FULLY concrete resolution
    /// upgrades — swapping one partial type for another would churn without
    /// fixing readers. Must run before the calling flow swaps its UF back.
    fn flush_pending_toplet_tys(&mut self) {
        for (key, ity) in std::mem::take(&mut self.pending_toplet_tys) {
            let existing = self.env.top_lets.get(&key);
            let stale = match existing {
                None => true,
                Some(t) => t.contains_unknown() || t.contains_typevar(),
            };
            if !stale {
                continue;
            }
            let r = resolve_ty(&ity, &self.uf);
            if !r.contains_unknown() && !r.contains_typevar() {
                self.env.top_lets.insert(key, r);
            }
        }
    }

    // ── Declaration checking ──

    /// Push generic type vars, structural bounds, and protocol bounds into the environment.
    fn enter_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>) {
        use crate::canonicalize::registration::SCALAR_TYPE_NAMES;
        let gs = match generics { Some(gs) => gs, None => return };
        for g in gs.iter() {
            // Check if this is a const param (single scalar type bound)
            let is_const = g.bounds.as_ref().map_or(false, |bs| {
                bs.len() == 1 && SCALAR_TYPE_NAMES.contains(&bs[0].as_str())
            });
            if is_const {
                let ty = self.resolve_type_expr(&ast::TypeExpr::Simple { name: sym(&g.bounds.as_ref().unwrap()[0]) });
                self.env.types.insert(sym(&g.name), Ty::ConstParam { name: sym(&g.name), ty: Box::new(ty) });
            } else {
                self.env.types.insert(sym(&g.name), Ty::TypeVar(sym(&g.name)));
            }
            if let Some(bte) = &g.structural_bound {
                let bt = self.resolve_type_expr(bte);
                self.env.structural_bounds.insert(sym(&g.name), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
            }
            if let Some(bounds) = &g.bounds {
                if !bounds.is_empty() && !is_const {
                    self.env.generic_protocol_bounds.insert(sym(&g.name), bounds.iter().map(|b| sym(b)).collect());
                }
            }
        }
    }

    /// Remove generic type vars, structural bounds, and protocol bounds from the environment.
    fn exit_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>) {
        let gs = match generics { Some(gs) => gs, None => return };
        for g in gs.iter() {
            self.env.types.remove(&sym(&g.name));
            self.env.structural_bounds.remove(&sym(&g.name));
            self.env.generic_protocol_bounds.remove(&sym(&g.name));
        }
    }

    /// Constrain an effect fn body against its return type signature.
    /// Effect fns accept: Unit body (control-flow returns), unwrapped T, or full Result[T, E].
    fn constrain_effect_body(&mut self, name: &str, ret_ty: &Ty, body_ty: Ty) {
        let body_resolved = resolve_ty(&body_ty, &self.uf);
        if body_resolved == Ty::Unit { return; } // while loops, guard patterns return via control flow
        if let Ty::Applied(crate::types::TypeConstructorId::Result, args) = ret_ty {
            // ret_ty is Result[T, E]: body can be Result[T, E] or unwrapped T
            if args.len() >= 1 {
                let ok = &args[0];
                if body_resolved.is_result() {
                    self.constrain(ret_ty.clone(), body_ty, format!("fn '{}'", name));
                } else {
                    self.constrain(ok.clone(), body_ty, format!("fn '{}'", name));
                }
                return;
            }
        }
        // ret_ty is non-Result (e.g. String): body can be T or Result[T, E] (auto-unwrapped)
        if let Ty::Applied(crate::types::TypeConstructorId::Result, ref args) = body_resolved {
            if args.len() >= 1 {
                self.constrain(ret_ty.clone(), args[0].clone(), format!("fn '{}'", name));
                return;
            }
        }
        self.constrain(ret_ty.clone(), body_ty, format!("fn '{}'", name));
    }

    fn check_fn_decl(
        &mut self,
        name: &str,
        params: &mut [ast::Param],
        return_type: &ast::TypeExpr,
        body: &mut ast::Expr,
        effect: &Option<bool>,
        generics: &mut Option<Vec<ast::GenericParam>>,
    ) {
        self.env.push_scope();
        self.enter_generics(generics);
        // A bare `self` first param is sugar for `self: Self` (see
        // registration.rs's matching fix). `Self` only stays an unresolved
        // placeholder inside a `protocol { ... }` declaration; on an actual
        // convention method it must resolve to the enclosing type.
        let receiver_ty = name.split_once('.').map(|(ty_name, _)| Ty::Named(sym(ty_name), Vec::new()));
        for (i, p) in params.iter_mut().enumerate() {
            let ty = if i == 0 && p.name.as_str() == "self"
                && matches!(&p.ty, ast::TypeExpr::Simple { name: tn } if tn.as_str() == "Self")
            {
                receiver_ty.clone().unwrap_or_else(|| self.resolve_type_expr(&p.ty))
            } else {
                self.resolve_type_expr(&p.ty)
            };
            self.deferred_unknown_type_checks.push((
                ty.clone(), self.current_span, format!("parameter '{}'", p.name),
            ));
            self.env.define_var(&p.name, ty.clone());
            self.env.param_vars.insert(sym(&p.name));
            if p.is_mut { self.env.mutable_vars.insert(sym(&p.name)); }
            if let Some(ref mut default_expr) = p.default {
                let dty = self.infer_expr(default_expr);
                self.constrain(ty, dty, format!("default arg '{}'", p.name));
            }
        }
        let ret_ty = self.resolve_type_expr(return_type);
        self.deferred_unknown_type_checks.push((
            ret_ty.clone(), self.current_span, format!("return type of '{}'", name),
        ));
        let prev = (self.env.current_ret.take(), self.env.can_call_effect, self.env.auto_unwrap, self.env.lambda_depth);
        let is_effect = effect.unwrap_or(false);
        self.env.current_ret = Some(ret_ty.clone());
        self.env.can_call_effect = is_effect;
        self.env.auto_unwrap = is_effect;
        self.env.lambda_depth = 0;
        let body_ity = self.infer_expr(body);
        if effect.unwrap_or(false) {
            self.constrain_effect_body(name, &ret_ty, body_ity);
        } else {
            // Capture the trailing `let` binding name (if any) to specialize
            // the Unit-leak E001 try: snippet downstream.
            let hint = trailing_let_name(body).map(FixHint::LastLetName);
            self.constrain_with_hint(ret_ty, body_ity, format!("fn '{}'", name), hint);
        }
        self.env.current_ret = prev.0; self.env.can_call_effect = prev.1; self.env.auto_unwrap = prev.2; self.env.lambda_depth = prev.3;
        self.exit_generics(generics);
        self.env.pop_scope();
    }

    fn check_decl(&mut self, decl: &mut ast::Decl) {
        match decl {
            ast::Decl::Fn { name, params, return_type, body: Some(body), effect, generics, .. } => {
                self.check_fn_decl(name, params, return_type, body, effect, generics);
            }
            ast::Decl::Test { body, where_clauses, .. } => {
                let wcs = where_clauses.clone();
                self.env.push_scope();
                let prev_call = self.env.can_call_effect; self.env.can_call_effect = true;
                let prev_test = self.env.in_test_block; self.env.in_test_block = true;
                // ONE bind-type map across the whole clause list, so same-name
                // bindings in DIFFERENT `where [...]` cases unify (see
                // `infer_test_where_collect`'s Bind arm).
                let mut seen_binds = std::collections::HashMap::new();
                for wc in &wcs { self.infer_test_where_collect(wc, &mut seen_binds); }
                self.infer_expr(body);
                self.env.in_test_block = prev_test;
                self.env.can_call_effect = prev_call;
                self.env.pop_scope();
            }
            ast::Decl::TestWhereDef { clauses, .. } => {
                let wcs = clauses.clone();
                for wc in &wcs { self.infer_test_where_inner(wc); }
            }
            ast::Decl::TopLet { name, ty, value, mutable, .. } => {
                if *mutable { self.env.mutable_vars.insert(sym(name)); }
                let ity = self.infer_expr(value);
                // A declared type annotation on a top-level `let`/`var` is the
                // source of truth — flow it into the value so an annotated empty
                // collection (`var items: List[Int] = []`) pins its element, the
                // same as a local typed `let` binding. (Was dropped here, so the
                // element stayed undecidable and tripped E018.)
                if let Some(te) = ty {
                    let declared = self.resolve_type_expr(te);
                    self.constrain(declared, ity.clone(), format!("top let {}", name));
                }
                let resolved = resolve_ty(&ity, &self.uf);
                // Update env.top_lets with the fully inferred type.
                // `register_decls` seeds module top_lets under the
                // prefixed key (`util.ANON`), so without this we'd only
                // refresh the unprefixed intra-module alias — lowering
                // reads the prefixed key and gets `Ty::Unknown`.
                let prefixed_key = self.current_module_prefix.as_ref()
                    .map(|p| sym(&format!("{}.{}", p, name)));
                if std::env::var_os("ALMIDE_TOPLET_DEBUG").is_some() {
                    eprintln!("[toplet-debug] refresh: name={} prefix={:?} resolved={:?} existing_prefixed={:?}",
                        name, self.current_module_prefix,
                        resolved,
                        prefixed_key.as_ref().map(|k| self.env.top_lets.get(k)));
                }
                if let Some(k) = prefixed_key {
                    if matches!(self.env.top_lets.get(&k), Some(Ty::Unknown) | None) {
                        self.env.top_lets.insert(k, resolved.clone());
                    }
                    self.pending_toplet_tys.push((k, ity.clone()));
                }
                if matches!(self.env.top_lets.get(&sym(name)), Some(Ty::Unknown) | None) {
                    self.env.top_lets.insert(sym(name), resolved);
                }
                self.pending_toplet_tys.push((sym(name), ity));
            }
            ast::Decl::Type { ty, .. } => {
                // Infer types for default value expressions in variant record fields
                infer_default_exprs(self, ty);
            }
            _ => {}
        }
    }

    // ── Exhaustiveness ──

    fn infer_test_where_inner(&mut self, wc: &ast::TestWhere) {
        let mut seen = std::collections::HashMap::new();
        self.infer_test_where_collect(wc, &mut seen);
    }

    fn infer_test_where_collect(
        &mut self,
        wc: &ast::TestWhere,
        seen: &mut std::collections::HashMap<Sym, Ty>,
    ) {
        match wc {
            ast::TestWhere::Bind { name, value } => {
                let mut val = value.clone();
                let ty = self.infer_expr(&mut val);
                // A `where greet = (name) => ...` binding shadows an existing
                // top-level function. Unify the inferred lambda type with that
                // function's signature so the lambda's parameter type variables
                // get pinned — otherwise they stay unbound and leak into the IR
                // as Unknown, tripping the ConcretizeTypes postcondition.
                self.unify_where_override_with_fn_sig(&[*name], &ty);
                // A CASE-table binding (`"add" [op = (a,b) => a+b, …]` / `"mul"
                // [op = …]`): each case re-binds the SAME name, but the test body
                // is inferred ONCE — against the LAST binding only. Unify every
                // same-name case binding with the first, so the body's call site
                // pins ALL cases' lambda param tyvars through the union-find (the
                // per-case lowering re-lowers the shared body, so the cases must
                // agree on types anyway — a heterogeneous table was never
                // lowerable). Without this, earlier cases' annotation-less lambda
                // params stayed unbound and leaked into the IR as Unknown.
                if let Some(prev) = seen.get(name) {
                    let prev = prev.clone();
                    self.unify_infer(&prev, &ty);
                } else {
                    seen.insert(*name, ty.clone());
                }
                let resolved = resolve_ty(&ty, &self.uf);
                self.env.define_var(name.as_str(), resolved);
            }
            ast::TestWhere::Override { path, value } => {
                let mut v = value.clone();
                let ty = self.infer_expr(&mut v);
                self.unify_where_override_with_fn_sig(path, &ty);
                let resolved = resolve_ty(&ty, &self.uf);
                let override_name = format!("__where_{}", path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("_"));
                self.env.define_var(&override_name, resolved);
            }
            ast::TestWhere::CallResponse { target, params, response } => {
                // Resolve param types from original function signature
                let target_name = if target.len() == 1 { *target.first().unwrap() }
                    else { sym(&target.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".")) };
                let sig_params: Vec<Ty> = self.env.functions.get(&target_name)
                    .map(|sig| sig.params.iter().map(|(_, t)| t.clone()).collect())
                    .unwrap_or_default();
                let param_vars: Vec<_> = params.iter().filter_map(|pat| {
                    if let ast::Pattern::Ident { name } = pat { Some(*name) } else { None }
                }).collect();
                let param_tys: Vec<_> = param_vars.iter().enumerate().map(|(i, pname)| {
                    let ty = sig_params.get(i).cloned().unwrap_or_else(|| self.fresh_var());
                    self.env.define_var(pname.as_str(), ty.clone());
                    ty
                }).collect();
                let mut r = response.clone();
                let ret_ty = self.infer_expr(&mut r);
                let ret_resolved = resolve_ty(&ret_ty, &self.uf);
                let fn_ty = Ty::Fn { params: param_tys, ret: Box::new(ret_resolved) };
                let override_name = format!("__where_{}", target.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("_"));
                self.env.define_var(&override_name, fn_ty);
            }
            ast::TestWhere::Case { bindings, .. } => {
                for b in bindings { self.infer_test_where_collect(b, seen); }
            }
        }
    }

    /// Unify a `where`-clause override value's inferred type with the
    /// shadowed top-level function's signature, pinning the override
    /// lambda's parameter type variables. `path` is the overridden
    /// function's name path (`["greet"]` or `["http", "get"]`); `value_ty`
    /// is the inferred type of the override expression.
    ///
    /// Without this, an override like `where greet = (name) => ...` leaves
    /// `name`'s type variable unbound — it resolves to Unknown and leaks
    /// into the IR, tripping the ConcretizeTypes postcondition (and falling
    /// back to a wrong WASM ValType for non-i32 params).
    ///
    /// No-op when the path names no known function or the signature is
    /// generic: unifying a lambda's `?N` param with a named TypeVar would
    /// resolve it to `A`, which is itself unresolved at codegen time.
    fn unify_where_override_with_fn_sig(&mut self, path: &[Sym], value_ty: &Ty) {
        let name = if path.len() == 1 {
            path[0]
        } else {
            sym(&path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("."))
        };
        let Some(sig) = self.env.functions.get(&name) else { return };
        let sig_ty = Ty::Fn {
            params: sig.params.iter().map(|(_, t)| t.clone()).collect(),
            ret: Box::new(sig.ret.clone()),
        };
        let mut typevars = Vec::new();
        TypeEnv::collect_typevars(&sig_ty, &mut typevars);
        if !typevars.is_empty() { return; }
        self.unify_infer(&sig_ty, value_ty);
    }
}

include!("mod_p2.rs");
