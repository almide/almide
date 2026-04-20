/// Type constructor infrastructure for HKT foundation.
///
/// This module provides a unified representation of type constructors (List, Option, Result, etc.)
/// with kind information and algebraic law annotations. The Ty enum itself is not changed —
/// these types serve as an abstraction layer that enables uniform operations across all
/// container types, including user-defined ones.
///
/// Design principle: "Users see simplicity, compiler sees power."
/// No HKT syntax is exposed to users. This infrastructure enables:
/// - Stream Fusion (map |> filter |> fold → single loop)
/// - Uniform type traversal (eliminate repeated match arms)
/// - Future Trait system with full expressiveness from day one

/// Identifies a type constructor.
///
/// Built-in constructors have dedicated variants for fast matching.
/// User-defined types use `UserDefined(name)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TypeConstructorId {
    // kind: *
    Int,
    Float,
    String,
    Bool,
    Unit,
    Bytes,
    Matrix,

    // kind: * -> *
    List,
    Option,
    Set,

    // kind: * -> * -> *
    Result,
    Map,

    // kind: *^n -> * (arity determined at definition)
    Tuple,

    // User-defined: kind determined at definition
    UserDefined(std::string::String),
}

/// Kind — the "type of a type constructor".
///
/// ```text
/// Int    : *           (concrete type)
/// List   : * -> *      (takes one type, produces a type)
/// Result : * -> * -> * (takes two types, produces a type)
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// `*` — a concrete type (Int, String, Bool, etc.)
    Star,
    /// `k1 -> k2` — a type constructor that takes a type of kind k1 and produces kind k2
    Arrow(Box<Kind>, Box<Kind>),
}

impl Kind {
    /// `* -> *` — single-parameter type constructor (List, Option)
    pub fn star_to_star() -> Kind {
        Kind::Arrow(Box::new(Kind::Star), Box::new(Kind::Star))
    }

    /// `* -> * -> *` — two-parameter type constructor (Result, Map)
    pub fn star2_to_star() -> Kind {
        Kind::Arrow(
            Box::new(Kind::Star),
            Box::new(Kind::Arrow(Box::new(Kind::Star), Box::new(Kind::Star))),
        )
    }

    /// Returns the arity (number of type parameters) of this kind.
    pub fn arity(&self) -> usize {
        match self {
            Kind::Star => 0,
            Kind::Arrow(_, rest) => 1 + rest.arity(),
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Kind::Star => write!(f, "*"),
            Kind::Arrow(from, to) => {
                if matches!(from.as_ref(), Kind::Arrow(_, _)) {
                    write!(f, "({}) -> {}", from, to)
                } else {
                    write!(f, "{} -> {}", from, to)
                }
            }
        }
    }
}

/// Algebraic laws that a type constructor satisfies.
///
/// These laws enable the compiler to perform optimizations that are
/// mathematically guaranteed to preserve semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlgebraicLaw {
    /// `map(f) >> map(g) = map(f >> g)` — fuse consecutive maps
    FunctorComposition,
    /// `map(id) = id` — eliminate identity maps
    FunctorIdentity,
    /// `filter(p) >> filter(q) = filter(x => p(x) && q(x))` — fuse consecutive filters
    FilterComposition,
    /// `map(f) >> fold(init, g) = fold(init, (acc, x) => g(acc, f(x)))` — fuse map into fold
    MapFoldFusion,
    /// `map(f) >> filter(p) = filter_map(x => { let y = f(x); if p(y) { some(y) } else { none } })`
    MapFilterFusion,
    /// `flat_map(f) >> flat_map(g) = flat_map(x => f(x).flat_map(g))` — monad associativity
    MonadAssociativity,
}

/// Metadata for a type constructor.
#[derive(Debug, Clone)]
pub struct TypeConstructorInfo {
    pub id: TypeConstructorId,
    pub name: std::string::String,
    pub kind: Kind,
    pub laws: Vec<AlgebraicLaw>,
}

/// Registry of all known type constructors and their properties.
#[derive(Debug, Clone)]
pub struct TypeConstructorRegistry {
    constructors: Vec<TypeConstructorInfo>,
    by_name: std::collections::HashMap<std::string::String, usize>,
}

impl TypeConstructorRegistry {
    /// Create a registry pre-populated with all built-in type constructors.
    pub fn new() -> Self {
        let mut reg = TypeConstructorRegistry {
            constructors: Vec::new(),
            by_name: std::collections::HashMap::new(),
        };
        reg.register_builtins();
        reg
    }

    fn register_builtins(&mut self) {
        use AlgebraicLaw::*;

        // Concrete types (kind: *)
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::Int,
            name: "Int".into(),
            kind: Kind::Star,
            laws: vec![],
        });
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::Float,
            name: "Float".into(),
            kind: Kind::Star,
            laws: vec![],
        });
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::String,
            name: "String".into(),
            kind: Kind::Star,
            laws: vec![],
        });
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::Bool,
            name: "Bool".into(),
            kind: Kind::Star,
            laws: vec![],
        });
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::Unit,
            name: "Unit".into(),
            kind: Kind::Star,
            laws: vec![],
        });
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::Bytes,
            name: "Bytes".into(),
            kind: Kind::Star,
            laws: vec![],
        });
        // Matrix: * -> * — parametric dtype (Sized Numeric Types P4).
        // Pre-arc: `Matrix` was non-parametric (kind *). Now `Matrix[T]`
        // parses and resolves to `Applied(Matrix, [T])`; bare `Matrix`
        // keeps compatibility via the `compatible(Ty::Matrix,
        // Applied(Matrix, [Float]))` rule.
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::Matrix,
            name: "Matrix".into(),
            kind: Kind::star_to_star(),
            laws: vec![],
        });

        // List: * -> * — satisfies Functor, Filterable, Foldable
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::List,
            name: "List".into(),
            kind: Kind::star_to_star(),
            laws: vec![
                FunctorComposition,
                FunctorIdentity,
                FilterComposition,
                MapFoldFusion,
                MapFilterFusion,
            ],
        });

        // Option: * -> * — satisfies Functor, Monad
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::Option,
            name: "Option".into(),
            kind: Kind::star_to_star(),
            laws: vec![
                FunctorComposition,
                FunctorIdentity,
                MonadAssociativity,
            ],
        });

        // Result: * -> * -> * — satisfies Functor (over ok type)
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::Result,
            name: "Result".into(),
            kind: Kind::star2_to_star(),
            laws: vec![
                FunctorComposition,
                FunctorIdentity,
            ],
        });

        // Set: * -> * — no standard algebraic laws for stream fusion
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::Set,
            name: "Set".into(),
            kind: Kind::star_to_star(),
            laws: vec![],
        });

        // Map: * -> * -> * — no standard algebraic laws for stream fusion
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::Map,
            name: "Map".into(),
            kind: Kind::star2_to_star(),
            laws: vec![],
        });

        // Tuple: variable arity — no algebraic laws
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::Tuple,
            name: "Tuple".into(),
            kind: Kind::Star, // simplified — actual arity varies
            laws: vec![],
        });
    }

    fn register(&mut self, info: TypeConstructorInfo) {
        let idx = self.constructors.len();
        self.by_name.insert(info.name.clone(), idx);
        self.constructors.push(info);
    }

    /// Register a user-defined type constructor.
    pub fn register_user_type(&mut self, name: &str, arity: usize) {
        if self.by_name.contains_key(name) {
            return; // Already registered
        }
        let kind = (0..arity).fold(Kind::Star, |acc, _| {
            Kind::Arrow(Box::new(Kind::Star), Box::new(acc))
        });
        self.register(TypeConstructorInfo {
            id: TypeConstructorId::UserDefined(name.to_string()),
            name: name.to_string(),
            kind,
            laws: vec![],
        });
    }

    /// Look up a type constructor by name.
    pub fn lookup(&self, name: &str) -> Option<&TypeConstructorInfo> {
        self.by_name.get(name).map(|&idx| &self.constructors[idx])
    }

    /// Get all laws that apply to a type constructor.
    pub fn laws_for(&self, name: &str) -> &[AlgebraicLaw] {
        self.lookup(name).map_or(&[], |info| &info.laws)
    }

    /// Get the kind of a type constructor.
    pub fn kind_of(&self, name: &str) -> Option<&Kind> {
        self.lookup(name).map(|info| &info.kind)
    }

    /// Check if a type constructor satisfies a specific algebraic law.
    pub fn satisfies(&self, name: &str, law: AlgebraicLaw) -> bool {
        self.laws_for(name).contains(&law)
    }
}

impl std::fmt::Display for TypeConstructorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeConstructorId::Int => write!(f, "Int"),
            TypeConstructorId::Float => write!(f, "Float"),
            TypeConstructorId::String => write!(f, "String"),
            TypeConstructorId::Bool => write!(f, "Bool"),
            TypeConstructorId::Unit => write!(f, "Unit"),
            TypeConstructorId::Bytes => write!(f, "Bytes"),
            TypeConstructorId::Matrix => write!(f, "Matrix"),
            TypeConstructorId::List => write!(f, "List"),
            TypeConstructorId::Option => write!(f, "Option"),
            TypeConstructorId::Set => write!(f, "Set"),
            TypeConstructorId::Result => write!(f, "Result"),
            TypeConstructorId::Map => write!(f, "Map"),
            TypeConstructorId::Tuple => write!(f, "Tuple"),
            TypeConstructorId::UserDefined(name) => write!(f, "{}", name),
        }
    }
}

impl Default for TypeConstructorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
