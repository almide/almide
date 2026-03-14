/// Internal resolved type representation for the Almide type checker.
/// Distinct from ast::TypeExpr which is a syntactic node.

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Ty {
    Int,
    Float,
    String,
    Bool,
    Unit,
    List(Box<Ty>),
    Option(Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Record { fields: Vec<(std::string::String, Ty)> },
    OpenRecord { fields: Vec<(std::string::String, Ty)> },
    Variant { name: std::string::String, cases: Vec<VariantCase> },
    Fn { params: Vec<Ty>, ret: Box<Ty> },
    Tuple(Vec<Ty>),
    Named(std::string::String, Vec<Ty>),
    /// Inline union type (e.g., Int | String). Members are sorted and deduplicated.
    Union(Vec<Ty>),
    /// Type variable for user-defined generics (e.g., T, U, A, B)
    TypeVar(std::string::String),
    /// Error recovery — unifies with everything to prevent cascade errors
    Unknown,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VariantCase {
    pub name: std::string::String,
    pub payload: VariantPayload,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum VariantPayload {
    Unit,
    Tuple(Vec<Ty>),
    Record(Vec<(std::string::String, Ty, Option<crate::ast::Expr>)>),
}

impl PartialEq for VariantPayload {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (VariantPayload::Unit, VariantPayload::Unit) => true,
            (VariantPayload::Tuple(a), VariantPayload::Tuple(b)) => a == b,
            (VariantPayload::Record(a), VariantPayload::Record(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|((n1, t1, _), (n2, t2, _))| n1 == n2 && t1 == t2)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<(std::string::String, Ty)>,
    pub ret: Ty,
    pub is_effect: bool,
    #[allow(dead_code)]
    pub generics: Vec<std::string::String>,
    /// Structural bounds for generics: TypeVar name → OpenRecord constraint type
    pub structural_bounds: std::collections::HashMap<std::string::String, Ty>,
}

/// Convenience macro for creating FnSig without generics (stdlib functions)
#[macro_export]
macro_rules! fn_sig {
    (params: $params:expr, ret: $ret:expr, is_effect: $eff:expr) => {
        FnSig { params: $params, ret: $ret, is_effect: $eff, generics: vec![], structural_bounds: std::collections::HashMap::new() }
    };
}

impl FnSig {
    /// Format parameter list as "name: Type, name: Type, ..."
    pub fn format_params(&self) -> String {
        self.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ")
    }
}

pub struct TypeEnv {
    /// User-defined type declarations: name -> Ty
    pub types: std::collections::HashMap<std::string::String, Ty>,
    /// Function signatures: name -> FnSig
    pub functions: std::collections::HashMap<std::string::String, FnSig>,
    /// Local variable scopes (stack of scopes)
    pub scopes: Vec<std::collections::HashMap<std::string::String, Ty>>,
    /// Current function's return type
    pub current_ret: Option<Ty>,
    /// Whether we're inside an effect function
    pub in_effect: bool,
    /// Set of effect function names
    pub effect_fns: std::collections::HashSet<std::string::String>,
    /// Variant constructor name -> (variant type name, case info)
    pub constructors: std::collections::HashMap<std::string::String, (std::string::String, VariantCase)>,
    /// User-defined module names (for distinguishing from stdlib in module calls)
    pub user_modules: std::collections::HashSet<std::string::String>,
    /// Whether we're inside a do block (for auto-unwrapping Result in let bindings)
    pub in_do_block: bool,
    /// Whether we're inside a test block (skip auto-unwrap of Result)
    pub in_test: bool,
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
}

impl Ty {
    pub fn display(&self) -> std::string::String {
        match self {
            Ty::Int => "Int".into(),
            Ty::Float => "Float".into(),
            Ty::String => "String".into(),
            Ty::Bool => "Bool".into(),
            Ty::Unit => "Unit".into(),
            Ty::List(t) => format!("List[{}]", t.display()),
            Ty::Option(t) => format!("Option[{}]", t.display()),
            Ty::Result(t, e) => format!("Result[{}, {}]", t.display(), e.display()),
            Ty::Map(k, v) => format!("Map[{}, {}]", k.display(), v.display()),
            Ty::Record { fields } => {
                let fs: Vec<_> = fields.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect();
                format!("{{ {} }}", fs.join(", "))
            }
            Ty::OpenRecord { fields } => {
                let fs: Vec<_> = fields.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect();
                format!("{{ {}, .. }}", fs.join(", "))
            }
            Ty::Variant { name, .. } => name.clone(),
            Ty::Fn { params, ret } => {
                let ps: Vec<_> = params.iter().map(|t| t.display()).collect();
                format!("fn({}) -> {}", ps.join(", "), ret.display())
            }
            Ty::Tuple(tys) => {
                let ts: Vec<_> = tys.iter().map(|t| t.display()).collect();
                format!("({})", ts.join(", "))
            }
            Ty::Named(n, args) => {
                if args.is_empty() {
                    n.clone()
                } else {
                    let ts: Vec<_> = args.iter().map(|t| t.display()).collect();
                    format!("{}[{}]", n, ts.join(", "))
                }
            }
            Ty::Union(members) => {
                let ms: Vec<_> = members.iter().map(|t| t.display()).collect();
                ms.join(" | ")
            }
            Ty::TypeVar(n) => n.clone(),
            Ty::Unknown => "Unknown".into(),
        }
    }

    /// Returns true if this type is or contains Ty::Unknown anywhere in its structure.
    pub fn contains_unknown(&self) -> bool {
        match self {
            Ty::Unknown => true,
            Ty::List(inner) | Ty::Option(inner) => inner.contains_unknown(),
            Ty::Result(ok, err) => ok.contains_unknown() || err.contains_unknown(),
            Ty::Map(k, v) => k.contains_unknown() || v.contains_unknown(),
            Ty::Tuple(tys) => tys.iter().any(|t| t.contains_unknown()),
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| t.contains_unknown()),
            Ty::Fn { params, ret } => params.iter().any(|t| t.contains_unknown()) || ret.contains_unknown(),
            Ty::Named(_, args) => args.iter().any(|t| t.contains_unknown()),
            Ty::Variant { cases, .. } => cases.iter().any(|c| match &c.payload {
                VariantPayload::Unit => false,
                VariantPayload::Tuple(tys) => tys.iter().any(|t| t.contains_unknown()),
                VariantPayload::Record(fs) => fs.iter().any(|(_, t, _)| t.contains_unknown()),
            }),
            Ty::Union(members) => members.iter().any(|t| t.contains_unknown()),
            Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Unit | Ty::TypeVar(_) => false,
        }
    }

    /// Construct a normalized union type: flatten nested unions, deduplicate, sort.
    /// Returns the inner type if only one member remains.
    pub fn union(mut members: Vec<Ty>) -> Ty {
        // Flatten nested unions
        let mut flat = Vec::new();
        for m in members.drain(..) {
            if let Ty::Union(inner) = m {
                flat.extend(inner);
            } else {
                flat.push(m);
            }
        }
        // Deduplicate
        flat.dedup();
        let mut unique = Vec::new();
        for t in flat {
            if !unique.contains(&t) {
                unique.push(t);
            }
        }
        // Sort by display name for canonical ordering
        unique.sort_by(|a, b| a.display().cmp(&b.display()));
        match unique.len() {
            0 => Ty::Unit,
            1 => unique.into_iter().next().unwrap(),
            _ => Ty::Union(unique),
        }
    }

    /// Check if two types are compatible (Unknown matches everything)
    pub fn compatible(&self, other: &Ty) -> bool {
        if *self == Ty::Unknown || *other == Ty::Unknown {
            return true;
        }
        // TypeVars are compatible with anything (they represent polymorphic types)
        if matches!(self, Ty::TypeVar(_)) || matches!(other, Ty::TypeVar(_)) {
            return true;
        }
        match (self, other) {
            (Ty::Int, Ty::Int) => true,
            (Ty::Float, Ty::Float) => true,
            (Ty::String, Ty::String) => true,
            (Ty::Bool, Ty::Bool) => true,
            (Ty::Unit, Ty::Unit) => true,
            (Ty::List(a), Ty::List(b)) => a.compatible(b),
            (Ty::Option(a), Ty::Option(b)) => a.compatible(b),
            (Ty::Result(a1, a2), Ty::Result(b1, b2)) => a1.compatible(b1) && a2.compatible(b2),
            (Ty::Map(k1, v1), Ty::Map(k2, v2)) => k1.compatible(k2) && v1.compatible(v2),
            (Ty::Named(a, _), Ty::Named(b, _)) => a == b,
            (Ty::Variant { name: a, .. }, Ty::Variant { name: b, .. }) => a == b,
            (Ty::Named(a, _), Ty::Variant { name: b, .. }) => a == b,
            (Ty::Variant { name: a, .. }, Ty::Named(b, _)) => a == b,
            (Ty::Fn { params: p1, ret: r1 }, Ty::Fn { params: p2, ret: r2 }) => {
                p1.len() == p2.len()
                    && p1.iter().zip(p2.iter()).all(|(a, b)| a.compatible(b))
                    && r1.compatible(r2)
            }
            (Ty::Record { fields: f1 }, Ty::Record { fields: f2 }) => {
                // Both closed: exact match
                f1.len() == f2.len()
                    && f1.iter().zip(f2.iter()).all(|((n1, t1), (n2, t2))| n1 == n2 && t1.compatible(t2))
            }
            (Ty::OpenRecord { fields: required }, Ty::Record { fields: actual })
            | (Ty::OpenRecord { fields: required }, Ty::OpenRecord { fields: actual }) => {
                // Open parameter: all required fields must exist in the argument (by name, order-independent)
                required.iter().all(|(n1, t1)| actual.iter().any(|(n2, t2)| n1 == n2 && t1.compatible(t2)))
            }
            (Ty::Record { .. }, Ty::OpenRecord { .. }) => {
                // Closed parameter × open argument: not allowed
                false
            }
            (Ty::Tuple(a), Ty::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.compatible(y))
            }
            // Union: a concrete type is compatible with a union if it matches any member
            (Ty::Union(members), other) => members.iter().any(|m| m.compatible(other)),
            (other, Ty::Union(members)) => members.iter().any(|m| other.compatible(m)),
            _ => false,
        }
    }
}

/// Check if binding TypeVar `var` to `ty` would create an infinite type.
/// Only detects direct self-recursion: T = List[T], T = Option[T], etc.
/// Does NOT reject cases where a same-named TypeVar appears from a different scope
/// (e.g., binding A to List[(A, B)] where A is from the caller's generic context).
/// Full scope-aware occurs check requires scoped TypeVar IDs (future work).
fn occurs_in(var: &str, ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(_) => false, // bare TypeVar is never a recursive occurrence
        Ty::List(inner) | Ty::Option(inner) => matches!(inner.as_ref(), Ty::TypeVar(n) if n == var),
        Ty::Result(a, b) | Ty::Map(a, b) => {
            matches!(a.as_ref(), Ty::TypeVar(n) if n == var)
            || matches!(b.as_ref(), Ty::TypeVar(n) if n == var)
        }
        _ => false,
    }
}

/// Unify a signature type against a concrete type, collecting TypeVar bindings.
/// Returns true if the types are compatible. Unknown still accepts anything (error recovery).
pub fn unify(sig_ty: &Ty, actual_ty: &Ty, bindings: &mut std::collections::HashMap<std::string::String, Ty>) -> bool {
    // Unknown = error recovery, always accept
    if *sig_ty == Ty::Unknown || *actual_ty == Ty::Unknown {
        return true;
    }
    // TypeVar: bind or check consistency
    if let Ty::TypeVar(name) = sig_ty {
        if let Some(bound) = bindings.get(name) {
            return bound.compatible(actual_ty);
        } else {
            // Occurs check: prevent infinite types like T = List[T]
            if occurs_in(name, actual_ty) {
                return false;
            }
            bindings.insert(name.clone(), actual_ty.clone());
            return true;
        }
    }
    // When actual is a TypeVar, it represents an unresolved polymorphic type.
    // Accept it (polymorphic types are compatible with anything) but don't bind —
    // the TypeVar will be resolved when the concrete call happens.
    if matches!(actual_ty, Ty::TypeVar(_)) {
        return true;
    }
    match (sig_ty, actual_ty) {
        (Ty::List(a), Ty::List(b)) => unify(a, b, bindings),
        (Ty::Option(a), Ty::Option(b)) => unify(a, b, bindings),
        (Ty::Result(a1, a2), Ty::Result(b1, b2)) => unify(a1, b1, bindings) && unify(a2, b2, bindings),
        (Ty::Map(k1, v1), Ty::Map(k2, v2)) => unify(k1, k2, bindings) && unify(v1, v2, bindings),
        (Ty::Fn { params: p1, ret: r1 }, Ty::Fn { params: p2, ret: r2 }) => {
            if p1.len() != p2.len() { return false; }
            if !p1.iter().zip(p2.iter()).all(|(a, b)| unify(a, b, bindings)) { return false; }
            // Try direct return type unification first
            if unify(r1, r2, bindings) { return true; }
            // Effect auto-unwrap: if actual closure returns Result[X, _], try unwrapping
            if let Ty::Result(inner, _) = r2.as_ref() {
                return unify(r1, inner, bindings);
            }
            if let Ty::Result(inner, _) = r1.as_ref() {
                return unify(inner, r2, bindings);
            }
            false
        }
        (Ty::Tuple(a), Ty::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| unify(x, y, bindings))
        }
        // Named types with type args: unify each arg to bind TypeVars
        (Ty::Named(a, a_args), Ty::Named(b, b_args)) if a == b && a_args.len() == b_args.len() => {
            a_args.iter().zip(b_args.iter()).all(|(x, y)| unify(x, y, bindings))
        }
        // Union: actual type is compatible if it matches any member
        (Ty::Union(members), _) => members.iter().any(|m| unify(m, actual_ty, bindings)),
        (_, Ty::Union(members)) => members.iter().any(|m| unify(sig_ty, m, bindings)),
        _ => sig_ty.compatible(actual_ty),
    }
}

/// Substitute TypeVars in a type using the collected bindings.
pub fn substitute(ty: &Ty, bindings: &std::collections::HashMap<std::string::String, Ty>) -> Ty {
    if bindings.is_empty() {
        return ty.clone();
    }
    match ty {
        Ty::TypeVar(name) => bindings.get(name).cloned().unwrap_or(Ty::Unknown),
        Ty::Unknown => Ty::Unknown,
        Ty::List(inner) => Ty::List(Box::new(substitute(inner, bindings))),
        Ty::Option(inner) => Ty::Option(Box::new(substitute(inner, bindings))),
        Ty::Result(ok, err) => Ty::Result(Box::new(substitute(ok, bindings)), Box::new(substitute(err, bindings))),
        Ty::Map(k, v) => Ty::Map(Box::new(substitute(k, bindings)), Box::new(substitute(v, bindings))),
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|p| substitute(p, bindings)).collect(),
            ret: Box::new(substitute(ret, bindings)),
        },
        Ty::Tuple(tys) => Ty::Tuple(tys.iter().map(|t| substitute(t, bindings)).collect()),
        Ty::Record { fields } => Ty::Record {
            fields: fields.iter().map(|(n, t)| (n.clone(), substitute(t, bindings))).collect(),
        },
        Ty::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|(n, t)| (n.clone(), substitute(t, bindings))).collect(),
        },
        Ty::Union(members) => Ty::union(members.iter().map(|m| substitute(m, bindings)).collect()),
        Ty::Named(name, args) if !args.is_empty() => {
            Ty::Named(name.clone(), args.iter().map(|a| substitute(a, bindings)).collect())
        }
        _ => ty.clone(),
    }
}

/// Check if a type contains any unbound TypeVars.
pub fn contains_typevar(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(_) => true,
        Ty::List(inner) | Ty::Option(inner) => contains_typevar(inner),
        Ty::Result(a, b) | Ty::Map(a, b) => contains_typevar(a) || contains_typevar(b),
        Ty::Tuple(elems) => elems.iter().any(contains_typevar),
        Ty::Fn { params, ret } => params.iter().any(contains_typevar) || contains_typevar(ret),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| contains_typevar(t)),
        Ty::Union(members) => members.iter().any(contains_typevar),
        Ty::Named(_, args) => args.iter().any(contains_typevar),
        _ => false,
    }
}

impl TypeEnv {
    pub fn new() -> Self {
        TypeEnv {
            types: std::collections::HashMap::new(),
            functions: std::collections::HashMap::new(),
            scopes: vec![std::collections::HashMap::new()],
            current_ret: None,
            in_effect: false,
            effect_fns: std::collections::HashSet::new(),
            constructors: std::collections::HashMap::new(),
            user_modules: std::collections::HashSet::new(),
            in_do_block: false,
            in_test: false,
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
            Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Unit => true,
            Ty::List(inner) | Ty::Option(inner) => self.is_eq_inner(inner, seen),
            Ty::Result(ok, err) => self.is_eq_inner(ok, seen) && self.is_eq_inner(err, seen),
            Ty::Map(k, v) => self.is_eq_inner(k, seen) && self.is_eq_inner(v, seen),
            Ty::Tuple(tys) => tys.iter().all(|t| self.is_eq_inner(t, seen)),
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().all(|(_, t)| self.is_eq_inner(t, seen)),
            Ty::Variant { name, cases, .. } => {
                if !seen.insert(name.clone()) {
                    return true; // Recursive type — assume Eq to break cycle
                }
                cases.iter().all(|c| match &c.payload {
                    crate::types::VariantPayload::Unit => true,
                    crate::types::VariantPayload::Tuple(tys) => tys.iter().all(|t| self.is_eq_inner(t, seen)),
                    crate::types::VariantPayload::Record(fs) => fs.iter().all(|(_, t, _)| self.is_eq_inner(t, seen)),
                })
            }
            Ty::Named(name, _) => {
                if !seen.insert(name.clone()) {
                    return true; // Recursive type
                }
                if let Some(resolved) = self.types.get(name) {
                    self.is_eq_inner(resolved, seen)
                } else {
                    true
                }
            }
            Ty::Union(members) => members.iter().all(|m| self.is_eq_inner(m, seen)),
            Ty::Fn { .. } => false,
            Ty::TypeVar(_) => true,
            Ty::Unknown => true,
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
            Ty::Int | Ty::String | Ty::Bool | Ty::Unit => true,
            Ty::Float => false,
            Ty::List(inner) | Ty::Option(inner) => self.is_hash_inner(inner, seen),
            Ty::Result(ok, err) => self.is_hash_inner(ok, seen) && self.is_hash_inner(err, seen),
            Ty::Map(_, _) => false, // Maps themselves are not hashable
            Ty::Tuple(tys) => tys.iter().all(|t| self.is_hash_inner(t, seen)),
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().all(|(_, t)| self.is_hash_inner(t, seen)),
            Ty::Variant { name, cases, .. } => {
                if !seen.insert(name.clone()) {
                    return true;
                }
                cases.iter().all(|c| match &c.payload {
                    crate::types::VariantPayload::Unit => true,
                    crate::types::VariantPayload::Tuple(tys) => tys.iter().all(|t| self.is_hash_inner(t, seen)),
                    crate::types::VariantPayload::Record(fs) => fs.iter().all(|(_, t, _)| self.is_hash_inner(t, seen)),
                })
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
            Ty::Union(members) => members.iter().all(|m| self.is_hash_inner(m, seen)),
            Ty::Fn { .. } => false,
            Ty::TypeVar(_) => true,
            Ty::Unknown => true,
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
                        let mut bindings = std::collections::HashMap::new();
                        for (i, arg) in args.iter().enumerate() {
                            if let Some(name) = param_names.get(i) {
                                bindings.insert(name.clone(), arg.clone());
                            }
                        }
                        if bindings.is_empty() {
                            resolved.clone()
                        } else {
                            substitute(resolved, &bindings)
                        }
                    }
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone(),
        }
    }

    /// Collect unique TypeVar names from a type in the order they first appear
    fn collect_typevars(ty: &Ty, out: &mut Vec<std::string::String>) {
        match ty {
            Ty::TypeVar(name) => {
                if !out.contains(name) {
                    out.push(name.clone());
                }
            }
            Ty::List(inner) | Ty::Option(inner) => Self::collect_typevars(inner, out),
            Ty::Result(a, b) | Ty::Map(a, b) => {
                Self::collect_typevars(a, out);
                Self::collect_typevars(b, out);
            }
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                for (_, t) in fields {
                    Self::collect_typevars(t, out);
                }
            }
            Ty::Variant { cases, .. } => {
                for c in cases {
                    match &c.payload {
                        VariantPayload::Tuple(tys) => {
                            for t in tys { Self::collect_typevars(t, out); }
                        }
                        VariantPayload::Record(fs) => {
                            for (_, t, _) in fs { Self::collect_typevars(t, out); }
                        }
                        VariantPayload::Unit => {}
                    }
                }
            }
            Ty::Fn { params, ret } => {
                for p in params { Self::collect_typevars(p, out); }
                Self::collect_typevars(ret, out);
            }
            Ty::Tuple(tys) => {
                for t in tys { Self::collect_typevars(t, out); }
            }
            _ => {}
        }
    }
}
