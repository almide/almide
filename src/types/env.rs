use super::{Ty, VariantCase, substitute, ProtocolDef};

pub struct TypeEnv {
    /// User-defined type declarations: name -> Ty
    pub types: std::collections::HashMap<std::string::String, Ty>,
    /// Function signatures: name -> FnSig
    pub functions: std::collections::HashMap<std::string::String, super::FnSig>,
    /// Local variable scopes (stack of scopes)
    pub scopes: Vec<std::collections::HashMap<std::string::String, Ty>>,
    /// Current function's return type
    pub current_ret: Option<Ty>,
    /// Whether auto-unwrapping of Result is enabled (effect fn bodies)
    pub auto_unwrap: bool,
    /// Whether effect functions may be called from this context
    pub can_call_effect: bool,
    /// Set of effect function names
    pub effect_fns: std::collections::HashSet<std::string::String>,
    /// Variant constructor name -> (variant type name, case info)
    pub constructors: std::collections::HashMap<std::string::String, (std::string::String, VariantCase)>,
    /// User-defined module names (for distinguishing from stdlib in module calls)
    pub user_modules: std::collections::HashSet<std::string::String>,
    /// Stdlib modules available in scope (Tier 1 implicit + explicitly imported)
    pub imported_stdlib: std::collections::HashSet<std::string::String>,
    /// Whether we're inside a do block (for auto-unwrapping Result in let bindings)
    pub in_do_block: bool,
    /// Track used variables (for unused variable warnings)
    pub used_vars: std::collections::HashSet<std::string::String>,
    /// Track used modules (for unused import warnings)
    pub used_modules: std::collections::HashSet<std::string::String>,
    /// Maps import name ("json") to qualified name ("json_v2") for versioned deps
    pub module_aliases: std::collections::HashMap<std::string::String, std::string::String>,
    /// Symbols that are local (file-private) in their module: "module.func" -> true
    pub local_symbols: std::collections::HashSet<std::string::String>,
    /// Temporarily suppress auto-unwrap of Result (for match on ok/err)
    pub skip_auto_unwrap: bool,
    /// Variables declared with `var` (mutable). Parameters and `let` are immutable.
    pub mutable_vars: std::collections::HashSet<std::string::String>,
    /// Variables that are function parameters (for better error messages).
    pub param_vars: std::collections::HashSet<std::string::String>,
    /// Declaration locations: variable name -> (line, col)
    pub var_decl_locs: std::collections::HashMap<std::string::String, (usize, usize)>,
    /// Top-level `let` constants: name -> type
    pub top_lets: std::collections::HashMap<std::string::String, Ty>,
    /// Types that implement the Eq protocol (via `deriving Eq`)
    pub eq_types: std::collections::HashSet<std::string::String>,
    /// Structural bounds for generic type parameters: TypeVar name → OpenRecord constraint
    pub structural_bounds: std::collections::HashMap<std::string::String, Ty>,
    /// Protocol bounds for generic type parameters in scope: TypeVar name → list of protocol names
    pub generic_protocol_bounds: std::collections::HashMap<std::string::String, Vec<std::string::String>>,
    /// Minimum required arguments for functions with default params: fn key -> min count
    pub fn_min_params: std::collections::HashMap<std::string::String, usize>,
    /// Protocol definitions: protocol name → ProtocolDef
    pub protocols: std::collections::HashMap<std::string::String, ProtocolDef>,
    /// Types' declared protocol conformances: type name → set of protocol names
    pub type_protocols: std::collections::HashMap<std::string::String, std::collections::HashSet<std::string::String>>,
    /// Protocol conformances already validated via `impl` blocks (skip re-validation)
    pub impl_validated: std::collections::HashSet<(std::string::String, std::string::String)>,
}

impl TypeEnv {
    pub fn new() -> Self {
        TypeEnv {
            types: std::collections::HashMap::new(),
            functions: std::collections::HashMap::new(),
            scopes: vec![std::collections::HashMap::new()],
            current_ret: None,
            auto_unwrap: false,
            can_call_effect: false,
            effect_fns: std::collections::HashSet::new(),
            constructors: std::collections::HashMap::new(),
            user_modules: std::collections::HashSet::new(),
            imported_stdlib: {
                let mut s = std::collections::HashSet::new();
                // Tier 1: implicit imports (core type modules)
                for m in &["string", "int", "float", "list", "map", "set", "option", "result"] {
                    s.insert(m.to_string());
                }
                s
            },
            in_do_block: false,
            used_vars: std::collections::HashSet::new(),
            used_modules: std::collections::HashSet::new(),
            module_aliases: std::collections::HashMap::new(),
            local_symbols: std::collections::HashSet::new(),
            skip_auto_unwrap: false,
            mutable_vars: std::collections::HashSet::new(),
            param_vars: std::collections::HashSet::new(),
            var_decl_locs: std::collections::HashMap::new(),
            top_lets: std::collections::HashMap::new(),
            eq_types: std::collections::HashSet::new(),
            structural_bounds: std::collections::HashMap::new(),
            generic_protocol_bounds: std::collections::HashMap::new(),
            fn_min_params: std::collections::HashMap::new(),
            protocols: std::collections::HashMap::new(),
            type_protocols: std::collections::HashMap::new(),
            impl_validated: std::collections::HashSet::new(),
        }
    }

    /// Check if a type implements the Eq protocol.
    /// Primitives are implicitly Eq. Container types are Eq if their elements are.
    /// User-defined types require `deriving Eq`. Function types are never Eq.
    /// Check if a type supports equality (`==`, `!=`).
    /// All value types are Eq by default. Only function types are not.
    pub fn is_eq(&self, ty: &Ty) -> bool {
        let mut seen = std::collections::HashSet::new();
        self.is_eq_inner(ty, &mut seen)
    }

    fn is_eq_inner(&self, ty: &Ty, seen: &mut std::collections::HashSet<std::string::String>) -> bool {
        match ty {
            // Fn types are never Eq
            Ty::Fn { .. } => false,
            // Named/Variant need cycle detection via `seen`
            Ty::Variant { name, .. } => {
                if !seen.insert(name.clone()) {
                    return true; // Recursive type — assume Eq to break cycle
                }
                ty.children().iter().all(|child| self.is_eq_inner(child, seen))
            }
            Ty::Named(name, _) => {
                if !seen.insert(name.clone()) {
                    return true;
                }
                if let Some(resolved) = self.types.get(name) {
                    self.is_eq_inner(resolved, seen)
                } else {
                    true
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

    fn is_hash_inner(&self, ty: &Ty, seen: &mut std::collections::HashSet<std::string::String>) -> bool {
        match ty {
            // Float, Fn, Map are never hashable
            Ty::Float | Ty::Fn { .. } => false,
            Ty::Applied(super::TypeConstructorId::Map, _) => false,
            // Named/Variant need cycle detection via `seen`
            Ty::Variant { name, .. } => {
                if !seen.insert(name.clone()) {
                    return true;
                }
                ty.children().iter().all(|child| self.is_hash_inner(child, seen))
            }
            Ty::Named(name, _) => {
                if !seen.insert(name.clone()) {
                    return true;
                }
                if let Some(resolved) = self.types.get(name) {
                    self.is_hash_inner(resolved, seen)
                } else {
                    true
                }
            }
            // All other types: hashable if all children are hashable
            _ => ty.children().iter().all(|child| self.is_hash_inner(child, seen)),
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(std::collections::HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn define_var(&mut self, name: &str, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    pub fn define_var_at(&mut self, name: &str, ty: Ty, line: usize, col: usize) {
        self.define_var(name, ty);
        self.var_decl_locs.insert(name.to_string(), (line, col));
    }

    pub fn var_decl_loc(&self, name: &str) -> Option<(usize, usize)> {
        self.var_decl_locs.get(name).copied()
    }

    pub fn lookup_var(&self, name: &str) -> Option<&Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    pub fn resolve_named(&self, ty: &Ty) -> Ty {
        self.resolve_named_with_seen(ty, &mut std::collections::HashSet::new())
    }

    fn resolve_named_with_seen(&self, ty: &Ty, seen: &mut std::collections::HashSet<std::string::String>) -> Ty {
        match ty {
            Ty::Named(name, args) => {
                // Cycle detection: prevent infinite recursion on recursive type aliases
                if !seen.insert(name.clone()) {
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
                            .map(|(name, arg)| (name.clone(), arg.clone()))
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
    pub fn collect_typevars(ty: &Ty, out: &mut Vec<std::string::String>) {
        if let Ty::TypeVar(name) = ty {
            if !out.contains(name) {
                out.push(name.clone());
            }
            return;
        }
        for child in ty.children() {
            Self::collect_typevars(child, out);
        }
    }
}
