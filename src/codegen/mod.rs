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
pub mod pass;
pub mod pass_auto_parallel;
pub mod pass_box_deref;
pub mod pass_builtin_lowering;
pub mod pass_capture_clone;
pub mod pass_clone;
pub mod pass_fan_lowering;
pub mod pass_match_lowering;
pub mod pass_match_subject;
pub mod pass_result_erasure;
pub mod pass_result_propagation;
pub mod pass_shadow_resolve;
pub mod pass_stdlib_lowering;
pub mod pass_effect_inference;
pub mod pass_stream_fusion;
pub mod pass_tco;
pub mod pass_licm;
pub mod pass_peephole;
pub mod template;
pub mod target;
pub mod walker;
pub mod emit_wasm;

use crate::ir::*;
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

pub fn codegen_with(program: &mut IrProgram, target: Target, options: &CodegenOptions) -> CodegenOutput {
    let config = target::configure(target);

    // Layer 2: Run Nanopass pipeline (semantic rewrites — takes ownership, returns modified)
    let owned = std::mem::take(program);
    let transformed = config.pipeline.run(owned, target);
    *program = transformed;

    // Layer 3: Target-specific emit
    match target {
        Target::Wasm => CodegenOutput::Binary(emit_wasm::emit(program)),
        _ => CodegenOutput::Source(emit_source(program, target, &config, options)),
    }
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
            output.push_str("use std::collections::HashMap;\nuse std::collections::HashSet;\n");
            output.push_str("trait AlmideConcat<Rhs> { type Output; fn concat(self, rhs: Rhs) -> Self::Output; }\n");
            output.push_str("impl AlmideConcat<String> for String { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }\n");
            output.push_str("impl AlmideConcat<&str> for String { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }\n");
            output.push_str("impl AlmideConcat<String> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }\n");
            output.push_str("impl AlmideConcat<&str> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }\n");
            output.push_str("impl<T: Clone> AlmideConcat<Vec<T>> for Vec<T> { type Output = Vec<T>; #[inline(always)] fn concat(self, rhs: Vec<T>) -> Vec<T> { let mut r = self; r.extend(rhs); r } }\n");
            output.push_str("macro_rules! almide_eq { ($a:expr, $b:expr) => { ($a) == ($b) }; }\n");
            output.push_str("macro_rules! almide_ne { ($a:expr, $b:expr) => { ($a) != ($b) }; }\n");
            // Include only runtime modules referenced by the user code.
            // Scan user_code for `almide_rt_<module>_` or `almide_json_` patterns.
            let mut needed: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for (name, _) in crate::generated::rust_runtime::RUST_RUNTIME_MODULES {
                let prefix = format!("almide_rt_{}_", name);
                let alt_prefix = format!("almide_{}_", name);
                if user_code.contains(&prefix) || user_code.contains(&alt_prefix) {
                    needed.insert(name);
                }
            }
            // Also check for direct type references (Value, AlmideJsonPath, etc.)
            if user_code.contains("Value::") || user_code.contains(": Value") || user_code.contains("<Value") {
                needed.insert("value");
            }
            if user_code.contains("AlmideJsonPath") || user_code.contains("JsonPath") {
                needed.insert("json");
            }
            // Runtime dependency: json depends on value
            if needed.contains("json") { needed.insert("value"); }
            for (name, source) in crate::generated::rust_runtime::RUST_RUNTIME_MODULES {
                if needed.contains(name) {
                    output.push_str(&strip_test_blocks(source));
                    output.push('\n');
                }
            }
            output.push('\n');
        }
        Target::TypeScript => {
            output.push_str("// TypeScript target removed — use --target wasm for JS runtimes\n");
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
