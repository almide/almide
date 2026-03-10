/// Internal resolved type representation for the Almide type checker.
/// Distinct from ast::TypeExpr which is a syntactic node.

#[derive(Debug, Clone, PartialEq)]
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
    Variant { name: std::string::String, cases: Vec<VariantCase> },
    Fn { params: Vec<Ty>, ret: Box<Ty> },
    Tuple(Vec<Ty>),
    Named(std::string::String),
    /// Type variable for user-defined generics (e.g., T, U, A, B)
    TypeVar(std::string::String),
    /// Error recovery — unifies with everything to prevent cascade errors
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariantCase {
    pub name: std::string::String,
    pub payload: VariantPayload,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VariantPayload {
    Unit,
    Tuple(Vec<Ty>),
    Record(Vec<(std::string::String, Ty)>),
}

#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<(std::string::String, Ty)>,
    pub ret: Ty,
    pub is_effect: bool,
    #[allow(dead_code)]
    pub generics: Vec<std::string::String>,
}

/// Convenience macro for creating FnSig without generics (stdlib functions)
#[macro_export]
macro_rules! fn_sig {
    (params: $params:expr, ret: $ret:expr, is_effect: $eff:expr) => {
        FnSig { params: $params, ret: $ret, is_effect: $eff, generics: vec![] }
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
            Ty::Variant { name, .. } => name.clone(),
            Ty::Fn { params, ret } => {
                let ps: Vec<_> = params.iter().map(|t| t.display()).collect();
                format!("fn({}) -> {}", ps.join(", "), ret.display())
            }
            Ty::Tuple(tys) => {
                let ts: Vec<_> = tys.iter().map(|t| t.display()).collect();
                format!("({})", ts.join(", "))
            }
            Ty::Named(n) => n.clone(),
            Ty::TypeVar(n) => n.clone(),
            Ty::Unknown => "Unknown".into(),
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
            (Ty::Named(a), Ty::Named(b)) => a == b,
            (Ty::Variant { name: a, .. }, Ty::Variant { name: b, .. }) => a == b,
            (Ty::Named(a), Ty::Variant { name: b, .. }) => a == b,
            (Ty::Variant { name: a, .. }, Ty::Named(b)) => a == b,
            (Ty::Fn { params: p1, ret: r1 }, Ty::Fn { params: p2, ret: r2 }) => {
                p1.len() == p2.len()
                    && p1.iter().zip(p2.iter()).all(|(a, b)| a.compatible(b))
                    && r1.compatible(r2)
            }
            (Ty::Record { fields: f1 }, Ty::Record { fields: f2 }) => {
                f1.len() == f2.len()
                    && f1.iter().zip(f2.iter()).all(|((n1, t1), (n2, t2))| n1 == n2 && t1.compatible(t2))
            }
            (Ty::Tuple(a), Ty::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.compatible(y))
            }
            _ => false,
        }
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
            bindings.insert(name.clone(), actual_ty.clone());
            return true;
        }
    }
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
        _ => ty.clone(),
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

    pub fn lookup_var(&self, name: &str) -> Option<&Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    pub fn resolve_named(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Named(name) => {
                if let Some(resolved) = self.types.get(name) {
                    resolved.clone()
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone(),
        }
    }
}
