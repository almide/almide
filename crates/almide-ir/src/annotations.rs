use std::collections::{HashSet, HashMap, BTreeSet};
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
    /// Uppercased names of `Lazy` top_lets whose initializer can ABORT (today:
    /// contains an integer `/` or `%`, which abort on a zero divisor / MIN÷-1).
    /// `fn main` forces these in DECLARATION ORDER before running, so an aborting
    /// initializer fires at startup — matching wasm, which evaluates every top-let
    /// eagerly in `_start`. Pure lazies stay lazy (timing is unobservable for them).
    pub eager_force_top_lets: Vec<String>,
    /// VarIds of `Const`-kind top_lets. Their declarations render as
    /// `const NAME_UPPER: T = ...;`, so every reference must also render the
    /// uppercased name — a lowercase source binding (`let zero = 0`) would
    /// otherwise emit `zero` against `const ZERO` (E0425, native-only failure
    /// while wasm builds: a cross-target divergence).
    pub const_top_let_vars: HashSet<VarId>,
    /// Unified variable storage classification.
    /// Keyed by VarId for local vars (RcCow) and module vars with known ids.
    pub var_storage: HashMap<VarId, VarStorage>,
    /// Keyed by uppercased name for cross-module synthetic refs
    /// (ALMIDE_RT_<MOD>_<NAME>) that carry fresh VarIds.
    pub var_storage_by_name: HashMap<String, VarStorage>,
    /// §4 Stage 1 — the unified top-let storage attribute, computed once by
    /// `TopLetStoragePass` and asserted equal to every legacy predicate by
    /// the walker-side agreement verifier. Stage 2 makes consumers read THIS
    /// and deletes the legacy sets above.
    pub globals: HashMap<VarId, crate::top_let_storage::GlobalInfo>,
    /// Synthetic cross-module use-site VarId → declaration VarId.
    pub global_alias: HashMap<VarId, VarId>,
    /// ONE init/eager order (declaration order, root then modules) — the
    /// vector both the native force loop and wasm `__init_globals` are
    /// meant to consume in stage 2 (C-007 by construction).
    pub global_init_order: Vec<VarId>,
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
    /// Heap-typed function-local vars that are BOTH copy-aliased (some other live
    /// binding shares their heap value via `var b = a`, `let b = a`, `b = r.field`,
    /// an if/match arm, or a destructure element) AND mutated in place (IndexAssign,
    /// FieldAssign, ListSwap/Reverse/RotateLeft/CopySlice, or an in-place stdlib
    /// mutator like `list.push`/`map.insert`). Without a copy-on-write at the
    /// mutation, the sibling binding would observe the mutation — violating Almide's
    /// value semantics (RcCow doc, `lib.rs`). Populated by `AliasCowPass`.
    ///
    /// A `BTreeSet` (not `HashSet`) so iteration order is deterministic: the WASM
    /// emitter reads this to gate per-site COW, and a non-deterministic order could
    /// perturb emit and break the host-deterministic wasm32-vs-native byte gate.
    pub needs_cow: BTreeSet<VarId>,
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

    /// True if `var` is a copy-aliased, in-place-mutated heap local that needs a
    /// copy-on-write at its mutation sites to preserve value semantics. (AliasCowPass.)
    pub fn needs_cow(&self, var: &VarId) -> bool {
        self.needs_cow.contains(var)
    }

    /// Alias-resolved global lookup — the ONLY way stage-2 consumers are
    /// meant to ask "is this VarId a module global, and how is it stored?".
    pub fn global(&self, id: VarId) -> Option<&crate::top_let_storage::GlobalInfo> {
        let decl = self.global_alias.get(&id).copied().unwrap_or(id);
        self.globals.get(&decl)
    }

    pub fn is_module_var(&self, var: &VarId, name: &str) -> bool {
        matches!(self.get_var_storage(var, name), VarStorage::ModuleCell | VarStorage::ModuleRc)
    }
}
