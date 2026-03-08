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
            Ty::Unknown => "Unknown".into(),
        }
    }

    /// Check if two types are compatible (Unknown matches everything)
    pub fn compatible(&self, other: &Ty) -> bool {
        if *self == Ty::Unknown || *other == Ty::Unknown {
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
