/// RustIR — intermediate representation between Almide IR and Rust source code.
///
/// All codegen decisions (auto-?, clone, Ok-wrap, mut, type annotations) are made
/// during IR → RustIR lowering. The Render pass is a pure, stateless string emitter.

// ── Expressions ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RustExpr {
    // Literals
    IntLit(i64),
    FloatLit(f64),
    StringLit(String),
    BoolLit(bool),
    Unit,

    // Variables
    Var(String),

    // Operators
    BinOp { op: RustBinOp, left: Box<RustExpr>, right: Box<RustExpr> },
    UnOp { op: RustUnOp, operand: Box<RustExpr> },

    // Calls
    Call { func: String, args: Vec<RustExpr> },
    MethodCall { receiver: Box<RustExpr>, method: String, args: Vec<RustExpr> },
    MacroCall { name: String, args: Vec<RustExpr> },

    // Control flow
    If { cond: Box<RustExpr>, then: Box<RustExpr>, else_: Option<Box<RustExpr>> },
    Match { subject: Box<RustExpr>, arms: Vec<RustMatchArm> },
    Block { stmts: Vec<RustStmt>, expr: Option<Box<RustExpr>> },
    For { var: String, iter: Box<RustExpr>, body: Vec<RustStmt> },
    While { cond: Box<RustExpr>, body: Vec<RustStmt> },
    Loop { label: Option<String>, body: Vec<RustStmt> },
    Break,
    Continue { label: Option<String> },
    Return(Option<Box<RustExpr>>),

    // Ownership / error handling
    Clone(Box<RustExpr>),
    ToOwned(Box<RustExpr>),
    Borrow(Box<RustExpr>),
    Deref(Box<RustExpr>),
    TryOp(Box<RustExpr>),
    ResultOk(Box<RustExpr>),
    ResultErr(Box<RustExpr>),
    OptionSome(Box<RustExpr>),
    OptionNone,

    // Collections
    Vec(Vec<RustExpr>),
    HashMap(Vec<(RustExpr, RustExpr)>),
    Tuple(Vec<RustExpr>),
    Range { start: Box<RustExpr>, end: Box<RustExpr>, inclusive: bool, elem_ty: RustType },

    // Access
    Field(Box<RustExpr>, String),
    Index(Box<RustExpr>, Box<RustExpr>),
    TupleIndex(Box<RustExpr>, usize),

    // Structs
    StructInit { name: String, fields: Vec<(String, RustExpr)> },
    StructUpdate { base: Box<RustExpr>, fields: Vec<(String, RustExpr)> },

    // Lambda
    Closure { params: Vec<RustParam>, body: Box<RustExpr> },

    // Strings
    Format { template: String, args: Vec<RustExpr> },

    // Type cast
    Cast { expr: Box<RustExpr>, ty: RustType },

    // Unsafe
    Unsafe(Box<RustExpr>),

    // Raw Rust code (escape hatch for runtime calls, macros, etc.)
    Raw(String),
}

// ── Operators ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub enum RustBinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Neq, Lt, Gt, Lte, Gte,
    And, Or,
    BitXor,
}

#[derive(Debug, Clone, Copy)]
pub enum RustUnOp {
    Neg, Not,
}

// ── Statements ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RustStmt {
    Let { name: String, ty: Option<RustType>, mutable: bool, value: RustExpr },
    LetPattern { pattern: RustPattern, value: RustExpr },
    Assign { target: String, value: RustExpr },
    FieldAssign { target: String, field: String, value: RustExpr },
    IndexAssign { target: String, index: RustExpr, value: RustExpr },
    Expr(RustExpr),
    Comment(String),
}

// ── Patterns ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RustPattern {
    Wildcard,
    Var(String),
    Literal(RustExpr),
    Constructor { name: String, args: Vec<RustPattern> },
    Struct { name: String, fields: Vec<(String, Option<RustPattern>)>, rest: bool },
    Tuple(Vec<RustPattern>),
    Box(Box<RustPattern>),
    Ref(Box<RustPattern>),
    Or(Vec<RustPattern>),
}

#[derive(Debug, Clone)]
pub struct RustMatchArm {
    pub pattern: RustPattern,
    pub guard: Option<RustExpr>,
    pub body: RustExpr,
}

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RustType {
    I64, F64, Bool, String, Unit,
    Vec(Box<RustType>),
    HashMap(Box<RustType>, Box<RustType>),
    Option(Box<RustType>),
    Result(Box<RustType>, Box<RustType>),
    Tuple(Vec<RustType>),
    Named(std::string::String),
    Generic(std::string::String, Vec<RustType>),
    Ref(Box<RustType>),
    RefStr,
    Slice(Box<RustType>),
    Fn(Vec<RustType>, Box<RustType>),
    Infer,
}

// ── Top-level items ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RustParam {
    pub name: String,
    pub ty: RustType,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
pub struct RustFunction {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<RustParam>,
    pub ret_ty: RustType,
    pub body: Vec<RustStmt>,
    pub tail_expr: Option<RustExpr>,
    pub attrs: Vec<String>,
    pub is_pub: bool,
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub struct RustStruct {
    pub name: String,
    pub fields: Vec<(String, RustType)>,
    pub generics: Vec<String>,
    pub derives: Vec<String>,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct RustEnum {
    pub name: String,
    pub variants: Vec<RustVariant>,
    pub generics: Vec<String>,
    pub derives: Vec<String>,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct RustVariant {
    pub name: String,
    pub kind: RustVariantKind,
}

#[derive(Debug, Clone)]
pub enum RustVariantKind {
    Unit,
    Tuple(Vec<RustType>),
    Struct(Vec<(String, RustType)>),
}

#[derive(Debug, Clone)]
pub struct RustTypeAlias {
    pub name: String,
    pub ty: RustType,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct RustConst {
    pub name: String,
    pub ty: RustType,
    pub value: RustExpr,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct RustImpl {
    pub type_name: String,
    pub trait_name: Option<String>,
    pub methods: Vec<RustFunction>,
}

#[derive(Debug, Clone)]
pub struct RustProgram {
    pub prelude: Vec<String>,
    pub macros: Vec<String>,
    pub structs: Vec<RustStruct>,
    pub enums: Vec<RustEnum>,
    pub type_aliases: Vec<RustTypeAlias>,
    pub consts: Vec<RustConst>,
    pub impls: Vec<RustImpl>,
    pub functions: Vec<RustFunction>,
    pub test_functions: Vec<RustFunction>,
    pub main_wrapper: Option<RustFunction>,
    pub runtime: String,
}
