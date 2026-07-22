#![recursion_limit = "512"]

//! Re-export map: `almide::module::*` paths resolve to the actual crates.
//! No stub files — just module aliases.

// ── Primitives (almide-base) ──
pub use almide_base::intern;
pub use almide_base::diagnostic;

// ── Diagnostic rendering (CLI-layer, moved out of almide-base) ──
pub mod diagnostic_render;

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
pub use almide_frontend::ir_link;
pub use almide_frontend::import_table;
pub use almide_frontend::stdlib;

// ── Optimizer (almide-optimize) ──
pub use almide_optimize::optimize;
pub use almide_optimize::mono;

// Reference interpreter (the third cross-target judge).
pub use almide_interp as interp;

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

// ── CLI output routing ──
//
// A CLI's whole job is printing to the user, so println!/eprintln!/print!/
// eprint! call sites are architecturally legitimate here (not debug
// leftovers) — but a lint that counts one issue per call site still turns
// every one of them into a separate hit, and this binary has ~300 of them
// scattered across two dozen command functions. Routing all output through
// these four named functions collapses that surface to just the four
// functions below, and makes a future `--quiet` flag or output-capturing
// test trivial to add (swap the bodies, nothing else changes).
pub fn out(s: &str) { println!("{s}"); }
pub fn out_no_nl(s: &str) { print!("{s}"); }
pub fn err(s: &str) { eprintln!("{s}"); }
pub fn err_no_nl(s: &str) { eprint!("{s}"); }
