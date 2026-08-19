#![recursion_limit = "512"]

//! Root FACADE (unit 4 boundary adaptation, PORTLOG.md): the incumbent's
//! src/lib.rs re-export map (almide@a877d2138), trimmed to the crates ported
//! so far. `resolve.rs` and `project.rs` are verbatim ports; every `crate::…`
//! path they use resolves here exactly as it did in the incumbent root crate.
//! Codegen / optimizer / interp / tools re-exports return with their units.

pub use almide_base::diagnostic;
pub use almide_base::intern;

// In the incumbent this is `pub mod diagnostic_render` (src/diagnostic_render.rs);
// greenfield ported that file as almide-diag's render module (unit 1).
pub use almide_diag::render as diagnostic_render;

pub use almide_lang::ast;
pub use almide_lang::lexer;
pub use almide_lang::parser;
pub use almide_lang::stdlib_info;

pub use almide_ir as ir;

pub use almide_frontend::canonicalize;
pub use almide_frontend::check;
pub use almide_frontend::import_table;
pub use almide_frontend::ir_link;
pub use almide_frontend::lower;
pub use almide_frontend::stdlib;

pub use almide_optimize::mono;
pub use almide_optimize::optimize;

// Reference interpreter (the third cross-target judge).
pub use almide_interp as interp;

pub mod types {
    pub use almide_frontend::types::{TypeEnv, TypeMap};
    pub use almide_lang::types::*;
}

pub mod project;
pub mod resolve;

// CLI output routing, verbatim (resolve/project report through these).
pub fn out(s: &str) { println!("{s}"); }
pub fn out_no_nl(s: &str) { print!("{s}"); }
pub fn err(s: &str) { eprintln!("{s}"); }
pub fn err_no_nl(s: &str) { eprint!("{s}"); }
