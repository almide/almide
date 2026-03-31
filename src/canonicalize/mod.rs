//! Canonicalize: name resolution and declaration registration.
//!
//! Extracts import resolution and declaration registration from the type checker
//! into a standalone pre-pass. The pipeline becomes:
//!
//! ```text
//! Parser → AST → Canonicalize (this module) → Checker (inference only) → Lowering → IR
//! ```

pub mod resolve;
