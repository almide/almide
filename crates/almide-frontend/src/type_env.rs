use almide_lang::types::{Ty, VariantCase, substitute, ProtocolDef};
use almide_base::intern::{Sym, sym};
use crate::import_table::ImportTable;

pub struct EnvKeySnapshot {
    functions: std::collections::HashSet<Sym>,
    types: std::collections::HashSet<Sym>,
    /// key → candidate count at snapshot time. Constructor registration
    /// PUSHES candidates onto a Vec (same name across variant types), so a
    /// key-set restore would leak candidates pushed under a pre-existing key
    /// — the temp unprefixed registration then double-counts an owner and
    /// `report_ambiguous_ctor` fires a false "declared in T and T" (#785
    /// exposed this: the top-let refresh registers BEFORE the entry is
    /// inferred, where the old infer_module leak was ordered after).
    constructors: std::collections::HashMap<Sym, usize>,
    top_lets: std::collections::HashSet<Sym>,
}

#[derive(Clone)]
pub struct TypeEnv {
    /// User-defined type declarations: name -> Ty
    pub types: std::collections::HashMap<Sym, Ty>,
    /// Function signatures: name -> FnSig
    pub functions: std::collections::HashMap<Sym, almide_lang::types::FnSig>,
    /// Deprecated functions: same key as `functions`. Populated at
    /// registration from `@deprecated`; read at every call site so the
    /// warning can name the replacement instead of leaving the caller to
    /// guess it.
    pub deprecations: std::collections::HashMap<Sym, crate::deprecation::Deprecation>,
    /// Diagnostics raised while reading declaration attributes (E053).
    /// Collected on the env because registration has no diagnostics sink of
    /// its own; drained by the canonicalizer.
    pub attr_diagnostics: Vec<almide_base::diagnostic::Diagnostic>,
    /// Local variable scopes (stack of scopes)
    pub scopes: Vec<std::collections::HashMap<Sym, Ty>>,
    /// Current function's return type
    pub current_ret: Option<Ty>,
    /// ADR-0006 D1 (#1108 Phase 2b): the INNERMOST lambda's provisional
    /// failure channel — `Result[fresh, String]`. A `!` in the lambda body
    /// propagates into THIS channel (never across the closure boundary,
    /// #489 unchanged); if the body uses it, the lambda infers fallible.
    pub lambda_ret: Option<Ty>,
    /// Whether the innermost lambda's channel was actually used — the
    /// usage-driven fallibility bit (L2).
    pub lambda_prop_used: bool,
    /// Whether auto-unwrapping of Result is enabled (effect fn bodies)
    pub auto_unwrap: bool,
    /// Whether effect functions may be called from this context
    pub can_call_effect: bool,
    /// Set while checking a METERED pure region (`fan.bounded` body /
    /// `fan.race` arms) — names the surface so the effect-isolation
    /// diagnostic can say "move the effect out of the region" instead of
    /// the misleading "mark the caller as effect fn" (the caller already is).
    pub metered_region: Option<&'static str>,
    /// Variant constructor name -> candidate (variant type name, case info) list.
    /// Usually one entry; MORE than one when the same constructor name is declared
    /// in multiple variant types (e.g. a local type and a dependency's) — an
    /// ambiguous name. `lookup_ctor` returns the first; `ctor_candidate_count`
    /// detects ambiguity (#413).
    pub constructors: std::collections::HashMap<Sym, Vec<(Sym, Option<Sym>, VariantCase)>>,
    /// Set while `infer_module` temporarily registers a module's declarations
    /// UNPREFIXED for intra-module resolution: the real owning module of those
    /// alias registrations. Constructor-candidate bookkeeping uses it so the
    /// alias and the canonical prefixed entry collapse to ONE candidate —
    /// otherwise a module's own bare `Wrap(n)` saw two candidates that both
    /// answer `Wrapper` and reported a phantom E019 ("declared in Wrapper and
    /// Wrapper", #862's surfacing of the module-diagnostics path).
    pub alias_owner_module: Option<Sym>,
    /// Set when the ENTRY program is a bundled stdlib module compiled on its
    /// own (`almide compile bytes --json` stages the bundled source as the
    /// entry): that module's name. The entry program's unprefixed `type`
    /// declaration of a name the module OWNS (`STDLIB_OWNED_TYPES`) is then
    /// the stdlib's own registration and keeps the bare key — the identity
    /// every stdlib signature carries — instead of the entry program's
    /// shadow scope (#1828's `self.X`, which is for a USER program declaring
    /// the name). `None` for every other entry program.
    pub entry_bundled_module: Option<Sym>,
    /// User-defined module names (for distinguishing from stdlib in module calls)
    pub user_modules: std::collections::HashSet<Sym>,
    /// PACKAGE roots imported as external dependencies (`import snaidhm`,
    /// `import extlib` — any non-`self`, non-stdlib import root seen in the
    /// main program or ANY module). Visibility's project identity (#870):
    /// a DOTTED module name belongs to its first segment's package; a BARE
    /// name is a dep package iff it is in this set, else the SELF package
    /// (self submodules load under bare names, dep submodules under
    /// `dep.sub` dotted names).
    pub dep_root_modules: std::collections::HashSet<Sym>,
    /// The package's own module name (set when `register_module` is called with `is_self: true`).
    /// Used to resolve `import self` in the main file.
    pub self_module_name: Option<Sym>,
    /// Single source of truth for import resolution (aliases, accessible modules, stdlib, usage tracking).
    pub import_table: ImportTable,

    /// Visibility of user-defined functions keyed by fn key ("module.func" or bare "func").
    /// Absent entries default to `Public` (stdlib, derive-generated, builtins).
    /// Checked in `resolve_module_call` to reject cross-module access to `mod fn` / `local fn`.
    pub fn_visibility: std::collections::HashMap<Sym, almide_lang::ast::Visibility>,

    /// Track used variables (for unused variable warnings)
    pub used_vars: std::collections::HashSet<Sym>,
    /// Symbols that are local (file-private) in their module: "module.func" -> true
    pub local_symbols: std::collections::HashSet<Sym>,
    /// Temporarily suppress auto-unwrap of Result (for match on ok/err)
    pub skip_auto_unwrap: bool,
    /// Variable names whose `let` binding should NOT auto-unwrap Result
    /// because they're later used as the subject of a `match x { ok(_) =>
    /// ..., err(_) => ... }`. Pre-computed at block entry (see Block
    /// inference in check/infer.rs).
    pub skip_auto_unwrap_for: std::collections::HashSet<Sym>,
    /// Variables declared with `var` (mutable). Parameters and `let` are immutable.
    pub mutable_vars: std::collections::HashSet<Sym>,
    /// Escape analysis: current lambda nesting depth (0 = not in lambda).
    pub lambda_depth: usize,
    /// Escape analysis: the lambda depth at which each `var` was declared.
    pub var_lambda_depth: std::collections::HashMap<Sym, usize>,
    /// Variables that are function parameters (for better error messages).
    pub param_vars: std::collections::HashSet<Sym>,
    /// Declaration locations: variable name -> (line, col)
    pub var_decl_locs: std::collections::HashMap<Sym, (usize, usize)>,
    /// Top-level `let` constants: name -> type
    pub top_lets: std::collections::HashMap<Sym, Ty>,
    /// Record type key (same keys as `types`) -> field names that carry a
    /// declared DEFAULT. Used by record-construction validation: a missing
    /// field is an error only when it has no default (#488).
    pub record_field_defaults: std::collections::HashMap<Sym, std::collections::HashSet<Sym>>,
    /// Record-payload variant CASE name -> field names with a declared
    /// default (`| Rect { color: String = "" }`). Keyed by bare ctor name;
    /// same-name ctors across types union their sets (#413 corner — the
    /// worst case is a suppressed missing-field error, never wrong code).
    pub ctor_field_defaults: std::collections::HashMap<Sym, std::collections::HashSet<Sym>>,
    /// Bare type names that are currently a dual-registration of a PREFIXED
    /// (dependency / submodule) type — i.e. `env.types["Persona"]` mirrors
    /// `env.types["fizz_persona.Persona"]` for unqualified access. A LOCAL type
    /// (main program, no prefix) with the same name is allowed to shadow this
    /// bare alias instead of colliding with it (#433): unqualified use resolves
    /// to the local type, the dependency's stays reachable via its qualified key.
    pub prefixed_bare_aliases: std::collections::HashSet<Sym>,
    /// Types that implement the Eq protocol (via `deriving Eq`)
    pub eq_types: std::collections::HashSet<Sym>,
    /// Structural bounds for generic type parameters: TypeVar name → OpenRecord constraint
    pub structural_bounds: std::collections::HashMap<Sym, Ty>,
    /// Protocol bounds for generic type parameters in scope: TypeVar name → list of protocol names
    pub generic_protocol_bounds: std::collections::HashMap<Sym, Vec<Sym>>,
    /// Minimum required arguments for functions with default params: fn key -> min count
    pub fn_min_params: std::collections::HashMap<Sym, usize>,
    /// Default parameter expressions, keyed by the SAME prefixed fn key as
    /// `fn_min_params` (`lib.greet`, not `greet`). Lowering's own per-file map
    /// only ever sees the program being lowered, so a call into an imported
    /// module had no defaults to fill from (#1088).
    pub fn_defaults: std::collections::HashMap<Sym, Vec<Option<almide_lang::ast::Expr>>>,
    /// Protocol definitions: protocol name → ProtocolDef
    pub protocols: std::collections::HashMap<Sym, ProtocolDef>,
    /// Explicit `fn Type.method` declarations that have a body, keyed by the
    /// prefixed fn key — the cross-module half of lowering's per-file set.
    pub explicit_convention_fns: std::collections::HashSet<Sym>,
    /// Types' declared protocol conformances: type name → set of protocol names
    pub type_protocols: std::collections::HashMap<Sym, std::collections::HashSet<Sym>>,
    /// Function declaration locations: fn key -> (line, col)
    pub fn_decl_spans: std::collections::HashMap<Sym, (usize, usize)>,
    /// Whether we're inside a test block (effect fn calls return Result[T, String])
    pub in_test_block: bool,
    /// Fn names whose parse failed mid-body. Checker suppresses cascading
    /// "undefined function 'name'" diagnostics for calls to these — the real
    /// cause is the parse error already surfaced.
    pub failed_fn_names: std::collections::HashSet<String>,

    /// Maps canonical module name → versioned module name for dependencies.
    /// e.g. "snaidhm.web.gpu" → "snaidhm_v0.web.gpu"
    /// Used by expression lowering to generate correct cross-module constant names.
    pub module_versioned_names: std::collections::HashMap<Sym, Sym>,

    /// DefTable: canonical definitions for all symbols (functions, types, top-lets).
    /// Populated during register_decls, used for cross-package name resolution.
    pub def_table: almide_ir::DefTable,
    /// Qualified name → DefId lookup: "list.push" → DefId(42), "SafeHtml" → DefId(99)
    pub def_map: std::collections::HashMap<Sym, almide_ir::DefId>,

    /// Opaque type aliases: `mod type SafeHtml = String` → stores inner target type.
    pub opaque_alias_targets: std::collections::HashMap<Sym, Ty>,
    /// Opaque type alias constructor visibility.
    pub opaque_alias_visibility: std::collections::HashMap<Sym, crate::ast::Visibility>,
    /// Which module defined each opaque alias (None = main file).
    pub opaque_alias_module: std::collections::HashMap<Sym, Option<Sym>>,
}

impl TypeEnv {
    pub fn new() -> Self {
        TypeEnv {
            deprecations: std::collections::HashMap::new(),
            attr_diagnostics: Vec::new(),
            types: std::collections::HashMap::new(),
            functions: std::collections::HashMap::new(),
            scopes: vec![std::collections::HashMap::new()],
            current_ret: None,
            lambda_ret: None,
            lambda_prop_used: false,
            auto_unwrap: false,
            can_call_effect: false,
            metered_region: None,
            constructors: std::collections::HashMap::new(),
            user_modules: std::collections::HashSet::new(),
            dep_root_modules: std::collections::HashSet::new(),
            self_module_name: None,
            import_table: ImportTable::new(),
            fn_visibility: std::collections::HashMap::new(),
            record_field_defaults: std::collections::HashMap::new(),
            ctor_field_defaults: std::collections::HashMap::new(),

            used_vars: std::collections::HashSet::new(),
            local_symbols: std::collections::HashSet::new(),
            skip_auto_unwrap: false,
            skip_auto_unwrap_for: std::collections::HashSet::new(),
            mutable_vars: std::collections::HashSet::new(),
            lambda_depth: 0,
            var_lambda_depth: std::collections::HashMap::new(),
            param_vars: std::collections::HashSet::new(),
            var_decl_locs: std::collections::HashMap::new(),
            top_lets: std::collections::HashMap::new(),
            prefixed_bare_aliases: std::collections::HashSet::new(),
            eq_types: std::collections::HashSet::new(),
            structural_bounds: std::collections::HashMap::new(),
            generic_protocol_bounds: std::collections::HashMap::new(),
            fn_min_params: std::collections::HashMap::new(),
            fn_defaults: std::collections::HashMap::new(),
            explicit_convention_fns: std::collections::HashSet::new(),
            protocols: std::collections::HashMap::new(),
            type_protocols: std::collections::HashMap::new(),
            fn_decl_spans: std::collections::HashMap::new(),
            in_test_block: false,
            failed_fn_names: std::collections::HashSet::new(),
            module_versioned_names: std::collections::HashMap::new(),
            def_table: almide_ir::DefTable::new(),
            def_map: std::collections::HashMap::new(),
            opaque_alias_targets: std::collections::HashMap::new(),
            opaque_alias_visibility: std::collections::HashMap::new(),
            opaque_alias_module: std::collections::HashMap::new(),
            alias_owner_module: None,
            entry_bundled_module: None,
        }
    }

    /// Is an UNPREFIXED declaration of `name` the stdlib's own — the entry
    /// program being the bundled module that owns the name
    /// (`entry_bundled_module`)? Such a declaration keeps the bare key; a
    /// user program's declaration of the same name takes its shadow scope
    /// (#1828).
    pub fn entry_owns_stdlib_type(&self, name: &str) -> bool {
        self.entry_bundled_module.is_some_and(|m| {
            almide_lang::stdlib_info::stdlib_owned_type_owner(name) == Some(m.as_str())
        })
    }

    /// The `opaque_alias_targets` key a constructor call or pattern spelled
    /// `name` names from `cur_mod` (#1835) — the newtype's identity
    /// (`registration::opaque_alias_identity`): the module's own `m.name`;
    /// the entry program's shadow of a stdlib-owned name (`self.name`); the
    /// bare name (a bundled module's newtype, or the entry program's plain
    /// one); else the unique OTHER module's `x.name` — a foreign constructor,
    /// which the checker reports as E033 rather than as an unknown name.
    pub fn opaque_alias_key(&self, name: &str, cur_mod: Option<&str>) -> Option<Sym> {
        let has = |k: &str| self.opaque_alias_targets.contains_key(&sym(k));
        if name.contains('.') {
            return has(name).then(|| sym(name));
        }
        if let Some(m) = cur_mod {
            let own = format!("{}.{}", m, name);
            if has(&own) {
                return Some(sym(&own));
            }
        }
        if let Some(shadow) = crate::canonicalize::resolve::stdlib_shadow_key(name, cur_mod) {
            if has(&shadow) {
                return Some(sym(&shadow));
            }
        }
        if has(name) {
            return Some(sym(name));
        }
        let mut foreign = self.opaque_alias_targets.keys()
            .filter(|k| k.as_str().rsplit_once('.').is_some_and(|(_, base)| base == name));
        let first = *foreign.next()?;
        foreign.next().is_none().then_some(first)
    }

    /// Snapshot the current keys in functions/types/constructors/top_lets.
    /// Used by module body checking to temporarily register unprefixed declarations
    /// and clean them up afterwards.
    pub fn snapshot_keys(&self) -> EnvKeySnapshot {
        EnvKeySnapshot {
            functions: self.functions.keys().cloned().collect(),
            types: self.types.keys().cloned().collect(),
            constructors: self.constructors.iter().map(|(k, v)| (*k, v.len())).collect(),
            top_lets: self.top_lets.keys().cloned().collect(),
        }
    }

    /// Remove any keys — and any constructor CANDIDATES — added since the
    /// snapshot was taken. Registration pushes candidates in order, so
    /// truncating to the snapshot count drops exactly the temp additions.
    pub fn restore_keys(&mut self, snapshot: &EnvKeySnapshot) {
        self.functions.retain(|k, _| snapshot.functions.contains(k));
        self.types.retain(|k, _| snapshot.types.contains(k));
        self.constructors.retain(|k, _| snapshot.constructors.contains_key(k));
        for (k, v) in self.constructors.iter_mut() {
            if let Some(&n) = snapshot.constructors.get(k) {
                v.truncate(n);
            }
        }
        self.top_lets.retain(|k, _| snapshot.top_lets.contains(k));
    }

    pub fn is_eq(&self, ty: &Ty) -> bool {
        let mut seen = std::collections::HashSet::new();
        self.is_eq_inner(ty, &mut seen)
    }

    fn is_eq_inner(&self, ty: &Ty, seen: &mut std::collections::HashSet<Sym>) -> bool {
        match ty {
            // Fn types are never Eq
            Ty::Fn { .. } => false,
            // Named/Variant need cycle detection via `seen`
            Ty::Variant { name, .. } => {
                if !seen.insert(*name) {
                    return true; // Recursive type — assume Eq to break cycle
                }
                ty.children().iter().all(|child| self.is_eq_inner(child, seen))
            }
            Ty::Named(name, _) => {
                if !seen.insert(*name) {
                    return true;
                }
                match self.types.get(name) {
                    // A Named that resolves to its variant DEFINITION is one
                    // logical node: re-entering the Variant arm would read the
                    // name already in `seen` as a cycle and skip the payloads
                    // (#1773's class — a Float payload passed as hashable).
                    // Claim the variant's name and walk the payloads directly.
                    Some(resolved @ Ty::Variant { name: vn, .. }) => {
                        seen.insert(*vn);
                        resolved.children().iter().all(|child| self.is_eq_inner(child, seen))
                    }
                    Some(resolved) => self.is_eq_inner(resolved, seen),
                    None => true,
                }
            }
            // All other types: Eq if all children are Eq
            _ => ty.children().iter().all(|child| self.is_eq_inner(child, seen)),
        }
    }

    /// Check if a type is hashable (can be used as a Map key).
    /// All value types except Float and Fn are hashable.
    pub fn is_hash(&self, ty: &Ty) -> bool {
        let mut seen = std::collections::HashSet::new();
        self.is_hash_inner(ty, &mut seen)
    }

    fn is_hash_inner(&self, ty: &Ty, seen: &mut std::collections::HashSet<Sym>) -> bool {
        match ty {
            // Float, Fn, Map are never hashable
            Ty::Float | Ty::Fn { .. } => false,
            Ty::Applied(almide_lang::types::TypeConstructorId::Map, _) => false,
            // Named/Variant need cycle detection via `seen`
            Ty::Variant { name, .. } => {
                if !seen.insert(*name) {
                    return true;
                }
                ty.children().iter().all(|child| self.is_hash_inner(child, seen))
            }
            Ty::Named(name, _) => {
                if !seen.insert(*name) {
                    return true;
                }
                match self.types.get(name) {
                    // One logical node with its variant definition — see
                    // is_eq_inner. Without the bypass the payload walk was
                    // skipped as a false cycle and `| Temp(Float)` keyed a
                    // Map (#1773: check passed, rustc refused Hash on f64).
                    Some(resolved @ Ty::Variant { name: vn, .. }) => {
                        seen.insert(*vn);
                        resolved.children().iter().all(|child| self.is_hash_inner(child, seen))
                    }
                    Some(resolved) => self.is_hash_inner(resolved, seen),
                    None => true,
                }
            }
            // All other types: hashable if all children are hashable
            _ => ty.children().iter().all(|child| self.is_hash_inner(child, seen)),
        }
    }

    /// Why [`Self::is_hash`] said no, for diagnostics: the first unhashable
    /// LEAF in the type ("a function", "a Float", "a Map"), or `None` when the
    /// type is hashable. Mirrors `is_hash_inner`'s traversal exactly, cycle
    /// detection included, so the two can never disagree on reachability
    /// (#1518: a fn-typed field inside a Named record key needs the E016
    /// closure wording, not the generic unhashable one).
    pub fn hash_blocker(&self, ty: &Ty) -> Option<&'static str> {
        let mut seen = std::collections::HashSet::new();
        self.hash_blocker_inner(ty, &mut seen)
    }

    fn hash_blocker_inner(&self, ty: &Ty, seen: &mut std::collections::HashSet<Sym>) -> Option<&'static str> {
        match ty {
            Ty::Float => Some("a Float"),
            Ty::Fn { .. } => Some("a function"),
            Ty::Applied(almide_lang::types::TypeConstructorId::Map, _) => Some("a Map"),
            Ty::Variant { name, .. } => {
                if !seen.insert(*name) {
                    return None;
                }
                ty.children().iter().find_map(|child| self.hash_blocker_inner(child, seen))
            }
            Ty::Named(name, _) => {
                if !seen.insert(*name) {
                    return None;
                }
                match self.types.get(name) {
                    // Mirror of is_hash_inner's variant-definition bypass —
                    // the two traversals must never disagree on reachability.
                    Some(resolved @ Ty::Variant { name: vn, .. }) => {
                        seen.insert(*vn);
                        resolved.children().iter().find_map(|child| self.hash_blocker_inner(child, seen))
                    }
                    Some(resolved) => self.hash_blocker_inner(resolved, seen),
                    None => None,
                }
            }
            _ => ty.children().iter().find_map(|child| self.hash_blocker_inner(child, seen)),
        }
    }

    /// Can values of `ty` be ORDERED end-to-end (the native runtime's `T: Ord`
    /// bound on list.sort/min/max and sort_by keys)? Float is rejected HERE —
    /// f64 is not Ord — and the caller special-cases the BARE-Float element,
    /// which routes to the dedicated `_float` twins. Fn/Map/Set never order.
    pub fn is_ord(&self, ty: &Ty) -> bool {
        let mut seen = std::collections::HashSet::new();
        self.is_ord_inner(ty, &mut seen)
    }

    fn is_ord_inner(&self, ty: &Ty, seen: &mut std::collections::HashSet<Sym>) -> bool {
        match ty {
            Ty::Float | Ty::Float32 | Ty::Float64 | Ty::Fn { .. } => false,
            Ty::Applied(almide_lang::types::TypeConstructorId::Map, _) => false,
            Ty::Applied(almide_lang::types::TypeConstructorId::Set, _) => false,
            Ty::Variant { name, .. } => {
                if !seen.insert(*name) {
                    return true;
                }
                self.declares_ord(*name)
                    && ty.children().iter().all(|child| self.is_ord_inner(child, seen))
            }
            Ty::Named(name, _) => {
                if !seen.insert(*name) {
                    return true;
                }
                if let Some(resolved) = self.types.get(name).cloned() {
                    // #1521: a user record/variant is Ord natively only when it
                    // DECLARES `: Ord` (the derive). Structural orderability of
                    // its fields is not enough — check said yes while rustc
                    // rejected the monomorph ("P: Ord is not satisfied") —
                    // list.max(List[record]) was the silent cell.
                    if matches!(resolved, Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Variant { .. })
                        && !self.declares_ord(*name)
                    {
                        return false;
                    }
                    // Variant-definition bypass (see is_eq_inner): the
                    // declares_ord gate above already admitted the derive;
                    // the payloads must still order structurally.
                    if let Ty::Variant { name: vn, .. } = &resolved {
                        seen.insert(*vn);
                        return resolved.children().iter().all(|child| self.is_ord_inner(child, seen));
                    }
                    self.is_ord_inner(&resolved, seen)
                } else {
                    true
                }
            }
            _ => ty.children().iter().all(|child| self.is_ord_inner(child, seen)),
        }
    }

    /// Does the user type declare `: Ord`? Keyed leniently like the derive
    /// checks: `type_protocols` interns bare names, a cross-module type may
    /// carry the qualified `mod.Type` spelling — accept either.
    fn declares_ord(&self, name: Sym) -> bool {
        let bare = name.as_str().rsplit('.').next().unwrap_or(name.as_str());
        let declares = |n: &str| self.type_protocols.get(&sym(n))
            .is_some_and(|s| s.contains(&sym("Ord")));
        declares(name.as_str()) || declares(bare)
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(std::collections::HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn define_var(&mut self, name: &str, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(sym(name), ty);
        }
    }

    pub fn define_var_at(&mut self, name: &str, ty: Ty, line: usize, col: usize) {
        self.define_var(name, ty);
        self.var_decl_locs.insert(sym(name), (line, col));
    }

    pub fn var_decl_loc(&self, name: &str) -> Option<(usize, usize)> {
        self.var_decl_locs.get(&sym(name)).copied()
    }

    pub fn lookup_var(&self, name: &str) -> Option<&Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(&sym(name)) {
                return Some(ty);
            }
        }
        None
    }

    /// Collect all visible names (variables, top_lets, functions, builtins) for "did you mean?" suggestions.
    pub fn all_visible_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for scope in &self.scopes {
            for name in scope.keys() {
                names.push(name.to_string());
            }
        }
        for name in self.top_lets.keys() {
            names.push(name.to_string());
        }
        for name in self.functions.keys() {
            names.push(name.to_string());
        }
        // Builtins not in env.functions
        for &b in &["println", "eprintln", "panic", "assert", "assert_eq", "assert_ne", "to_string"] {
            names.push(b.to_string());
        }
        // Stdlib module-qualified function names (e.g. "string.trim", "list.map")
        for &module in crate::stdlib::STDLIB_MODULES {
            for func in crate::stdlib::module_functions(module) {
                names.push(format!("{}.{}", module, func));
            }
        }
        names
    }

    /// Resolve a bare variant-constructor name to its (type name, case). Returns
    /// the FIRST registered candidate (deterministic). When the name is ambiguous
    /// (`ctor_candidate_count > 1`) callers should report it; this fallback keeps
    /// type checking from cascading.
    pub fn lookup_ctor(&self, name: &Sym) -> Option<(Sym, VariantCase)> {
        self.constructors.get(name).and_then(|cands| cands.first().map(|(t, _m, c)| (*t, c.clone())))
    }

    /// Like `lookup_ctor`, but only among candidates OWNED by `module` — the
    /// qualified-access question: `mod.Ctor` must resolve inside `mod` alone,
    /// never to another module's same-named constructor that happened to
    /// register first (almide#1426, edit-locality hunt V3).
    pub fn lookup_ctor_owned(&self, name: &Sym, module: &str) -> Option<(Sym, VariantCase)> {
        let cands = self.constructors.get(name)?;
        cands.iter()
            .find(|(_, owner, _)| owner.map_or(false, |o| o.as_str() == module))
            .map(|(t, _m, c)| (*t, c.clone()))
    }

    /// Like `lookup_ctor`, but when the constructor name is ambiguous across
    /// packages, prefer the candidate declared in `cur_mod` (#413) and return its
    /// type name QUALIFIED with that owner (`mod.Type`) so the construction's `.ty`
    /// is the namespaced enum. A module's own bare `Active` means *its* `Active`.
    pub fn lookup_ctor_in(&self, name: &Sym, cur_mod: Option<&str>) -> Option<(Sym, VariantCase)> {
        let cands = self.constructors.get(name)?;
        let pick = cur_mod
            .and_then(|m| cands.iter().find(|(_, owner, _)| owner.map_or(false, |o| o.as_str() == m)))
            .or_else(|| cands.first())?;
        let (t, owner, c) = pick;
        // Qualify with the owner so the resolved `.ty` carries the namespaced enum
        // (`mod.Type`) — unless already qualified or owned by stdlib.
        let qual = match owner {
            Some(o) if !t.as_str().contains('.') && !almide_lang::stdlib_info::is_bundled_module(o.as_str())
                => sym(&format!("{}.{}", o.as_str(), t.as_str())),
            _ => *t,
        };
        Some((qual, c.clone()))
    }

    /// Does `cur_mod` itself declare this constructor? When it does,
    /// `lookup_ctor_in` picks that candidate deterministically, so the bare
    /// name is unambiguous inside that module however many sibling packages
    /// reuse it (#413). `None` (the entry program) owns nothing.
    pub fn ctor_owned_by(&self, name: &Sym, cur_mod: Option<&str>) -> bool {
        let Some(m) = cur_mod else { return false };
        self.constructors.get(name).is_some_and(|cands| {
            cands.iter().any(|(_, owner, _)| owner.is_some_and(|o| o.as_str() == m))
        })
    }

    /// How many variant types declare this constructor name (1 = unambiguous,
    /// >1 = ambiguous, e.g. a local type and a dependency share the name).
    pub fn ctor_candidate_count(&self, name: &Sym) -> usize {
        self.constructors.get(name).map_or(0, |c| c.len())
    }

    /// The variant type names that declare this constructor, for an ambiguity
    /// diagnostic that lists `Type::Ctor` qualifications to disambiguate.
    pub fn ctor_candidate_types(&self, name: &Sym) -> Vec<Sym> {
        self.constructors.get(name).map_or_else(Vec::new, |c| c.iter().map(|(t, _m, _c)| *t).collect())
    }

    pub fn resolve_named(&self, ty: &Ty) -> Ty {
        self.resolve_named_with_seen(ty, &mut std::collections::HashSet::new())
    }

    fn resolve_named_with_seen(&self, ty: &Ty, seen: &mut std::collections::HashSet<Sym>) -> Ty {
        match ty {
            Ty::Named(name, args) => {
                // Cycle detection: prevent infinite recursion on recursive type aliases
                if !seen.insert(*name) {
                    return ty.clone();
                }
                if let Some(resolved) = self.types.get(name) {
                    if args.is_empty() {
                        resolved.clone()
                    } else {
                        // Build substitution from generic params to concrete args
                        // Extract generic param names from the resolved type's TypeVars
                        let mut param_names = Vec::new();
                        Self::collect_typevars(resolved, &mut param_names);
                        let bindings: std::collections::HashMap<_, _> = param_names.iter()
                            .zip(args.iter())
                            .map(|(name, arg)| (*name, arg.clone()))
                            .collect();
                        if bindings.is_empty() { resolved.clone() } else { substitute(resolved, &bindings) }
                    }
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone(),
        }
    }

    /// Collect unique TypeVar names from a type in the order they first appear.
    /// Uses Ty::children() for uniform traversal.
    pub fn collect_typevars(ty: &Ty, out: &mut Vec<Sym>) {
        if let Ty::TypeVar(name) = ty {
            if !out.contains(name) {
                out.push(*name);
            }
            return;
        }
        for child in ty.children() {
            Self::collect_typevars(child, out);
        }
    }
}
