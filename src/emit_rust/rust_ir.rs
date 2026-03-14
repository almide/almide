/// RustIR — typed intermediate representation for Rust code generation.
///
/// Principles:
/// 1. **All decisions here** — clone, borrow, ?, Ok-wrap, mut, type annotations
///    are encoded in the data structure, not in the renderer.
/// 2. **Renderer is trivial** — pure pattern match → string, no conditionals.
/// 3. **IR types carry full information** — no looking up external state during render.

// ── Expressions ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    // Literals
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Unit,

    // Variables
    Var(String),

    // Operators
    BinOp { op: &'static str, left: Box<Expr>, right: Box<Expr> },
    UnOp { op: &'static str, operand: Box<Expr> },

    // Calls
    Call { func: String, args: Vec<Expr> },
    MethodCall { recv: Box<Expr>, method: String, args: Vec<Expr> },
    Macro { name: String, args: Vec<Expr> },

    // Control flow
    If { cond: Box<Expr>, then: Box<Expr>, else_: Option<Box<Expr>> },
    Match { subject: Box<Expr>, arms: Vec<MatchArm> },
    Block { stmts: Vec<Stmt>, tail: Option<Box<Expr>> },
    For { var: String, iter: Box<Expr>, body: Vec<Stmt> },
    While { cond: Box<Expr>, body: Vec<Stmt> },
    Loop { label: Option<String>, body: Vec<Stmt> },
    Break,
    Continue { label: Option<String> },
    Return(Option<Box<Expr>>),

    // Ownership
    Clone(Box<Expr>),
    Borrow(Box<Expr>),
    Try(Box<Expr>),           // expr?
    Ok(Box<Expr>),            // Ok(expr)
    Err(Box<Expr>),           // Err(expr)
    Some(Box<Expr>),          // Some(expr)
    None,

    // Collections
    Vec(Vec<Expr>),
    HashMap(Vec<(Expr, Expr)>),
    Tuple(Vec<Expr>),
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool, elem_ty: Type },

    // Access
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    TupleIdx(Box<Expr>, usize),

    // Structs
    Struct { name: String, fields: Vec<(String, Expr)> },
    StructUpdate { base: Box<Expr>, fields: Vec<(String, Expr)> },

    // Lambda
    Closure { params: Vec<String>, body: Box<Expr> },

    // Strings
    Format { template: String, args: Vec<Expr> },

    // Raw (escape hatch for generated runtime calls)
    Raw(String),
}

// ── Statements ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    Let { name: String, ty: Option<Type>, mutable: bool, value: Expr },
    LetPattern { pattern: Pattern, value: Expr },
    Assign { target: String, value: Expr },
    FieldAssign { target: String, field: String, value: Expr },
    IndexAssign { target: String, index: Expr, value: Expr },
    Expr(Expr),
}

// ── Patterns ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Pattern {
    Wild,
    Var(String),
    Lit(Expr),
    Ctor { name: String, args: Vec<Pattern> },
    Struct { name: String, fields: Vec<(String, Option<Pattern>)>, rest: bool },
    Tuple(Vec<Pattern>),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pat: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Type {
    I64, F64, Bool, Str, Unit,
    Vec(Box<Type>),
    HashMap(Box<Type>, Box<Type>),
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Named(String),
    Generic(String, Vec<Type>),
    Ref(Box<Type>),
    RefStr,
    Slice(Box<Type>),
    Fn(Vec<Type>, Box<Type>),
    Infer,
}

// ── Top-level ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Vec<Stmt>,
    pub tail: Option<Expr>,
    pub attrs: Vec<String>,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<(String, Type)>,
    pub generics: Vec<String>,
    pub derives: Vec<String>,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<Variant>,
    pub generics: Vec<String>,
    pub derives: Vec<String>,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub name: String,
    pub kind: VariantKind,
}

#[derive(Debug, Clone)]
pub enum VariantKind {
    Unit,
    Tuple(Vec<Type>),
    Struct(Vec<(String, Type)>),
}

#[derive(Debug, Clone)]
pub struct Program {
    pub prelude: Vec<String>,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
    pub functions: Vec<Function>,
    pub tests: Vec<Function>,
    pub main: Option<Function>,
    pub runtime: String,
}
