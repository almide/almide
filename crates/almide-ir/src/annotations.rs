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
}
// Module-global storage (Cell / RcRefCell / Const / Lazy) lives in
// `top_let_storage::TopLetStorage` since §4 — VarStorage is LOCALS-ONLY.

/// Annotations populated by Nanopass passes, read by the walker.
#[derive(Debug, Clone, Default)]
pub struct CodegenAnnotations {
    /// Unified variable storage classification.
    /// Keyed by VarId for local vars (RcCow) and module vars with known ids.
    pub var_storage: HashMap<VarId, VarStorage>,
    /// Temps that must ALWAYS clone on use (CaptureClone's `__cap_*` clone
    /// bindings, LICM's `__licm_*` hoists) — marked by the PRODUCING pass via
    /// a VarId snapshot, replacing the name-prefix tests in CloneInsertion
    /// (the #486-class fragility: a rename scheme change silently broke
    /// classification with no gate).
    pub always_clone_vars: HashSet<VarId>,
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
    /// Record types ALL of whose generic params are phantom (declared but used
    /// by no field). Rust rejects an unused type param (`error[E0392]`), so the
    /// Rust struct is emitted WITHOUT generics and every `Ty::Named` reference to
    /// it drops its type args — the params carry no runtime data (codegen erases
    /// types), so `Tagged[String]` and `Tagged[Int]` share one representation,
    /// matching the wasm target which already erases them (#621). Construction
    /// and patterns are unaffected (struct literals carry no type args).
    pub phantom_param_structs: HashSet<String>,
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
    /// Bindings whose declared IR type is REAL (the postcondition gate verifies
    /// it) but whose Rust-side representation is a borrow the `Ty` system cannot
    /// spell (a TCO borrow-preserved `Bytes` param temp is `&Vec<u8>`): the
    /// walker renders their annotation as `_` and lets Rust infer. Replaces the
    /// old smuggle of `Ty::Unknown` through the temp's type, which the
    /// ConcretizeTypes postcondition rightly refuses. Populated by
    /// `TailCallOptPass`.
    pub infer_binding_tys: BTreeSet<VarId>,
}

impl CodegenAnnotations {
    /// Look up the storage mode for a variable. Checks VarId first (precise),
    /// then falls back to uppercased name for cross-module synthetic refs only.
    pub fn get_var_storage(&self, var: &VarId) -> VarStorage {
        self.var_storage.get(var).copied().unwrap_or(VarStorage::Local)
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

    /// True if `var`'s binding must render its type annotation as `_` on the
    /// Rust target (the runtime representation is a borrow the `Ty` system
    /// cannot spell). The IR type stays real. (TailCallOptPass.)
    pub fn is_infer_binding(&self, var: &VarId) -> bool {
        self.infer_binding_tys.contains(var)
    }

    /// Alias-resolved global lookup — the ONLY way stage-2 consumers are
    /// meant to ask "is this VarId a module global, and how is it stored?".
    pub fn global(&self, id: VarId) -> Option<&crate::top_let_storage::GlobalInfo> {
        let decl = self.global_alias.get(&id).copied().unwrap_or(id);
        self.globals.get(&decl)
    }
}
