use std::collections::{HashSet, HashMap};
use crate::{VarId, IrExpr};

/// How a variable is stored at the Rust codegen level.
///
/// Each mutable binding falls into one of four categories, determined once
/// during program rendering and looked up at every read/write site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarStorage {
    /// Plain local variable (immutable let or mutable Copy-type var).
    Local,
    /// Local `var` of non-Copy type → `RcCow<T>` (Swift-style COW).
    RcCow,
    /// Module-level `var` of Copy type → `thread_local! { Cell<T> }`.
    ModuleCell,
    /// Module-level `var` of non-Copy type → `thread_local! { RefCell<Rc<T>> }`.
    ModuleRc,
}

/// Annotations populated by Nanopass passes, read by the walker.
#[derive(Debug, Clone, Default)]
pub struct CodegenAnnotations {
    pub lazy_vars: HashSet<VarId>,
    /// Uppercased names of module-level `top_lets` whose kind is `Lazy`
    /// (e.g. `ALMIDE_RT_UTIL_CATEGORY_ORDER`). Synthetic cross-module
    /// Vars reference these by name but carry a fresh VarId, so
    /// `lazy_vars` misses them. The walker checks this set before
    /// emitting `(*NAME)` — scalar `Const` top_lets (plain `const
    /// NAME: i64 = 42;`) must NOT be dereferenced.
    pub lazy_top_let_names: HashSet<String>,
    /// Unified variable storage classification.
    /// Keyed by VarId for local vars (RcCow) and module vars with known ids.
    pub var_storage: HashMap<VarId, VarStorage>,
    /// Keyed by uppercased name for cross-module synthetic refs
    /// (ALMIDE_RT_<MOD>_<NAME>) that carry fresh VarIds.
    pub var_storage_by_name: HashMap<String, VarStorage>,
    pub ctor_to_enum: HashMap<String, String>,
    pub anon_records: HashMap<Vec<String>, String>,
    /// Anon-record keys (sorted field names) whose struct has a closure (`Fn`)
    /// field — its generated struct derives `Clone` only (a closure is not
    /// `Debug`/`PartialEq`), like a `type`-declared record's `has_fn_fields` path.
    pub anon_records_with_fn: std::collections::HashSet<Vec<String>>,
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
    /// `Mutability::Var` locals that are captured-and-mutated by a closure. On the
    /// Rust target these are lowered to a shared `Rc<Cell<T>>` / `Rc<RefCell<T>>`
    /// cell (declaration, every read/write, and the closure capture go through the
    /// cell) so a mutation inside the closure is observed by the enclosing scope —
    /// a plain `move` closure would capture a *copy* and silently drop the mutation.
    /// (Closure v2, P3.)
    pub shared_mut_vars: HashSet<VarId>,
}

impl CodegenAnnotations {
    /// Look up the storage mode for a variable. Checks VarId first (precise),
    /// then falls back to uppercased name for cross-module synthetic refs only.
    ///
    /// A genuine global is always referenced by its real VarId (registered at
    /// classification time), so the VarId lookup hits. The by-name fallback is a
    /// safety net for cross-module synthetic refs, whose names are
    /// `ALMIDE_RT_`-prefixed and therefore collision-free. It must NOT match a
    /// bare name like `N`: a local that merely shares the name with a user global
    /// (e.g. a stdlib fn's `n` parameter when the program has a global `n`) would
    /// otherwise be misclassified as the global and read through its thread_local.
    pub fn get_var_storage(&self, var: &VarId, name: &str) -> VarStorage {
        if let Some(s) = self.var_storage.get(var) {
            return *s;
        }
        let upper = name.to_uppercase();
        if upper.starts_with("ALMIDE_RT_") {
            if let Some(s) = self.var_storage_by_name.get(&upper) {
                return *s;
            }
        }
        VarStorage::Local
    }

    pub fn is_rc_cow(&self, var: &VarId) -> bool {
        matches!(self.var_storage.get(var), Some(VarStorage::RcCow))
    }

    /// True if `var` is a closure-captured mutable local lowered to a shared cell
    /// (`Rc<Cell<T>>`/`Rc<RefCell<T>>`) on the Rust target. (Closure v2, P3.)
    pub fn is_shared_mut(&self, var: &VarId) -> bool {
        self.shared_mut_vars.contains(var)
    }

    pub fn is_module_var(&self, var: &VarId, name: &str) -> bool {
        matches!(self.get_var_storage(var, name), VarStorage::ModuleCell | VarStorage::ModuleRc)
    }
}
