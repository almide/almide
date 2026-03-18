//! Codegen v3: Three-layer architecture
//!
//! ```text
//! IrProgram (typed IR)
//!     ↓
//! Layer 1: Core IR normalization (target-agnostic)
//!     ↓
//! Layer 2: Semantic Rewrite (target-specific Nanopass pipeline)
//!     ↓
//! Layer 3: Template Renderer (TOML-driven syntax output)
//!     ↓
//! Target source code (Rust / TypeScript / Go / Python)
//! ```
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
pub mod pass_match_lowering;
pub mod pass_result_propagation;
pub mod pass_stdlib_lowering;
pub mod template;
pub mod target;
pub mod walker;

use crate::ir::IrProgram;
use pass::Target;

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

/// Full codegen v3 pipeline: IR → Nanopass → Annotations → Walker → source code.
pub fn emit(program: &mut IrProgram, target: Target) -> String {
    let config = target::configure(target);

    // Layer 2: Run Nanopass pipeline (semantic rewrites — modifies IR)
    config.pipeline.run(program, target);

    // Build annotations (pass decisions as data — walker reads these)
    let mut ann = annotations::CodegenAnnotations::default();
    if target == Target::Rust {
        ann.clone_vars = pass_clone::collect_clone_vars(program);
        let (deref, recursive) = pass_box_deref::collect_deref_vars(program);
        ann.deref_vars = deref;
        ann.recursive_enums = recursive;
    }

    // Layer 3: Template-driven rendering (walker reads annotations, never checks types)
    let ctx = walker::RenderContext::new(&config.templates, &program.var_table)
        .with_target(target)
        .with_annotations(ann);
    let user_code = walker::render_program(&ctx, program);

    // Prepend runtime preamble
    let mut output = String::new();
    match target {
        Target::Rust => {
            output.push_str("#![allow(unused_parens, unused_variables, dead_code, unused_imports, unused_mut, unused_must_use)]\n\n");
            output.push_str("use std::collections::HashMap;\n");
            // Core traits and macros (same as lower_rust.rs)
            output.push_str("trait AlmideConcat<Rhs> { type Output; fn concat(self, rhs: Rhs) -> Self::Output; }\n");
            output.push_str("impl AlmideConcat<String> for String { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }\n");
            output.push_str("impl AlmideConcat<&str> for String { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }\n");
            output.push_str("impl AlmideConcat<String> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }\n");
            output.push_str("impl AlmideConcat<&str> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }\n");
            output.push_str("impl<T: Clone> AlmideConcat<Vec<T>> for Vec<T> { type Output = Vec<T>; #[inline(always)] fn concat(self, rhs: Vec<T>) -> Vec<T> { let mut r = self; r.extend(rhs); r } }\n");
            output.push_str("macro_rules! almide_eq { ($a:expr, $b:expr) => { ($a) == ($b) }; }\n");
            output.push_str("macro_rules! almide_ne { ($a:expr, $b:expr) => { ($a) != ($b) }; }\n");
            // Embed the full Rust runtime (stdlib functions), strip test blocks
            for (_name, source) in crate::generated::rust_runtime::RUST_RUNTIME_MODULES {
                output.push_str(&strip_test_blocks(source));
                output.push('\n');
            }
            output.push('\n');
        }
        Target::TypeScript => {
            // Embed the full TS runtime (Deno mode)
            output.push_str(&crate::emit_ts_runtime::full_runtime(false));
            output.push('\n');
        }
        _ => {}
    }
    output.push_str(&user_code);
    output
}
