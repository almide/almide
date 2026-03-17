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

pub mod pass;
pub mod template;
pub mod target;
pub mod walker;

use crate::ir::IrProgram;
use pass::Target;

/// Full codegen v3 pipeline: IR → Nanopass → Walker → source code.
///
/// ```text
/// IrProgram → [Pass 1] → [Pass 2] → ... → [Pass N] → Walker → String
/// ```
pub fn emit(program: &mut IrProgram, target: Target) -> String {
    let config = target::configure(target);

    // Layer 2: Run Nanopass pipeline (semantic rewrites)
    config.pipeline.run(program, target);

    // Layer 3: Template-driven rendering
    let ctx = walker::RenderContext::new(&config.templates, &program.var_table);
    walker::render_program(&ctx, program)
}
