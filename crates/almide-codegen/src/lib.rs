#![recursion_limit = "512"]
//! Codegen v3: Three-layer architecture
//!
//! ```text
//! IrProgram (typed IR)
//!     ↓
//! Layer 1: Core IR normalization (target-agnostic)
//!     ↓
//! Layer 2: Semantic Rewrite (target-specific Nanopass pipeline)
//!     ↓
//! Layer 3: Emit (target-specific output)
//!     Rust/TS/JS → Template Renderer (TOML-driven syntax) → source code
//!     WASM       → Direct binary emit → .wasm bytes
//! ```
//!
//! Single entry point: `codegen(program, target) → CodegenOutput`
//!
//! Design references:
//! - MLIR progressive lowering (dialect conversion)
//! - Haxe Reflaxe (plugin trait for target addition)
//! - Nanopass framework (many small passes, each doing one thing)
//! - NLLB-200 (shared encoder + language-specific decoder)
//! - Cranelift ISLE (rules-as-data for verifiability)

pub mod annotations;
pub mod generated;
pub mod pass;
pub mod verify_names;
pub mod pass_auto_parallel;
pub mod pass_borrow_inference;
pub mod pass_box_deref;
pub mod pass_builtin_lowering;
pub mod pass_capture_clone;
pub mod pass_clone;
pub mod pass_alias_cow;
pub mod pass_top_let_storage;
pub mod pass_fan_lowering;
pub mod pass_list_pattern;
pub mod pass_match_subject;
pub mod pass_pattern_literal_guard;
pub mod pass_result_propagation;
pub mod pass_intrinsic_lowering;
pub mod pass_normalize_runtime_calls;
pub mod pass_stdlib_lowering;
pub mod pass_effect_inference;
pub mod pass_tco;
pub mod pass_tail_call_mark;
pub mod pass_licm;
pub mod pass_peephole;
pub mod pass_anf;
pub mod pass_stack_balance;
pub mod pass_perceus;
pub mod pass_globalize_closure_ids;
pub mod pass_canonicalize;
pub mod perceus_verified;
pub mod pass_egg_saturation;
pub mod pass_matrix_shape_spec;
pub mod pass_const_fold;
pub mod pass_rust_lowering;
pub mod pass_lambda_type_resolve;
pub mod pass_concretize_types;
pub mod pass_resolve_calls;
pub mod pass_closure_conversion;
pub mod pass_mut_param_lowering;
pub mod pass_unify_var_tables;
pub mod pass_ir_link_flatten;
pub mod template;
pub mod target;
pub mod walker;
pub mod reachability;
pub mod emit_wgsl;

use almide_ir::*;
use pass::Target;

/// Codegen output: source code for text targets, binary for WASM.
pub enum CodegenOutput {
    Source(String),
    Binary(Vec<u8>),
}

/// Options that control codegen behavior beyond target selection.
#[derive(Debug, Clone, Default)]
pub struct CodegenOptions {
    /// Emit `#[repr(C)]` on structs/enums for stable C ABI layout.
    pub repr_c: bool,
    /// Waiver for the Perceus RC gate (`--emit-unverified`): when a function
    /// fails Perceus verification, emit it anyway with a warning instead of
    /// refusing the build. A violation is a compiler bug (callee-inserted RC,
    /// not user code), so the default is a hard error; this flag exists only as
    /// an explicit, recorded escape hatch for shipping despite a known leak.
    pub allow_unverified: bool,
}

/// Marker the Rust emitter inserts between the inlined runtime preamble and the
/// user code. The rlib fast path splits generated source here: everything before
/// is the runtime (provided instead by the precompiled `almide_rt` rlib), and
/// everything after — the user code — is wrapped into a slim main that links it.
pub const RT_BOUNDARY_MARKER: &str = "//__ALMIDE_RT_BOUNDARY__";

/// Rebuild full generated Rust source into a slim main that links the `almide_rt`
/// rlib instead of inlining the runtime. Splits on [`RT_BOUNDARY_MARKER`]: the
/// user code (after the marker) is prefixed with the crate attrs, std imports, and
/// `extern crate almide_rt`. Returns `None` if the marker is absent (e.g. the
/// source predates this emitter or isn't a Rust target) so callers fall back.
pub fn slim_main_with_external_runtime(full_rs: &str) -> Option<String> {
    let idx = full_rs.find(RT_BOUNDARY_MARKER)?;
    let user_code = full_rs[idx + RT_BOUNDARY_MARKER.len()..].trim_start_matches('\n');
    let mut out = String::new();
    out.push_str("#![allow(unused_parens, unused_variables, dead_code, unused_imports, unused_mut, unused_must_use)]\n\n");
    out.push_str("use std::collections::HashMap;\nuse std::collections::HashSet;\n");
    out.push_str("#[macro_use] extern crate almide_rt;\nuse almide_rt::*;\n\n");
    out.push_str(user_code);
    Some(out)
}

/// Strip `mod tests { ... }` blocks from runtime source (avoid conflicts with user tests)
fn strip_test_blocks(src: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    let mut in_test_mod = false;
    for line in src.lines() {
        let trimmed = line.trim();
        if !in_test_mod && (trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("mod tests")) {
            in_test_mod = true;
            depth = 0;
        }
        if in_test_mod {
            for ch in line.chars() {
                if ch == '{' { depth += 1; }
                if ch == '}' { depth -= 1; }
            }
            if depth <= 0 && line.contains('}') {
                in_test_mod = false;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Unified codegen entry point: IR → Nanopass pipeline → target output.
///
/// Handles all targets through a single path:
/// - Rust/TS/JS: Nanopass → Walker (template renderer) → source code
/// - WASM: Nanopass → direct binary emit → .wasm bytes
pub fn codegen(program: &mut IrProgram, target: Target) -> CodegenOutput {
    codegen_with(program, target, &CodegenOptions::default())
}

/// Matrix ops with a native (Rust) intrinsic but no WASM lowering: no primitive
/// decomposition (`@rewrite` desugar) and no hand-written wasm arm, so the wasm
/// emitter cannot lower a call to them. Currently just `qwen3_block_q1_0_kv` — a
/// packed-GGUF Qwen3 block (RoPE + GQA + KV-cache, 3-matrix tuple return) that
/// only exists as a native fast path. Add an op here if you introduce another
/// native-only matrix intrinsic; the CLI then rejects WASM builds that call it
/// with a clear message instead of letting the emitter ICE.
pub const NATIVE_ONLY_MATRIX_OPS: &[&str] = &["qwen3_block_q1_0_kv"];

/// If the program calls a native-only matrix op (see [`NATIVE_ONLY_MATRIX_OPS`]),
/// return its name. The CLI uses this to reject WASM builds at build time with a
/// clear diagnostic rather than an emitter ICE.
pub fn program_uses_native_only_matrix_on_wasm(program: &IrProgram) -> Option<&'static str> {
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
    use almide_ir::{CallTarget, IrExprKind};
    struct Scan {
        found: Option<&'static str>,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if self.found.is_some() {
                return;
            }
            if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } = &expr.kind {
                if module.as_str() == "matrix" {
                    let f = func.as_str();
                    if let Some(op) = NATIVE_ONLY_MATRIX_OPS.iter().copied().find(|o| *o == f) {
                        self.found = Some(op);
                        return;
                    }
                }
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
            if self.found.is_some() {
                return;
            }
            walk_stmt(self, stmt);
        }
    }
    // Only REACHABLE bodies can block the build: an unreachable native-only op
    // (e.g. in an imported module's unused fn) is pruned by the WASM emitter, so it
    // must not fail the build. Uses the SAME reachability the emitter prunes by, so
    // the pre-check and the emit agree (#644).
    let reachable = reachability::reachable_fn_names(program);
    let is_reachable = |module: Option<&str>, name: &str| {
        reachability::registered_keys(module, name)
            .iter()
            .any(|k| reachable.contains(k))
    };
    let mut scan = Scan { found: None };
    for func in &program.functions {
        if !is_reachable(None, func.name.as_str()) {
            continue;
        }
        scan.visit_expr(&func.body);
        if scan.found.is_some() {
            return scan.found;
        }
    }
    for m in &program.modules {
        let mname = m.name.to_string();
        for func in &m.functions {
            if !is_reachable(Some(&mname), func.name.as_str()) {
                continue;
            }
            scan.visit_expr(&func.body);
            if scan.found.is_some() {
                return scan.found;
            }
        }
    }
    None
}

/// AlmidePerceusBelt: type-state verified program.
///
/// Construction requires passing Perceus RC verification (Lean 4 certified).
/// WASM emit accepts only Verified — unverified programs cannot reach emission.
/// This is the type-level enforcement of RC correctness, analogous to how
/// Rust's borrow checker prevents use-after-free at the type level.
pub struct Verified<'a>(pub(crate) &'a IrProgram);

impl<'a> Verified<'a> {
    /// Verify Perceus RC balance and construct the `Verified` gate.
    ///
    /// A violation is a bug in the compiler's *own* inserted RC ops (the callee,
    /// not user code), so the receiver who can act on it is the compiler author,
    /// not the user — the diagnostic class is therefore ICE, and the default is a
    /// hard refusal to emit (controlled `process::exit`, mirroring
    /// [`pass_concretize_types::assert_types_concretized`], NOT a `panic!`
    /// backtrace). "leaks, not unsoundness" sets the message's tone, not whether
    /// the gate opens. `allow_unverified` (`--emit-unverified`) is the explicit
    /// waiver: emit despite a known leak, with a warning, for deliberate use.
    fn verify(program: &'a IrProgram, allow_unverified: bool) -> Self {
        let mut total_violations = 0usize;
        for func in &program.functions {
            if func.is_test { continue; }
            let violations = pass_perceus::perceus_verify_function(func, &program.var_table);
            total_violations += violations;
        }
        if total_violations > 0 {
            if allow_unverified {
                eprintln!(
                    "[Perceus] warning: {total_violations} RC violation(s) emitted unverified \
                     (--emit-unverified) — the WASM artifact may leak memory"
                );
            } else {
                // ICE convention: same shape as `assert_types_concretized` — a
                // `[COMPILER BUG]` diagnostic + controlled `process::exit`, NOT a
                // numeric E-code (those collide with the rustc `E060x` space the
                // diagnostics registry reserves) and NOT a `panic!` backtrace.
                eprintln!(
                    "error: [COMPILER BUG] Perceus RC verification failed\n  \
                     {total_violations} function-internal RC violation(s) (see the [perceus-belt] \
                     lines above) would emit a WASM artifact that leaks or double-frees memory, so \
                     the build is refused.\n  \
                     This is a bug in the compiler's inserted reference counting, NOT an error in \
                     your program — you cannot fix it in source.\n  \
                     hint: please report this at https://github.com/almide/almide/issues with the \
                     source above; to ship the leaky artifact anyway, re-run with --emit-unverified."
                );
                std::process::exit(1);
            }
        }
        Self(program)
    }
}

/// AlmidePerceusBelt — determinism layer (L3). Sibling of [`Verified`].
///
/// A function's WASM index is its position in `program.functions` /
/// `module.functions`. `Canonical` certifies those Vecs are in a
/// content-derived canonical order (the [`pass_canonicalize`] postcondition), so
/// the emitted module order is a pure function of the program — not of a host's
/// `HashMap` iteration or a pass's append order. WASM emit accepts only
/// `Canonical`, so a program reaches byte emission only after canonicalization:
/// the type-level guarantee that the in-browser (wasm32) compiler cannot emit a
/// module that diverges from the native one. This is the order-determinism
/// analogue of how `Verified` gates emit on RC balance.
///
/// `certify` consumes a [`Verified`], so "RC-verified" is a prerequisite of
/// "canonical" — you cannot mint this certificate for an unverified program.
pub struct Canonical<'a>(pub(crate) &'a IrProgram);

impl<'a> Canonical<'a> {
    /// Refine `Verified` → `Canonical`, asserting the canonical-order
    /// postcondition `CanonicalizePass` establishes. A violation means
    /// `CanonicalizePass` was removed from the pipeline or a later pass reordered
    /// functions — a compiler bug: debug builds panic, release builds warn and
    /// proceed (determinism may regress; correctness does not).
    pub(crate) fn certify(verified: Verified<'a>) -> Self {
        let program = verified.0;
        if !pass_canonicalize::is_canonical(program) {
            debug_assert!(
                false,
                "Canonical::certify: program is not in canonical function order — \
                 CanonicalizePass must run as the terminal WASM pass"
            );
            eprintln!(
                "[Determinism] WASM emit reached without canonical function order — \
                 output may be host-nondeterministic"
            );
        }
        Self(program)
    }
}

pub fn codegen_with(program: &mut IrProgram, target: Target, options: &CodegenOptions) -> CodegenOutput {
    let config = target::configure(target);
    let prof = std::env::var_os("ALMIDE_PROFILE").is_some();
    // Time only through the sanctioned, wasm-safe shim. Raw std::time is
    // forbidden in this crate (it panics on wasm32-unknown-unknown, the browser
    // playground target) — see almide_base::profile and the forbidden-impurities
    // CI gate.
    let pt = almide_base::profile::ProfileTimer::start(prof);

    // NameResolutionTotal (completeness §1a), two steps while declarations
    // are still in canonical (pre-mangle) state:
    // 1. repair — rewrite unambiguous bare type references (lambda params,
    //    fold-lowered locals, alias-qualified annotations, …) to their
    //    canonical qualified name, the state producers were supposed to pin
    //    (#433/#681 family);
    // 2. gate — refuse whatever is still bare-with-qualified-decl (genuinely
    //    ambiguous), turning the silent E0425 / wasm-trap class into a
    //    structured compiler-bug report.
    verify_names::repair_bare_type_names(program);
    verify_names::assert_names_resolvable(program);

    // Layer 2: Run Nanopass pipeline (semantic rewrites — takes ownership, returns modified)
    let owned = std::mem::take(program);
    let transformed = config.pipeline.run(owned, target);
    *program = transformed;
    if let Some(pt) = &pt { eprintln!("[prof:codegen] pipeline={:.3}s", pt.elapsed_secs()); }

    // HARD type-completeness gate (both targets, both debug and release).
    // After the full pipeline, every reachable IrExpr.ty must be concrete —
    // no Unknown/TypeVar and no value-position Never. A residual is a compiler
    // bug: WASM emit would silently fall back to i32 (`ty_to_valtype`'s catch-all,
    // the `fan.map` silent-miscompile class) and Rust emit to an arbitrary type.
    // `assert_types_concretized` refuses to emit and aborts with a clean,
    // span-tagged diagnostic (a controlled error, not an ICE). Stage-1(iv) of
    // the correctness completeness roadmap.
    pass_concretize_types::assert_types_concretized(program);

    let et = almide_base::profile::ProfileTimer::start(prof);

    // Layer 3: Target-specific emit
    match target {
        Target::Wasm => {
            // AlmidePerceusBelt: a program reaches WASM emit only after passing
            // two type-state gates, in this order:
            //   Verified  — Perceus RC balance (Lean 4-certified check)
            //   Canonical — functions are in canonical emit order, so the wasm32
            //               (browser) compiler can't diverge from native.
            // `CanonicalizePass` (terminal pipeline pass) establishes the order;
            // `Canonical::certify` consumes `Verified` and asserts it.
            // #782: the v0 wasm emitter is RETIRED. The v1 trust-spine renderer
            // (almide-mir) is the only wasm path; a v1 wall is a hard error at
            // the CLI layer, never a fallback into unverified codegen.
            unreachable!(
                "Target::Wasm reached the retired v0 emitter — the CLI must route                  wasm builds through the v1 trust-spine renderer (#782)"
            );
        }
        Target::Wgsl => CodegenOutput::Source(emit_wgsl::emit(program)),
        _ => CodegenOutput::Source(emit_source(program, target, &config, options)),
    }
}


/// The fixed runtime prelude: AlmideConcat trait + impls, the almide_eq!/almide_ne!
/// macros, and the RcCow<T> COW value type with its impls.
///
/// `for_crate` toggles between two emission modes:
/// - `false` (inline): private items, plain `macro_rules!` — one self-contained main.rs.
/// - `true` (rlib): `pub` items + `#[macro_export]` so a slim user main can link this
///   as the `almide_rt` crate (`use almide_rt::*` + `#[macro_use] extern crate`).
fn rust_runtime_prelude(for_crate: bool) -> String {
    let vis = if for_crate { "pub " } else { "" };
    let macro_attr = if for_crate { "#[macro_export]\n" } else { "" };
    let mut s = String::new();
    s.push_str(&format!("{vis}trait AlmideConcat<Rhs> {{ type Output; fn concat(self, rhs: Rhs) -> Self::Output; }}\n"));
    s.push_str("impl AlmideConcat<String> for String { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }\n");
    s.push_str("impl AlmideConcat<&str> for String { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }\n");
    s.push_str("impl AlmideConcat<String> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }\n");
    s.push_str("impl AlmideConcat<&str> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }\n");
    s.push_str("impl<T: Clone> AlmideConcat<Vec<T>> for Vec<T> { type Output = Vec<T>; #[inline(always)] fn concat(self, rhs: Vec<T>) -> Vec<T> { let mut r = self; r.extend(rhs); r } }\n");
    s.push_str(&format!("{macro_attr}macro_rules! almide_eq {{ ($a:expr, $b:expr) => {{ ($a) == ($b) }}; }}\n"));
    s.push_str(&format!("{macro_attr}macro_rules! almide_ne {{ ($a:expr, $b:expr) => {{ ($a) != ($b) }}; }}\n"));
    // almide_div!/almide_mod!: total integer `/` and `%`. `checked_div`/`checked_rem`
    // return `None` for BOTH a zero divisor AND signed `MIN / -1` overflow, so one
    // arm distinguishes the two messages by the divisor (b == 0). Generic over every
    // int width (i8..i64, u*); the Call form is not const-evaluable, so rustc's
    // `deny(unconditional_panic)` never fires on a literal `10 / 0` and the diagnostic
    // is the runtime abort below — byte-identical to the §13 termination convention
    // (`Error: <msg>\n` + exit 1) and to the WASM div/mod trap.
    s.push_str(&format!("{macro_attr}macro_rules! almide_div {{ ($a:expr, $b:expr) => {{{{ let (__a, __b) = ($a, $b); match __a.checked_div(__b) {{ Some(__v) => __v, None => {{ eprintln!(\"Error: {{}}\", if __b == 0 {{ \"division by zero\" }} else {{ \"integer overflow\" }}); std::process::exit(1); }} }} }}}}; }}\n"));
    s.push_str(&format!("{macro_attr}macro_rules! almide_mod {{ ($a:expr, $b:expr) => {{{{ let (__a, __b) = ($a, $b); match __a.checked_rem(__b) {{ Some(__v) => __v, None => {{ eprintln!(\"Error: {{}}\", if __b == 0 {{ \"division by zero\" }} else {{ \"integer overflow\" }}); std::process::exit(1); }} }} }}}}; }}\n"));
    // almide_index!/almide_index_set!: bounds-checked `xs[i]` get/set that abort
    // with the UNIFIED message (`Error: index out of bounds\n` + exit 1), so a
    // native OOB index matches the wasm trap and the div/mod abort contract
    // (#554/C-072) instead of a raw Rust panic (exit 101). i64 index is range-
    // checked against len as usize; negative or >= len aborts.
    s.push_str(&format!("{macro_attr}macro_rules! almide_index {{ ($xs:expr, $i:expr) => {{{{ let (__xs, __i) = (&$xs, $i as i64); if __i < 0 || (__i as u64) >= __xs.len() as u64 {{ eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }} __xs[__i as usize].clone() }}}}; }}\n"));
    s.push_str(&format!("{macro_attr}macro_rules! almide_index_set {{ ($xs:expr, $i:expr, $v:expr) => {{{{ let __i = $i as i64; if __i < 0 || (__i as u64) >= $xs.len() as u64 {{ eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }} $xs[__i as usize] = $v; }}}}; }}\n"));
    // RcCow<T>: COW value type. Clone = Rc::clone (O(1)), mutation = Rc::make_mut (COW).
    // Inspired by Swift's value type semantics.
    s.push_str(&format!("{vis}struct RcCow<T>({vis}std::rc::Rc<T>);\n"));
    s.push_str("impl<T: std::fmt::Debug> std::fmt::Debug for RcCow<T> { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.fmt(f) } }\n");
    s.push_str("impl<T: Clone> Clone for RcCow<T> { fn clone(&self) -> Self { RcCow(std::rc::Rc::clone(&self.0)) } }\n");
    s.push_str("impl<T: PartialEq> PartialEq for RcCow<T> { fn eq(&self, other: &Self) -> bool { *self.0 == *other.0 } }\n");
    s.push_str("impl<T: PartialEq> PartialEq<T> for RcCow<T> { fn eq(&self, other: &T) -> bool { *self.0 == *other } }\n");
    s.push_str("impl PartialEq<&str> for RcCow<String> { fn eq(&self, other: &&str) -> bool { self.0.as_str() == *other } }\n");
    s.push_str("impl<T> std::ops::Deref for RcCow<T> { type Target = T; fn deref(&self) -> &T { &self.0 } }\n");
    s.push_str("impl<T: Clone> std::ops::DerefMut for RcCow<T> { fn deref_mut(&mut self) -> &mut T { std::rc::Rc::make_mut(&mut self.0) } }\n");
    s.push_str(&format!("impl<T> RcCow<T> {{ {vis}fn new(v: T) -> Self {{ RcCow(std::rc::Rc::new(v)) }} {vis}fn make_mut(&mut self) -> &mut T where T: Clone {{ std::rc::Rc::make_mut(&mut self.0) }} {vis}fn into_inner(self) -> T where T: Clone {{ std::rc::Rc::try_unwrap(self.0).unwrap_or_else(|rc| (*rc).clone()) }} }}\n"));
    s.push_str("impl<T> From<T> for RcCow<T> { fn from(v: T) -> Self { RcCow::new(v) } }\n");
    s.push_str("impl<T: std::fmt::Display> std::fmt::Display for RcCow<T> { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { self.0.fmt(f) } }\n");
    s.push_str("impl<T: std::hash::Hash> std::hash::Hash for RcCow<T> { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { self.0.hash(state) } }\n");
    // Blanket AlmideConcat: RcCow<T> + Rhs and RcCow<T> + Val<U> — 2 impls cover all combos.
    s.push_str("impl<T: Clone, Rhs> AlmideConcat<Rhs> for RcCow<T> where T: AlmideConcat<Rhs> { type Output = RcCow<<T as AlmideConcat<Rhs>>::Output>; #[inline(always)] fn concat(self, rhs: Rhs) -> Self::Output { RcCow::new((*self).clone().concat(rhs)) } }\n");
    // SharedMut<T>: shared interior-mutable cell for a non-Copy `var` captured and
    // mutated through a closure (Closure v2, P6). The non-Copy analogue of the
    // `Rc<Cell<T>>` used for Copy captures: `Clone` is `Rc::clone` (O(1), shares the
    // SAME cell) so a `move` closure's mutation is visible to the enclosing scope —
    // unlike `RcCow`, whose `make_mut` clones on a shared write and loses it. The
    // `get`/`set` API mirrors `Cell` so reads/assigns lower identically for both.
    s.push_str(&format!("{vis}struct SharedMut<T>({vis}std::rc::Rc<std::cell::RefCell<T>>);\n"));
    s.push_str("impl<T> Clone for SharedMut<T> { fn clone(&self) -> Self { SharedMut(std::rc::Rc::clone(&self.0)) } }\n");
    s.push_str("impl<T: std::fmt::Debug> std::fmt::Debug for SharedMut<T> { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.borrow().fmt(f) } }\n");
    s.push_str("impl<T: PartialEq> PartialEq for SharedMut<T> { fn eq(&self, other: &Self) -> bool { *self.0.borrow() == *other.0.borrow() } }\n");
    s.push_str(&format!("impl<T> SharedMut<T> {{ {vis}fn new(v: T) -> Self {{ SharedMut(std::rc::Rc::new(std::cell::RefCell::new(v))) }} {vis}fn get(&self) -> T where T: Clone {{ self.0.borrow().clone() }} {vis}fn set(&self, v: T) {{ *self.0.borrow_mut() = v; }} {vis}fn borrow(&self) -> std::cell::Ref<'_, T> {{ self.0.borrow() }} {vis}fn borrow_mut(&self) -> std::cell::RefMut<'_, T> {{ self.0.borrow_mut() }} }}\n"));
    s.push_str(&almide_repr_prelude(vis));
    s
}

/// The `AlmideRepr` trait + std-type impls used by compound string interpolation.
///
/// `"${compound}"` must render a value back to its **Almide literal form**
/// (`[1, 2, 3]`, `["a": 1]`, `(1, "x")`, `some(v)`, …) byte-identically on the
/// Rust and WASM targets. Each compound interpolation part is lowered (by the
/// walker) to `almide_repr(&part)`; recursion is automatic via the trait, so a
/// `List[Map[String, List[Int]]]` composes with no per-shape generated code.
///
/// String *escaping inside a container* mirrors `almide_rt_value_stringify`
/// exactly (`\\ \" \n \r \t`) so the two repr layers never diverge. A BARE
/// top-level `${s}` String stays raw — the walker only routes *compound* parts
/// here, so the quoting `impl AlmideRepr for String` is reached only from inside
/// a container.
///
/// Numeric/Bool reprs delegate to the same `Display` path as bare interpolation
/// (`format!("{}", …)`), never a second formatter, so an `Int`/`Float`/`Bool`
/// inside a container reads identically to the same value interpolated bare.
///
/// `AlmideMap` / `AlmideSet` impls live in their runtime modules (map.rs /
/// set.rs) because those types are only pulled in when `needed` — emitting the
/// impl here would reference an undefined type in std-only programs.
fn almide_repr_prelude(vis: &str) -> String {
    let mut s = String::new();
    s.push_str(&format!("{vis}trait AlmideRepr {{ fn almide_repr(&self) -> String; }}\n"));
    // Free function: the uniform call site the walker emits for a compound part.
    s.push_str(&format!("{vis}fn almide_repr<T: AlmideRepr + ?Sized>(x: &T) -> String {{ x.almide_repr() }}\n"));
    // Escape a string for container context — identical set to almide_rt_value_stringify.
    s.push_str(&format!("{vis}fn almide_repr_str(sv: &str) -> String {{ format!(\"\\\"{{}}\\\"\", sv.replace('\\\\', \"\\\\\\\\\").replace('\\\"', \"\\\\\\\"\").replace('\\n', \"\\\\n\").replace('\\r', \"\\\\r\").replace('\\t', \"\\\\t\")) }}\n"));
    // Primitives: numbers/bools route through the SAME Display path as bare interp.
    for t in ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool"] {
        s.push_str(&format!("impl AlmideRepr for {t} {{ fn almide_repr(&self) -> String {{ format!(\"{{}}\", self) }} }}\n"));
    }
    // Strings inside a container are double-quoted + escaped. The impl TARGET of
    // every std type below is FULLY QUALIFIED (`std::string::String`,
    // `std::boxed::Box`, …): a user `type Box = { … }` / `type Vec = …` lowers to
    // a `pub struct Box`/`pub struct Vec` that would otherwise SHADOW the std type
    // at the impl site, binding this blanket impl to the user struct (E0614:
    // `(**self)` on a non-pointer, or a method-not-found). Qualifying pins each
    // blanket to the real std type so the user's own type keeps its generated impl.
    s.push_str("impl AlmideRepr for std::string::String { fn almide_repr(&self) -> String { almide_repr_str(self) } }\n");
    s.push_str("impl AlmideRepr for str { fn almide_repr(&self) -> String { almide_repr_str(self) } }\n");
    // List: `[a, b, c]`, empty `[]`.
    s.push_str("impl<T: AlmideRepr> AlmideRepr for std::vec::Vec<T> { fn almide_repr(&self) -> String { let mut o = String::from(\"[\"); for (i, e) in self.iter().enumerate() { if i > 0 { o.push_str(\", \"); } o.push_str(&e.almide_repr()); } o.push(']'); o } }\n");
    // Option: `some(v)` / `none`.
    s.push_str("impl<T: AlmideRepr> AlmideRepr for std::option::Option<T> { fn almide_repr(&self) -> String { match self { Some(v) => format!(\"some({})\", v.almide_repr()), None => \"none\".to_string() } } }\n");
    // Result: `ok(v)` / `err(e)`.
    s.push_str("impl<T: AlmideRepr, E: AlmideRepr> AlmideRepr for std::result::Result<T, E> { fn almide_repr(&self) -> String { match self { Ok(v) => format!(\"ok({})\", v.almide_repr()), Err(e) => format!(\"err({})\", e.almide_repr()) } } }\n");
    // RcCow / SharedMut transparently forward to the wrapped value.
    s.push_str("impl<T: AlmideRepr> AlmideRepr for RcCow<T> { fn almide_repr(&self) -> String { (**self).almide_repr() } }\n");
    s.push_str("impl<T: AlmideRepr + Clone> AlmideRepr for SharedMut<T> { fn almide_repr(&self) -> String { self.0.borrow().almide_repr() } }\n");
    // Reference forwarders so `almide_repr(&&x)` and slice elements compose.
    s.push_str("impl<T: AlmideRepr + ?Sized> AlmideRepr for &T { fn almide_repr(&self) -> String { (**self).almide_repr() } }\n");
    s.push_str("impl<T: AlmideRepr + ?Sized> AlmideRepr for std::boxed::Box<T> { fn almide_repr(&self) -> String { (**self).almide_repr() } }\n");
    // Tuples: `(a, b, …)` for arities 2..=12 (the parser caps tuple width well below this).
    let names = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L"];
    for arity in 2..=names.len() {
        let used = &names[..arity];
        let bounds = used.iter().map(|n| format!("{n}: AlmideRepr")).collect::<Vec<_>>().join(", ");
        let tys = used.join(", ");
        // Build the body: push each `self.i.almide_repr()` separated by ", ".
        let pushes = (0..arity).map(|i| {
            let sep = if i > 0 { "o.push_str(\", \"); " } else { "" };
            format!("{sep}o.push_str(&self.{i}.almide_repr());")
        }).collect::<Vec<_>>().join(" ");
        s.push_str(&format!(
            "impl<{bounds}> AlmideRepr for ({tys},) {{ fn almide_repr(&self) -> String {{ let mut o = String::from(\"(\"); {pushes} o.push(')'); o }} }}\n"
        ));
    }
    s
}

/// Resolve inter-module runtime dependencies into `needed` (auto-extracted from
/// source by build.rs — no manual whitelist). Fixpoint over RUNTIME_DEPS.
fn resolve_runtime_deps(needed: &mut std::collections::HashSet<&str>) {
    let mut added = true;
    while added {
        added = false;
        for (module, deps) in crate::generated::rust_runtime::RUNTIME_DEPS {
            if needed.contains(module) {
                for dep in *deps {
                    if needed.insert(dep) { added = true; }
                }
            }
        }
    }
}

/// Collect the runtime module bodies for the `needed` set: hoist top-level `use`
/// to the front, deduplicate, and skip struct definitions the walker already
/// emitted (present in `user_code`) to avoid E0428. Returns the assembled block.
/// Process one runtime module's (already-test-block-stripped) source lines,
/// appending top-level `use` lines into `use_set`/`use_lines` (deduped) and
/// everything else into `body_lines` — skipping a `#[derive(...)] pub
/// struct Name { ... }` block whose struct the walker already emitted into
/// `user_code`. Extracted from `rust_runtime_modules` (cog>30
/// decomposition, second round): a write-only accumulator over
/// `use_set`/`use_lines`/`body_lines`, never read back to change its own
/// branching within this call.
fn append_runtime_module_lines(
    source: &str,
    user_code: &str,
    use_set: &mut std::collections::HashSet<String>,
    use_lines: &mut Vec<String>,
    body_lines: &mut Vec<String>,
) {
    let stripped = strip_test_blocks(source);
    let lines: Vec<&str> = stripped.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        // Top-level use: not indented and starts with "use "
        if !line.starts_with(' ') && !line.starts_with('\t')
            && trimmed.starts_with("use ") && trimmed.ends_with(';')
        {
            if use_set.insert(trimmed.to_string()) {
                use_lines.push(trimmed.to_string());
            }
            i += 1;
            continue;
        }
        // Detect struct definitions: #[derive(...)] followed by pub struct Name
        // Skip the block if user_code already contains that struct (walker emitted it).
        if trimmed.starts_with("#[derive(") {
            if let Some(next) = lines.get(i + 1) {
                if let Some(struct_name) = next.trim().strip_prefix("pub struct ")
                    .and_then(|s| s.split_whitespace().next())
                    .map(|s| s.trim_end_matches('{').trim())
                {
                    let needle = format!("struct {}", struct_name);
                    if user_code.contains(&needle) {
                        // Skip derive + struct + fields + closing brace
                        i += 1; // skip #[derive]
                        let mut depth = 0u32;
                        while i < lines.len() {
                            if lines[i].contains('{') { depth += 1; }
                            if lines[i].contains('}') {
                                depth = depth.saturating_sub(1);
                                if depth == 0 { i += 1; break; }
                            }
                            i += 1;
                        }
                        continue;
                    }
                }
            }
        }
        body_lines.push(line.to_string());
        i += 1;
    }
    body_lines.push(String::new());
}

/// Remove single-item `use a::b::X;` lines when a group `use
/// a::b::{..., X, ...};` already covers them. Extracted from
/// `rust_runtime_modules`.
fn dedup_use_lines(use_lines: Vec<String>, use_set: &std::collections::HashSet<String>) -> Vec<String> {
    use_lines.into_iter().filter(|line| {
        // Parse: use path::Item;
        if let Some(rest) = line.strip_prefix("use ").and_then(|s| s.strip_suffix(';')) {
            if !rest.contains('{') {
                if let Some(pos) = rest.rfind("::") {
                    let prefix = &rest[..pos];
                    let item = &rest[pos + 2..];
                    // Check if any group import covers this item
                    let dominated = use_set.iter().any(|other| {
                        if let Some(orest) = other.strip_prefix("use ").and_then(|s| s.strip_suffix(';')) {
                            if let Some(opos) = orest.find("::{") {
                                let oprefix = &orest[..opos];
                                if oprefix == prefix {
                                    let items_str = &orest[opos + 3..orest.len() - 1]; // strip ::{ and }
                                    return items_str.split(',').any(|i| i.trim() == item);
                                }
                            }
                        }
                        false
                    });
                    if dominated { return false; }
                }
            }
        }
        true
    }).collect()
}

fn rust_runtime_modules(needed: &std::collections::HashSet<&str>, user_code: &str) -> String {
    let mut use_set = std::collections::HashSet::new();
    let mut use_lines = Vec::new();
    let mut body_lines = Vec::new();
    for (name, source) in crate::generated::rust_runtime::RUST_RUNTIME_MODULES {
        if needed.contains(name) {
            append_runtime_module_lines(source, user_code, &mut use_set, &mut use_lines, &mut body_lines);
        }
    }
    let use_lines = dedup_use_lines(use_lines, &use_set);
    let mut out = String::new();
    for u in &use_lines { out.push_str(u); out.push('\n'); }
    for line in &body_lines { out.push_str(line); out.push('\n'); }
    out
}

/// Runtime modules that cannot live in the bare-rustc `almide_rt` rlib: `http`
/// needs rustls, `zlib` needs flate2, and `sse` calls into `http` for its
/// streaming transport. Programs using these stay on the cargo path; the rlib
/// fast path only covers std-only programs (a link error otherwise falls back).
pub const NON_STD_RUNTIME_MODULES: &[&str] = &["http", "zlib", "sse"];

/// Emit the full `almide_rt` runtime crate source: the prelude (pub items +
/// exported macros) plus every std-only runtime module. Built once into an
/// `.rlib` and linked by slim user mains (see `CodegenOptions::external_runtime`).
///
/// Deterministic — depends only on the embedded runtime sources and the compiler
/// version, so callers can cache the resulting rlib keyed by a hash of this output.
pub fn emit_runtime_crate() -> String {
    let mut out = String::new();
    out.push_str("#![allow(unused_parens, unused_variables, dead_code, unused_imports, unused_mut, unused_must_use, non_snake_case)]\n\n");
    out.push_str("use std::collections::HashMap;\nuse std::collections::HashSet;\n\n");
    out.push_str(&rust_runtime_prelude(true));
    // Every std-only runtime module (exclude external-dep modules).
    let mut needed: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (name, _) in crate::generated::rust_runtime::RUST_RUNTIME_MODULES {
        if !NON_STD_RUNTIME_MODULES.contains(name) {
            needed.insert(*name);
        }
    }
    out.push_str(&rust_runtime_modules(&needed, ""));
    // matrix.rs calls `almide_kernel::…`; embed the kernel as a crate-local module so
    // the single-crate runtime rlib resolves it (no extern). Always present here — the
    // shared rlib includes every std module, matrix among them.
    if needed.contains("matrix") {
        out.push('\n');
        out.push_str(crate::generated::rust_runtime::ALMIDE_KERNEL_INLINE);
        out.push('\n');
    }
    out
}

/// Emit source code for text targets (Rust, TypeScript, JavaScript).
fn emit_source(program: &mut IrProgram, target: Target, config: &target::TargetConfig, options: &CodegenOptions) -> String {
    // Template-driven rendering (walker reads annotations, never checks types)
    let ann = std::mem::take(&mut program.codegen_annotations);
    let mut ctx = walker::RenderContext::new(&config.templates, &program.var_table)
        .with_target(target)
        .with_annotations(ann);
    ctx.repr_c = options.repr_c;
    let user_code = walker::render_program(&ctx, program);

    // Prepend runtime preamble
    let mut output = String::new();
    match target {
        Target::Rust => {
            output.push_str("#![allow(unused_parens, unused_variables, dead_code, unused_imports, unused_mut, unused_must_use)]\n\n");
            output.push_str("use std::collections::HashMap;\nuse std::collections::HashSet;\n\n");
            output.push_str(&rust_runtime_prelude(false));
            // Include runtime modules referenced in the IR. The IrProgram tracks
            // which stdlib modules are used across all functions and transitive
            // dependencies — no text search.
            let mut needed: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for m in &program.used_stdlib_modules {
                needed.insert(m.as_str());
            }
            // A few operators lower to a runtime call (not a CallTarget::Module),
            // so the IR's used-module set misses them — e.g. float `**` renders
            // `almide_rt_math_fpow(..)` via the power_expr template. Union in any
            // module whose `almide_rt_<module>_` symbol literally appears in the
            // emitted user code so the body (and its transitive deps) is included.
            for (name, _) in crate::generated::rust_runtime::RUST_RUNTIME_MODULES {
                if !needed.contains(name)
                    && user_code.contains(&format!("almide_rt_{}_", name))
                {
                    needed.insert(name);
                }
            }
            resolve_runtime_deps(&mut needed);
            output.push_str(&rust_runtime_modules(&needed, &user_code));
            // matrix.rs calls `almide_kernel::…`; when matrix is included, drop the
            // embedded kernel in beside it — above the boundary, so it stays in the
            // runtime preamble (the rlib split and the inline build both keep it).
            if needed.contains("matrix") {
                output.push('\n');
                output.push_str(crate::generated::rust_runtime::ALMIDE_KERNEL_INLINE);
                output.push('\n');
            }
            // Boundary marker: everything above is the inlined runtime preamble,
            // everything below is user code. The rlib fast path (see CLI) splits
            // here to swap the preamble for `extern crate almide_rt`.
            output.push_str(RT_BOUNDARY_MARKER);
            output.push('\n');
        }
        _ => {}
    }
    output.push_str(&user_code);
    output
}

/// Collect the set of stdlib module names actually used by the program.
/// Scans CallTarget::Module references in all functions, top_lets, and modules.
/// Also resolves inter-module runtime dependencies (e.g., json → value).
fn collect_used_modules(program: &IrProgram) -> std::collections::HashSet<String> {
    let mut used = std::collections::HashSet::new();
    // Explicit module imports
    for m in &program.modules {
        used.insert(m.name.to_string());
    }
    // Scan all expressions for CallTarget::Module references
    for func in &program.functions {
        scan_expr_modules(&func.body, &mut used);
    }
    for tl in &program.top_lets {
        scan_expr_modules(&tl.value, &mut used);
    }
    for module in &program.modules {
        for func in &module.functions {
            scan_expr_modules(&func.body, &mut used);
        }
        for tl in &module.top_lets {
            scan_expr_modules(&tl.value, &mut used);
        }
    }
    // Resolve runtime dependencies (module A's runtime code references module B's functions)
    let deps: &[(&str, &[&str])] = &[
        ("json", &["value"]),
    ];
    let mut added = true;
    while added {
        added = false;
        for (module, requires) in deps {
            if used.contains(*module) {
                for req in *requires {
                    if used.insert(req.to_string()) {
                        added = true;
                    }
                }
            }
        }
    }
    used
}

/// Scan an expression tree for `CallTarget::Module` references, recording
/// every module name touched. Rewritten (cog>30 decomposition) onto the
/// shared `almide_ir::visit::IrVisitor`/`walk_expr` exhaustive traversal —
/// the same infrastructure `contains_aborting_int_div` uses elsewhere in
/// this crate — instead of a 90-line hand-rolled recursive match. `used`
/// is a write-only accumulator. `scan_stmt_modules` (the old hand-rolled
/// statement-level counterpart) is gone: `IrVisitor`'s default
/// `visit_stmt` → `walk_stmt` already routes every statement's
/// sub-expressions back through `visit_expr` below.
fn scan_expr_modules(expr: &IrExpr, used: &mut std::collections::HashSet<String>) {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct ModuleScanner<'a> { used: &'a mut std::collections::HashSet<String> }
    impl IrVisitor for ModuleScanner<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Module { module, .. }, .. } = &expr.kind {
                self.used.insert(module.to_string());
            }
            walk_expr(self, expr);
        }
    }
    ModuleScanner { used }.visit_expr(expr);
}
