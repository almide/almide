/// Internal resolved type representation for the Almide type checker.
/// Distinct from ast::TypeExpr which is a syntactic node.

mod env;
mod unify;
pub mod constructor;

pub use env::TypeEnv;
pub use unify::{unify, substitute, contains_typevar};
pub use constructor::{TypeConstructorId, TypeConstructorRegistry, Kind, AlgebraicLaw};
use crate::intern::Sym;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Ty {
    Int,
    Float,
    String,
    Bool,
    Unit,
    Bytes,
    Matrix,
    /// Parameterized type constructor: List[T], Option[T], Result[T,E], Map[K,V], Set[T], etc.
    /// Phase 4 of HKT Foundation — unifies all container types.
    Applied(constructor::TypeConstructorId, Vec<Ty>),
    Record { fields: Vec<(Sym, Ty)> },
    OpenRecord { fields: Vec<(Sym, Ty)> },
    Variant { name: Sym, cases: Vec<VariantCase> },
    Fn { params: Vec<Ty>, ret: Box<Ty> },
    Tuple(Vec<Ty>),
    Named(Sym, Vec<Ty>),
    /// Inline union type (e.g., Int | String). Members are sorted and deduplicated.
    Union(Vec<Ty>),
    /// Type variable for user-defined generics (e.g., T, U, A, B)
    TypeVar(Sym),
    /// Bottom type — returned by functions that never return (process.exit, panic).
    /// Unifies with any type (subtype of all types).
    Never,
    /// Error recovery — unifies with everything to prevent cascade errors
    Unknown,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VariantCase {
    pub name: Sym,
    pub payload: VariantPayload,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum VariantPayload {
    Unit,
    Tuple(Vec<Ty>),
    Record(Vec<(Sym, Ty, Option<crate::ast::Expr>)>),
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

/// A protocol definition (user-defined or built-in convention).
/// Protocols declare a set of methods that conforming types must implement.
#[derive(Debug, Clone)]
pub struct ProtocolDef {
    pub name: Sym,
    pub generics: Vec<Sym>,
    pub methods: Vec<ProtocolMethodSig>,
}

/// A single method signature within a protocol definition.
/// `Self` in parameters/return type is represented as `Ty::TypeVar("Self")`.
#[derive(Debug, Clone)]
pub struct ProtocolMethodSig {
    pub name: Sym,
    pub params: Vec<(Sym, Ty)>,
    pub ret: Ty,
    pub is_effect: bool,
}

#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<(Sym, Ty)>,
    pub ret: Ty,
    pub is_effect: bool,
    pub generics: Vec<Sym>,
    /// Structural bounds for generics: TypeVar name → OpenRecord constraint type
    pub structural_bounds: std::collections::HashMap<Sym, Ty>,
    /// Protocol bounds for generics: TypeVar name → list of protocol names
    pub protocol_bounds: std::collections::HashMap<Sym, Vec<Sym>>,
}

/// Convenience macro for creating FnSig without generics (stdlib functions)
#[macro_export]
macro_rules! fn_sig {
    (params: $params:expr, ret: $ret:expr, is_effect: $eff:expr) => {
        FnSig { params: $params, ret: $ret, is_effect: $eff, generics: vec![], structural_bounds: std::collections::HashMap::new(), protocol_bounds: std::collections::HashMap::new() }
    };
}

impl FnSig {
    /// Format parameter list as "name: Type, name: Type, ..."
    pub fn format_params(&self) -> std::string::String {
        self.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ")
    }
}

impl Ty {
    pub fn display(&self) -> String {
        match self {
            Ty::Int => "Int".into(),
            Ty::Float => "Float".into(),
            Ty::String => "String".into(),
            Ty::Bool => "Bool".into(),
            Ty::Unit => "Unit".into(),
            Ty::Bytes => "Bytes".into(),
            Ty::Matrix => "Matrix".into(),
            Ty::Applied(id, args) => {
                let name = match id {
                    TypeConstructorId::List => "List",
                    TypeConstructorId::Option => "Option",
                    TypeConstructorId::Set => "Set",
                    TypeConstructorId::Result => "Result",
                    TypeConstructorId::Map => "Map",
                    TypeConstructorId::Tuple => "Tuple",
                    TypeConstructorId::UserDefined(n) => n.as_str(),
                    _ => return id.to_string(),
                };
                if args.is_empty() {
                    name.to_string()
                } else {
                    let ts: Vec<_> = args.iter().map(|t| t.display()).collect();
                    format!("{}[{}]", name, ts.join(", "))
                }
            }
            Ty::Record { fields } => {
                let fs: Vec<_> = fields.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect();
                format!("{{ {} }}", fs.join(", "))
            }
            Ty::OpenRecord { fields } => {
                let fs: Vec<_> = fields.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect();
                format!("{{ {}, .. }}", fs.join(", "))
            }
            Ty::Variant { name, .. } => name.to_string(),
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
                    n.to_string()
                } else {
                    let ts: Vec<_> = args.iter().map(|t| t.display()).collect();
                    format!("{}[{}]", n, ts.join(", "))
                }
            }
            Ty::Union(members) => {
                let ms: Vec<_> = members.iter().map(|t| t.display()).collect();
                ms.join(" | ")
            }
            Ty::TypeVar(n) => n.to_string(),
            Ty::Never => "Never".into(),
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

    /// Check if two types are compatible (Unknown and Never match everything)
    pub fn compatible(&self, other: &Ty) -> bool {
        if *self == Ty::Unknown || *other == Ty::Unknown
            || *self == Ty::Never || *other == Ty::Never {
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
            (Ty::Bytes, Ty::Bytes) => true,
            (Ty::Matrix, Ty::Matrix) => true,
            (Ty::Applied(id1, args1), Ty::Applied(id2, args2)) if id1 == id2 && args1.len() == args2.len() => {
                args1.iter().zip(args2.iter()).all(|(a, b)| a.compatible(b))
            }
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
            Ty::Bytes => Some(TypeConstructorId::Bytes),
            Ty::Matrix => Some(TypeConstructorId::Matrix),
            Ty::Applied(id, _) => Some(id.clone()),
            Ty::Tuple(_) => Some(TypeConstructorId::Tuple),
            Ty::Named(name, _) => Some(TypeConstructorId::UserDefined(name.to_string())),
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
            Ty::Applied(_, args) => args.iter().collect(),
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
            Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Unit | Ty::Bytes | Ty::Matrix
            | Ty::TypeVar(_) | Ty::Never | Ty::Unknown => vec![],

            // Parameterized types (List, Option, Result, Map, user-defined)
            Ty::Applied(_, args) => args.iter().collect(),

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
            Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Unit | Ty::Bytes | Ty::Matrix
            | Ty::TypeVar(_) | Ty::Never | Ty::Unknown => self.clone(),

            Ty::Applied(id, args) => Ty::Applied(id.clone(), args.iter().map(|a| f(a)).collect()),

            Ty::Tuple(tys) => Ty::Tuple(tys.iter().map(f).collect()),
            Ty::Named(name, args) => Ty::Named(*name, args.iter().map(f).collect()),
            Ty::Union(members) => Ty::union(members.iter().map(f).collect()),

            Ty::Record { fields } => Ty::Record {
                fields: fields.iter().map(|(n, t)| (*n, f(t))).collect(),
            },
            Ty::OpenRecord { fields } => Ty::OpenRecord {
                fields: fields.iter().map(|(n, t)| (*n, f(t))).collect(),
            },

            Ty::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(f).collect(),
                ret: Box::new(f(ret)),
            },

            Ty::Variant { name, cases } => Ty::Variant {
                name: *name,
                cases: cases.iter().map(|c| VariantCase {
                    name: c.name,
                    payload: match &c.payload {
                        VariantPayload::Unit => VariantPayload::Unit,
                        VariantPayload::Tuple(tys) => VariantPayload::Tuple(tys.iter().map(f).collect()),
                        VariantPayload::Record(fs) => VariantPayload::Record(
                            fs.iter().map(|(n, t, d)| (*n, f(t), d.clone())).collect(),
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
            Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Unit | Ty::Bytes | Ty::Matrix
            | Ty::TypeVar(_) | Ty::Never | Ty::Unknown => self.clone(),

            Ty::Applied(id, args) => Ty::Applied(id.clone(), args.iter().map(|a| f(a)).collect()),

            Ty::Tuple(tys) => Ty::Tuple(tys.iter().map(|t| f(t)).collect()),
            Ty::Named(name, args) => Ty::Named(*name, args.iter().map(|t| f(t)).collect()),
            Ty::Union(members) => Ty::union(members.iter().map(|t| f(t)).collect()),

            Ty::Record { fields } => Ty::Record {
                fields: fields.iter().map(|(n, t)| (*n, f(t))).collect(),
            },
            Ty::OpenRecord { fields } => Ty::OpenRecord {
                fields: fields.iter().map(|(n, t)| (*n, f(t))).collect(),
            },

            Ty::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(|t| f(t)).collect(),
                ret: Box::new(f(ret)),
            },

            Ty::Variant { name, cases } => Ty::Variant {
                name: *name,
                cases: cases.iter().map(|c| VariantCase {
                    name: c.name,
                    payload: match &c.payload {
                        VariantPayload::Unit => VariantPayload::Unit,
                        VariantPayload::Tuple(tys) => VariantPayload::Tuple(tys.iter().map(|t| f(t)).collect()),
                        VariantPayload::Record(fs) => VariantPayload::Record(
                            fs.iter().map(|(n, t, d)| (*n, f(t), d.clone())).collect(),
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
        matches!(self, Ty::Applied(TypeConstructorId::List | TypeConstructorId::Option | TypeConstructorId::Set | TypeConstructorId::Result | TypeConstructorId::Map, _))
    }

    /// Returns the constructor name for display/debug purposes.
    pub fn constructor_name(&self) -> Option<&str> {
        match self {
            Ty::Int => Some("Int"),
            Ty::Float => Some("Float"),
            Ty::String => Some("String"),
            Ty::Bool => Some("Bool"),
            Ty::Unit => Some("Unit"),
            Ty::Bytes => Some("Bytes"),
            Ty::Matrix => Some("Matrix"),
            Ty::Applied(id, _) => Some(match id {
                TypeConstructorId::List => "List",
                TypeConstructorId::Option => "Option",
                TypeConstructorId::Set => "Set",
                TypeConstructorId::Result => "Result",
                TypeConstructorId::Map => "Map",
                TypeConstructorId::Tuple => "Tuple",
                TypeConstructorId::Int => "Int",
                TypeConstructorId::Float => "Float",
                TypeConstructorId::String => "String",
                TypeConstructorId::Bool => "Bool",
                TypeConstructorId::Unit => "Unit",
                TypeConstructorId::Bytes => "Bytes",
                TypeConstructorId::Matrix => "Matrix",
                TypeConstructorId::UserDefined(n) => return Some(n.as_str()),
            }),
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
    pub fn list(inner: Ty) -> Ty { Ty::Applied(TypeConstructorId::List, vec![inner]) }

    /// Construct Option[T]
    #[inline]
    pub fn option(inner: Ty) -> Ty { Ty::Applied(TypeConstructorId::Option, vec![inner]) }

    /// Construct Result[T, E]
    #[inline]
    pub fn result(ok: Ty, err: Ty) -> Ty { Ty::Applied(TypeConstructorId::Result, vec![ok, err]) }

    /// Construct Map[K, V]
    #[inline]
    pub fn map_of(key: Ty, val: Ty) -> Ty { Ty::Applied(TypeConstructorId::Map, vec![key, val]) }

    /// Construct Set[T]
    #[inline]
    pub fn set_of(elem: Ty) -> Ty { Ty::Applied(TypeConstructorId::Set, vec![elem]) }

    // ── Accessors (Phase 4: uniform access to container type args) ──

    /// Get the inner type of a single-param container (List, Option, or Set).
    /// Returns None for non-container types.
    pub fn inner(&self) -> Option<&Ty> {
        match self {
            Ty::Applied(TypeConstructorId::List, args) | Ty::Applied(TypeConstructorId::Option, args) | Ty::Applied(TypeConstructorId::Set, args) if args.len() == 1 => Some(&args[0]),
            _ => None,
        }
    }

    /// Get the two type args of a dual-param container (Result or Map).
    pub fn inner2(&self) -> Option<(&Ty, &Ty)> {
        match self {
            Ty::Applied(TypeConstructorId::Result, args) | Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => Some((&args[0], &args[1])),
            _ => None,
        }
    }

    /// Check if this is a List type.
    pub fn is_list(&self) -> bool { matches!(self, Ty::Applied(TypeConstructorId::List, _)) }
    /// Check if this is an Option type.
    pub fn is_option(&self) -> bool { matches!(self, Ty::Applied(TypeConstructorId::Option, _)) }
    /// Check if this is a Result type.
    pub fn is_result(&self) -> bool { matches!(self, Ty::Applied(TypeConstructorId::Result, _)) }
    /// Check if this is a Map type.
    pub fn is_map(&self) -> bool { matches!(self, Ty::Applied(TypeConstructorId::Map, _)) }
    /// Check if this is a Set type.
    pub fn is_set(&self) -> bool { matches!(self, Ty::Applied(TypeConstructorId::Set, _)) }
    /// Check if this is a function type.
    pub fn is_fn(&self) -> bool { matches!(self, Ty::Fn { .. }) }
    /// Extract the inner type T from Option[T]. Returns None if not an Option.
    pub fn option_inner(&self) -> Option<Ty> {
        match self {
            Ty::Applied(TypeConstructorId::Option, args) if !args.is_empty() => Some(args[0].clone()),
            _ => None,
        }
    }
    /// Extract the Ok type T from Result[T, E]. Returns None if not a Result.
    pub fn result_ok_ty(&self) -> Option<Ty> {
        match self {
            Ty::Applied(TypeConstructorId::Result, args) if !args.is_empty() => Some(args[0].clone()),
            _ => None,
        }
    }
}
