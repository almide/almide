use std::collections::{HashSet, HashMap};
use crate::{VarId, IrExpr};

/// Annotations populated by Nanopass passes, read by the walker.
#[derive(Debug, Clone, Default)]
pub struct CodegenAnnotations {
    pub lazy_vars: HashSet<VarId>,
    pub ctor_to_enum: HashMap<String, String>,
    pub anon_records: HashMap<Vec<String>, String>,
    pub named_records: HashMap<Vec<String>, String>,
    /// Field count of each nominal record type (name → number of fields).
    /// Used to decide whether a record destructure pattern needs a trailing
    /// `..` to cover fields the user didn't name.
    pub record_field_counts: HashMap<String, usize>,
    pub recursive_enums: HashSet<String>,
    pub boxed_fields: HashSet<(String, String)>,
    pub default_fields: HashMap<(String, String), IrExpr>,
    /// User-defined record/enum names whose generated Rust struct cannot
    /// derive `PartialEq` (a field transitively blocks equality — e.g.
    /// contains a Matrix or a function pointer).
    pub eq_blocked_types: HashSet<String>,
}
