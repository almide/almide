//! Codegen annotations: decisions made by passes, consumed by walker.
//!
//! This separates "what to do" (passes) from "how to format" (walker).
//! The walker never checks types or context — it only reads annotations.

use std::collections::{HashSet, HashMap};
use crate::ir::VarId;

/// Annotations populated by Nanopass passes, read by the walker.
#[derive(Debug, Clone, Default)]
pub struct CodegenAnnotations {
    /// Variables that need .clone() when used (Rust only)
    pub clone_vars: HashSet<VarId>,

    /// Variables that need *deref when used (Box'd pattern bindings)
    pub deref_vars: HashSet<VarId>,

    /// Variables that are top-level lazy (need *DEREF in Rust)
    pub lazy_vars: HashSet<VarId>,

    /// Enum constructor → parent enum name (Red → Color)
    pub ctor_to_enum: HashMap<String, String>,

    /// Anonymous record field names → struct name
    pub anon_records: HashMap<Vec<String>, String>,

    /// Named record field names → type name
    pub named_records: HashMap<Vec<String>, String>,

    /// Set of enum names that have recursive variants (for Box detection)
    pub recursive_enums: HashSet<String>,

    /// (constructor_name, field_name) pairs where the field needs Box::new() wrapping
    /// because the field's declared type recursively references the parent enum
    pub boxed_fields: HashSet<(String, String)>,

    /// Default field values for constructors/records: (ctor_name, field_name) → default IR expr
    pub default_fields: HashMap<(String, String), crate::ir::IrExpr>,
}
