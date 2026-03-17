/// Internal resolved type representation for the Almide type checker.
/// Distinct from ast::TypeExpr which is a syntactic node.

mod env;
mod unify;

pub use env::TypeEnv;
pub use unify::{unify, substitute, contains_typevar};

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
            1 => match unique.into_iter().next() { Some(t) => t, None => Ty::Unit },
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
            // Named ↔ Record: Named types are compatible with their structural expansion
            // (this handles the case where one side is resolve_named'd and the other isn't)
            (Ty::Named(_, _), Ty::Record { .. }) | (Ty::Record { .. }, Ty::Named(_, _)) => true,
            (Ty::Named(_, _), Ty::Variant { .. }) | (Ty::Variant { .. }, Ty::Named(_, _)) => true,
            // Union: a concrete type is compatible with a union if it matches any member
            (Ty::Union(members), other) => members.iter().any(|m| m.compatible(other)),
            (other, Ty::Union(members)) => members.iter().any(|m| other.compatible(m)),
            _ => false,
        }
    }
}
