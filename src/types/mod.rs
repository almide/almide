/// Internal resolved type representation for the Almide type checker.
/// Distinct from ast::TypeExpr which is a syntactic node.

mod env;
mod unify;
pub mod constructor;

pub use env::TypeEnv;
pub use unify::{unify, substitute, contains_typevar};
pub use constructor::{TypeConstructorId, TypeConstructorRegistry, Kind, AlgebraicLaw};

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
        self.any_child_recursive(&|t| matches!(t, Ty::Unknown))
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
            // Union: a concrete type is compatible with a union if it matches any member
            (Ty::Union(members), other) => members.iter().any(|m| m.compatible(other)),
            (other, Ty::Union(members)) => members.iter().any(|m| other.compatible(m)),
            _ => false,
        }
    }

    // --- HKT Foundation: Type Constructor Helpers ---

    /// Returns the type constructor identifier for this type, if applicable.
    ///
    /// This provides a uniform way to identify what "kind" of container a type is,
    /// without pattern-matching on each variant individually.
    pub fn constructor_id(&self) -> Option<TypeConstructorId> {
        match self {
            Ty::Int => Some(TypeConstructorId::Int),
            Ty::Float => Some(TypeConstructorId::Float),
            Ty::String => Some(TypeConstructorId::String),
            Ty::Bool => Some(TypeConstructorId::Bool),
            Ty::Unit => Some(TypeConstructorId::Unit),
            Ty::List(_) => Some(TypeConstructorId::List),
            Ty::Option(_) => Some(TypeConstructorId::Option),
            Ty::Result(_, _) => Some(TypeConstructorId::Result),
            Ty::Map(_, _) => Some(TypeConstructorId::Map),
            Ty::Tuple(_) => Some(TypeConstructorId::Tuple),
            Ty::Named(name, _) => Some(TypeConstructorId::UserDefined(name.clone())),
            _ => None,
        }
    }

    /// Returns the type arguments of this type constructor application.
    ///
    /// ```text
    /// List[Int]         → [Int]
    /// Result[String, E] → [String, E]
    /// Map[K, V]         → [K, V]
    /// Tuple(A, B, C)    → [A, B, C]
    /// Int               → []
    /// ```
    pub fn type_args(&self) -> Vec<&Ty> {
        match self {
            Ty::List(inner) | Ty::Option(inner) => vec![inner.as_ref()],
            Ty::Result(ok, err) | Ty::Map(ok, err) => vec![ok.as_ref(), err.as_ref()],
            Ty::Tuple(tys) => tys.iter().collect(),
            Ty::Named(_, args) => args.iter().collect(),
            _ => vec![],
        }
    }

    /// Returns an iterator over all direct child types (for recursive traversal).
    ///
    /// This unifies the repeated pattern across contains_unknown, is_eq, is_hash,
    /// occurs_in, contains_typevar, collect_typevars, etc.
    pub fn children(&self) -> Vec<&Ty> {
        match self {
            // Leaf types — no children
            Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Unit
            | Ty::TypeVar(_) | Ty::Unknown => vec![],

            // Single-param containers
            Ty::List(inner) | Ty::Option(inner) => vec![inner.as_ref()],

            // Dual-param containers
            Ty::Result(a, b) | Ty::Map(a, b) => vec![a.as_ref(), b.as_ref()],

            // Variable-arity
            Ty::Tuple(tys) => tys.iter().collect(),
            Ty::Named(_, args) => args.iter().collect(),
            Ty::Union(members) => members.iter().collect(),

            // Structural types
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                fields.iter().map(|(_, t)| t).collect()
            }

            // Function type
            Ty::Fn { params, ret } => {
                let mut children: Vec<&Ty> = params.iter().collect();
                children.push(ret.as_ref());
                children
            }

            // Variant — children are inside payloads
            Ty::Variant { cases, .. } => {
                let mut children = Vec::new();
                for c in cases {
                    match &c.payload {
                        VariantPayload::Unit => {}
                        VariantPayload::Tuple(tys) => children.extend(tys.iter()),
                        VariantPayload::Record(fs) => children.extend(fs.iter().map(|(_, t, _)| t)),
                    }
                }
                children
            }
        }
    }

    /// Apply a transformation to all direct child types, producing a new Ty.
    ///
    /// This is the "map" counterpart to `children()`. Together they enable
    /// uniform recursive operations without repeating match arms.
    pub fn map_children<F>(&self, f: &F) -> Ty
    where
        F: Fn(&Ty) -> Ty,
    {
        match self {
            Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Unit
            | Ty::TypeVar(_) | Ty::Unknown => self.clone(),

            Ty::List(inner) => Ty::list(f(inner)),
            Ty::Option(inner) => Ty::option(f(inner)),
            Ty::Result(ok, err) => Ty::result(f(ok), f(err)),
            Ty::Map(k, v) => Ty::map_of(f(k), f(v)),

            Ty::Tuple(tys) => Ty::Tuple(tys.iter().map(f).collect()),
            Ty::Named(name, args) => Ty::Named(name.clone(), args.iter().map(f).collect()),
            Ty::Union(members) => Ty::union(members.iter().map(f).collect()),

            Ty::Record { fields } => Ty::Record {
                fields: fields.iter().map(|(n, t)| (n.clone(), f(t))).collect(),
            },
            Ty::OpenRecord { fields } => Ty::OpenRecord {
                fields: fields.iter().map(|(n, t)| (n.clone(), f(t))).collect(),
            },

            Ty::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(f).collect(),
                ret: Box::new(f(ret)),
            },

            Ty::Variant { name, cases } => Ty::Variant {
                name: name.clone(),
                cases: cases.iter().map(|c| VariantCase {
                    name: c.name.clone(),
                    payload: match &c.payload {
                        VariantPayload::Unit => VariantPayload::Unit,
                        VariantPayload::Tuple(tys) => VariantPayload::Tuple(tys.iter().map(f).collect()),
                        VariantPayload::Record(fs) => VariantPayload::Record(
                            fs.iter().map(|(n, t, d)| (n.clone(), f(t), d.clone())).collect(),
                        ),
                    },
                }).collect(),
            },
        }
    }

    /// Apply a mutable transformation to all direct child types, producing a new Ty.
    ///
    /// Like `map_children` but accepts `FnMut`, enabling use with closures
    /// that capture `&mut self` (e.g., type checker's `instantiate_inner`).
    pub fn map_children_mut<F>(&self, f: &mut F) -> Ty
    where
        F: FnMut(&Ty) -> Ty,
    {
        match self {
            Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Unit
            | Ty::TypeVar(_) | Ty::Unknown => self.clone(),

            Ty::List(inner) => Ty::list(f(inner)),
            Ty::Option(inner) => Ty::option(f(inner)),
            Ty::Result(ok, err) => Ty::result(f(ok), f(err)),
            Ty::Map(k, v) => Ty::map_of(f(k), f(v)),

            Ty::Tuple(tys) => Ty::Tuple(tys.iter().map(|t| f(t)).collect()),
            Ty::Named(name, args) => Ty::Named(name.clone(), args.iter().map(|t| f(t)).collect()),
            Ty::Union(members) => Ty::union(members.iter().map(|t| f(t)).collect()),

            Ty::Record { fields } => Ty::Record {
                fields: fields.iter().map(|(n, t)| (n.clone(), f(t))).collect(),
            },
            Ty::OpenRecord { fields } => Ty::OpenRecord {
                fields: fields.iter().map(|(n, t)| (n.clone(), f(t))).collect(),
            },

            Ty::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(|t| f(t)).collect(),
                ret: Box::new(f(ret)),
            },

            Ty::Variant { name, cases } => Ty::Variant {
                name: name.clone(),
                cases: cases.iter().map(|c| VariantCase {
                    name: c.name.clone(),
                    payload: match &c.payload {
                        VariantPayload::Unit => VariantPayload::Unit,
                        VariantPayload::Tuple(tys) => VariantPayload::Tuple(tys.iter().map(|t| f(t)).collect()),
                        VariantPayload::Record(fs) => VariantPayload::Record(
                            fs.iter().map(|(n, t, d)| (n.clone(), f(t), d.clone())).collect(),
                        ),
                    },
                }).collect(),
            },
        }
    }

    /// Check if any child type (recursively) satisfies a predicate.
    ///
    /// Replaces the repeated pattern:
    /// ```text
    /// match ty {
    ///     Ty::List(inner) | Ty::Option(inner) => pred(inner),
    ///     Ty::Result(a, b) | Ty::Map(a, b) => pred(a) || pred(b),
    ///     ...
    /// }
    /// ```
    pub fn any_child_recursive<F>(&self, pred: &F) -> bool
    where
        F: Fn(&Ty) -> bool,
    {
        if pred(self) {
            return true;
        }
        self.children().into_iter().any(|child| child.any_child_recursive(pred))
    }

    /// Check if all child types (recursively) satisfy a predicate.
    pub fn all_children_recursive<F>(&self, pred: &F) -> bool
    where
        F: Fn(&Ty) -> bool,
    {
        if !pred(self) {
            return false;
        }
        self.children().into_iter().all(|child| child.all_children_recursive(pred))
    }

    /// Returns true if this type is a parameterized container (List, Option, Result, Map).
    pub fn is_container(&self) -> bool {
        matches!(self, Ty::List(_) | Ty::Option(_) | Ty::Result(_, _) | Ty::Map(_, _))
    }

    /// Returns the constructor name for display/debug purposes.
    pub fn constructor_name(&self) -> Option<&str> {
        match self {
            Ty::Int => Some("Int"),
            Ty::Float => Some("Float"),
            Ty::String => Some("String"),
            Ty::Bool => Some("Bool"),
            Ty::Unit => Some("Unit"),
            Ty::List(_) => Some("List"),
            Ty::Option(_) => Some("Option"),
            Ty::Result(_, _) => Some("Result"),
            Ty::Map(_, _) => Some("Map"),
            Ty::Tuple(_) => Some("Tuple"),
            Ty::Named(name, _) => Some(name.as_str()),
            _ => None,
        }
    }

    // ── Smart constructors (Phase 4: Ty unification prep) ──
    // Use these instead of Ty::list(...) etc.
    // When Ty is unified to Applied, only these functions need to change.

    /// Construct List[T]
    #[inline]
    pub fn list(inner: Ty) -> Ty { Ty::List(Box::new(inner)) }

    /// Construct Option[T]
    #[inline]
    pub fn option(inner: Ty) -> Ty { Ty::Option(Box::new(inner)) }

    /// Construct Result[T, E]
    #[inline]
    pub fn result(ok: Ty, err: Ty) -> Ty { Ty::Result(Box::new(ok), Box::new(err)) }

    /// Construct Map[K, V]
    #[inline]
    pub fn map_of(key: Ty, val: Ty) -> Ty { Ty::Map(Box::new(key), Box::new(val)) }

    // ── Accessors (Phase 4: uniform access to container type args) ──

    /// Get the inner type of a single-param container (List or Option).
    /// Returns None for non-container types.
    pub fn inner(&self) -> Option<&Ty> {
        match self {
            Ty::List(inner) | Ty::Option(inner) => Some(inner),
            _ => None,
        }
    }

    /// Get the two type args of a dual-param container (Result or Map).
    pub fn inner2(&self) -> Option<(&Ty, &Ty)> {
        match self {
            Ty::Result(a, b) | Ty::Map(a, b) => Some((a, b)),
            _ => None,
        }
    }

    /// Check if this is a List type.
    pub fn is_list(&self) -> bool { matches!(self, Ty::List(_)) }
    /// Check if this is an Option type.
    pub fn is_option(&self) -> bool { matches!(self, Ty::Option(_)) }
    /// Check if this is a Result type.
    pub fn is_result(&self) -> bool { matches!(self, Ty::Result(_, _)) }
    /// Check if this is a Map type.
    pub fn is_map(&self) -> bool { matches!(self, Ty::Map(_, _)) }
    /// Check if this is a function type.
    pub fn is_fn(&self) -> bool { matches!(self, Ty::Fn { .. }) }
}
