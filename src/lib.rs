#![recursion_limit = "512"]

//! Re-export map: `almide::module::*` paths resolve to the actual crates.
//! No stub files — just module aliases.

// ── Primitives (almide-base) ──
pub use almide_base::intern;
pub use almide_base::diagnostic;

// ── Language (almide-lang) ──
pub use almide_lang::ast;
pub use almide_lang::lexer;
pub use almide_lang::parser;

// ── IR (almide-ir) ──
pub use almide_ir as ir;

// ── Codegen (almide-codegen) ──
pub use almide_codegen as codegen;

// ── Frontend (almide-frontend) ──
pub use almide_frontend::check;
pub use almide_frontend::canonicalize;
pub use almide_frontend::lower;
pub use almide_frontend::import_table;
pub use almide_frontend::stdlib;

// ── Optimizer (almide-optimize) ──
pub use almide_optimize::optimize;
pub use almide_optimize::mono;

// ── Tools (almide-tools) ──
pub use almide_tools::fmt;
pub use almide_tools::interface;
pub use almide_tools::almdi;

// ── Types (composite: almide-lang types + frontend TypeEnv + TypeMap) ──
pub mod types {
    pub use almide_lang::types::*;
    pub use almide_frontend::types::{TypeEnv, TypeMap};
}

// ── CLI-only modules (remain in main crate) ──
pub mod project;
pub mod project_fetch;
pub mod resolve;
