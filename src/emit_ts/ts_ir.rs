/// TsIR — typed intermediate representation for TypeScript/JavaScript code generation.
///
/// Input:    (constructed by lower_ts.rs)
/// Output:   (consumed by render_ts.rs)
/// Owns:     data type definitions for TS codegen IR
/// Does NOT: construction (lower_ts.rs), rendering (render_ts.rs)
///
/// Mirrors RustIR design: all codegen decisions encoded in the data structure,
/// renderer is a pure pattern match → string with zero conditionals.

// ── Expressions ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    // Literals
    Int(i64),
    BigInt(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
    Undefined,

    // Variables
    Var(String),

    // Operators
    BinOp { op: &'static str, left: Box<Expr>, right: Box<Expr> },
    UnOp { op: &'static str, operand: Box<Expr> },

    // Calls
    Call { func: Box<Expr>, args: Vec<Expr> },
    MethodCall { recv: Box<Expr>, method: String, args: Vec<Expr> },
    New { class: String, args: Vec<Expr> },

    // Control flow
    Ternary { cond: Box<Expr>, then: Box<Expr>, else_: Box<Expr> },
    Match { subject: Box<Expr>, arms: Vec<MatchArm>, has_err_arm: bool },
    Block { stmts: Vec<Stmt>, tail: Option<Box<Expr>> },
    Iife(Box<Expr>),
    For { binding: String, iter: Box<Expr>, body: Vec<Stmt> },
    ForRange { binding: String, start: Box<Expr>, end: Box<Expr>, inclusive: bool, body: Vec<Stmt> },
    While { cond: Box<Expr>, body: Vec<Stmt> },
    DoLoop { body: Vec<Stmt> },
    Break,
    Continue,
    Return(Option<Box<Expr>>),
    Throw(Box<Expr>),

    // Result erasure: err → throw
    ThrowError(Box<Expr>),
    ThrowStructuredError { msg: Box<Expr>, value: Box<Expr> },

    // Collections
    Array(Vec<Expr>),
    MapNew(Vec<(Expr, Expr)>),
    Object { fields: Vec<(String, Expr)> },
    ObjectWithTag { tag: String, fields: Vec<(String, Expr)> },
    Spread { base: Box<Expr>, fields: Vec<(String, Expr)> },
    Tuple(Vec<Expr>),
    RangeArray { start: Box<Expr>, end: Box<Expr>, inclusive: bool },

    // Access
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    TupleIdx(Box<Expr>, usize),

    // Lambda
    Arrow { params: Vec<String>, body: Box<Expr> },

    // Strings
    Template { parts: Vec<TemplatePart> },

    // Await
    Await(Box<Expr>),

    // Raw (escape hatch)
    Raw(String),
}

#[derive(Debug, Clone)]
pub enum TemplatePart {
    Lit(String),
    Expr(Expr),
}

// ── Statements ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    Var { name: String, value: Expr },
    Let { name: String, value: Expr },
    Const { name: String, value: Expr },
    VarDestructure { pattern: String, value: Expr },
    Assign { target: String, value: Expr },
    FieldAssign { target: String, field: String, value: Expr },
    IndexAssign { target: String, index: Expr, value: Expr },
    MapSet { target: String, key: Expr, value: Expr },
    If { cond: Expr, body: Vec<Stmt> },
    Expr(Expr),
    Comment(String),
    /// Test-mode try-catch for effect fn calls:
    /// `var name; try { name = value; } catch (e) { name = new __Err(...); }`
    TryCatchBind { name: String, value: Expr },
    /// Do-block err propagation:
    /// `if (name instanceof __Err) { throw ... }`
    ErrPropagate { name: String },
}

// ── Patterns (for match) ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Pattern {
    Wild,
    Bind(String),
    Literal(Expr),
    None,
    Some(Box<Pattern>),
    Ok(Box<Pattern>),
    Err(Box<Pattern>),
    Ctor { tag: String, args: Vec<(String, Pattern)> },
    RecordCtor { tag: String, fields: Vec<(String, Option<Pattern>)> },
    Tuple(Vec<Pattern>),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

// ── Types (for TS annotations) ───────────────────────────────────

#[derive(Debug, Clone)]
pub enum Type {
    Number,
    String,
    Boolean,
    Void,
    Any,
    Null,
    Array(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Object(Vec<(String, Type)>),
    Union(Vec<Type>),
    Fn { params: Vec<(String, Type)>, ret: Box<Type> },
    Named(String),
    Nullable(Box<Type>),
}

// ── Top-level ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: FnBody,
    pub is_async: bool,
    pub is_export: bool,
}

#[derive(Debug, Clone)]
pub enum FnBody {
    Block { stmts: Vec<Stmt>, tail: Option<Expr> },
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<Type>,
}

#[derive(Debug, Clone)]
pub struct Test {
    pub name: String,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum TypeDecl {
    Interface { name: String, generics: Vec<String>, fields: Vec<(String, Type)> },
    TypeAlias { name: String, generics: Vec<String>, target: Type },
    VariantCtors(Vec<VariantCtor>),
}

#[derive(Debug, Clone)]
pub struct VariantCtor {
    pub name: String,
    pub kind: VariantCtorKind,
}

#[derive(Debug, Clone)]
pub enum VariantCtorKind {
    /// `const X = { tag: "X" };`
    Const,
    /// `function X() { return { tag: "X" }; }` (generic unit)
    GenericUnit,
    /// `function X(_0, _1) { return { tag: "X", _0, _1 }; }`
    TupleCtor { arity: usize },
    /// `function X(name, age) { return { tag: "X", name, age }; }`
    RecordCtor { fields: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub type_decls: Vec<TypeDecl>,
    pub functions: Vec<Function>,
    pub exports: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub runtime: String,
    pub namespace_decls: Vec<String>,
    pub modules: Vec<Module>,
    pub type_decls: Vec<TypeDecl>,
    pub top_lets: Vec<Stmt>,
    pub functions: Vec<Function>,
    pub tests: Vec<Test>,
    pub entry_point: Option<EntryPoint>,
    pub js_mode: bool,
}

#[derive(Debug, Clone)]
pub struct EntryPoint {
    pub js_mode: bool,
}
