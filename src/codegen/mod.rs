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
pub mod pass_box_deref;
pub mod pass_builtin_lowering;
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
pub mod template;
pub mod target;
pub mod walker;
pub mod emit_wasm;

use crate::ir::IrProgram;
use pass::Target;

/// Codegen output: source code for text targets, binary for WASM.
pub enum CodegenOutput {
    Source(String),
    Binary(Vec<u8>),
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
    let config = target::configure(target);

    // Layer 2: Run Nanopass pipeline (semantic rewrites — modifies IR)
    config.pipeline.run(program, target);

    // Layer 3: Target-specific emit
    match target {
        Target::Wasm => CodegenOutput::Binary(emit_wasm::emit(program)),
        _ => CodegenOutput::Source(emit_source(program, target, &config)),
    }
}

/// Emit source code for text targets (Rust, TypeScript, JavaScript).
fn emit_source(program: &mut IrProgram, target: Target, config: &target::TargetConfig) -> String {
    // Template-driven rendering (walker reads annotations, never checks types)
    let ann = std::mem::take(&mut program.codegen_annotations);
    let ctx = walker::RenderContext::new(&config.templates, &program.var_table)
        .with_target(target)
        .with_annotations(ann);
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
            for (_name, source) in crate::generated::rust_runtime::RUST_RUNTIME_MODULES {
                output.push_str(&strip_test_blocks(source));
                output.push('\n');
            }
            output.push('\n');
        }
        Target::TypeScript => {
            output.push_str(&crate::emit_ts_runtime::full_runtime());
            output.push('\n');
        }
        _ => {}
    }
    output.push_str(&user_code);
    output
}
