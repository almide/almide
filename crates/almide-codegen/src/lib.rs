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
pub mod pass_auto_parallel;
pub mod pass_borrow_inference;
pub mod pass_box_deref;
pub mod pass_builtin_lowering;
pub mod pass_capture_clone;
pub mod pass_clone;
pub mod pass_fan_lowering;
pub mod pass_list_pattern;
pub mod pass_match_lowering;
pub mod pass_match_subject;
pub mod pass_result_erasure;
pub mod pass_result_propagation;
pub mod pass_shadow_resolve;
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
pub mod emit_wasm;
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

/// AlmidePerceusBelt: type-state verified program.
///
/// Construction requires passing Perceus RC verification (Lean 4 certified).
/// WASM emit accepts only Verified — unverified programs cannot reach emission.
/// This is the type-level enforcement of RC correctness, analogous to how
/// Rust's borrow checker prevents use-after-free at the type level.
pub struct Verified<'a>(pub(crate) &'a IrProgram);

impl<'a> Verified<'a> {
    /// Verify Perceus RC balance and construct Verified gate.
    /// Violations are reported as warnings. The program still compiles
    /// (Perceus violations cause leaks, not unsoundness), but the
    /// verification result is tracked for future hard-error mode.
    fn verify(program: &'a IrProgram) -> Self {
        let mut total_violations = 0usize;
        for func in &program.functions {
            if func.is_test { continue; }
            let violations = pass_perceus::perceus_verify_function(func, &program.var_table);
            total_violations += violations;
        }
        if total_violations > 0 {
            eprintln!("[Perceus] {total_violations} RC violation(s) — WASM may leak memory");
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

    // Layer 2: Run Nanopass pipeline (semantic rewrites — takes ownership, returns modified)
    let owned = std::mem::take(program);
    let transformed = config.pipeline.run(owned, target);
    *program = transformed;
    if let Some(pt) = &pt { eprintln!("[prof:codegen] pipeline={:.3}s", pt.elapsed_secs()); }
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
            let verified = Verified::verify(program);
            let canonical = Canonical::certify(verified);
            let out = emit_wasm::emit_certified(canonical);
            if let Some(et) = &et { eprintln!("[prof:codegen] wasm_emit={:.3}s", et.elapsed_secs()); }
            CodegenOutput::Binary(out)
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
fn rust_runtime_modules(needed: &std::collections::HashSet<&str>, user_code: &str) -> String {
    let mut use_set = std::collections::HashSet::new();
    let mut use_lines = Vec::new();
    let mut body_lines = Vec::new();
    for (name, source) in crate::generated::rust_runtime::RUST_RUNTIME_MODULES {
        if needed.contains(name) {
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
    }
    // Remove single-item `use a::b::X;` when a group `use a::b::{..., X, ...};` exists
    let use_lines: Vec<String> = use_lines.into_iter().filter(|line| {
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
    }).collect();
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
            resolve_runtime_deps(&mut needed);
            output.push_str(&rust_runtime_modules(&needed, &user_code));
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

fn scan_expr_modules(expr: &IrExpr, used: &mut std::collections::HashSet<String>) {
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            if let CallTarget::Module { module, .. } = target {
                used.insert(module.to_string());
            }
            if let CallTarget::Method { object, .. } = target {
                scan_expr_modules(object, used);
            }
            for a in args { scan_expr_modules(a, used); }
        }
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { scan_stmt_modules(s, used); }
            if let Some(e) = tail { scan_expr_modules(e, used); }
        }
        IrExprKind::If { cond, then, else_ } => {
            scan_expr_modules(cond, used);
            scan_expr_modules(then, used);
            scan_expr_modules(else_, used);
        }
        IrExprKind::Lambda { body, .. } => scan_expr_modules(body, used),
        IrExprKind::BinOp { left, right, .. } => {
            scan_expr_modules(left, used);
            scan_expr_modules(right, used);
        }
        IrExprKind::UnOp { operand, .. } => scan_expr_modules(operand, used),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
            for e in elements { scan_expr_modules(e, used); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            scan_expr_modules(iterable, used);
            for s in body { scan_stmt_modules(s, used); }
        }
        IrExprKind::While { cond, body } => {
            scan_expr_modules(cond, used);
            for s in body { scan_stmt_modules(s, used); }
        }
        IrExprKind::Match { subject, arms } => {
            scan_expr_modules(subject, used);
            for arm in arms {
                scan_expr_modules(&arm.body, used);
                if let Some(g) = &arm.guard { scan_expr_modules(g, used); }
            }
        }
        IrExprKind::Member { object, .. } | IrExprKind::OptionalChain { expr: object, .. } => {
            scan_expr_modules(object, used);
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { scan_expr_modules(v, used); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            scan_expr_modules(base, used);
            for (_, v) in fields { scan_expr_modules(v, used); }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p { scan_expr_modules(expr, used); }
            }
        }
        IrExprKind::ResultOk { expr: inner } | IrExprKind::ResultErr { expr: inner }
        | IrExprKind::OptionSome { expr: inner } | IrExprKind::Try { expr: inner }
        | IrExprKind::Unwrap { expr: inner } | IrExprKind::ToOption { expr: inner } => {
            scan_expr_modules(inner, used);
        }
        IrExprKind::UnwrapOr { expr: inner, fallback } => {
            scan_expr_modules(inner, used);
            scan_expr_modules(fallback, used);
        }
        IrExprKind::IndexAccess { object, index } => {
            scan_expr_modules(object, used);
            scan_expr_modules(index, used);
        }
        IrExprKind::MapAccess { object, key } => {
            scan_expr_modules(object, used);
            scan_expr_modules(key, used);
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { scan_expr_modules(k, used); scan_expr_modules(v, used); }
        }
        IrExprKind::Range { start, end, .. } => {
            scan_expr_modules(start, used);
            scan_expr_modules(end, used);
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { scan_expr_modules(a, used); }
        }
        IrExprKind::TupleIndex { object, .. } => scan_expr_modules(object, used),
        _ => {}
    }
}

fn scan_stmt_modules(stmt: &IrStmt, used: &mut std::collections::HashSet<String>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => scan_expr_modules(value, used),
        IrStmtKind::Assign { value, .. } => scan_expr_modules(value, used),
        IrStmtKind::Expr { expr } => scan_expr_modules(expr, used),
        IrStmtKind::Guard { cond, else_ } => {
            scan_expr_modules(cond, used);
            scan_expr_modules(else_, used);
        }
        IrStmtKind::BindDestructure { value, .. } => scan_expr_modules(value, used),
        _ => {}
    }
}
