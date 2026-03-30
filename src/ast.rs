use serde::{Deserialize, Serialize};
use crate::intern::Sym;

// Almide AST types — mirrors src/ast.ts

/// Unique expression identifier. Eliminates span-collision bugs in type lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ExprId(pub u32);

/// Generator for fresh ExprIds. Used by parser and sub-parsers.
#[derive(Debug, Clone)]
pub struct ExprIdGen {
    next: u32,
}
impl ExprIdGen {
    pub fn new() -> Self { ExprIdGen { next: 0 } }
    pub fn from(start: u32) -> Self { ExprIdGen { next: start } }
    pub fn next(&mut self) -> ExprId { let id = ExprId(self.next); self.next += 1; id }
    pub fn current(&self) -> u32 { self.next }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    #[serde(default)]
    pub end_col: usize,
}

/// Simplified type tag resolved by the checker.
/// Emitters use this for correct codegen (e.g. Float vs Int arithmetic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedType {
    Int,
    Float,
    String,
    Bool,
    Unit,
    Bytes,
    Matrix,
    List,
    Map,
    Set,
    Option,
    Result,
    Fn,
    Record,
    Tuple,
    Variant,
    Named,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeExpr {
    Simple { name: Sym },
    Generic { name: Sym, args: Vec<TypeExpr> },
    Record { fields: Vec<FieldType> },
    OpenRecord { fields: Vec<FieldType> },
    Fn { params: Vec<TypeExpr>, ret: Box<TypeExpr> },
    Tuple { elements: Vec<TypeExpr> },
    Variant { cases: Vec<VariantCase> },
    Union { members: Vec<TypeExpr> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VariantCase {
    Unit { name: Sym },
    Tuple { name: Sym, fields: Vec<TypeExpr> },
    Record { name: Sym, fields: Vec<FieldType> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldType {
    pub name: Sym,
    #[serde(rename = "type")]
    pub ty: TypeExpr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Expr>,
    /// Serialization alias: `name as "external_key": Type`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<Sym>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMethod {
    pub name: Sym,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    #[serde(default)]
    pub effect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: Sym,
    pub bounds: Option<Vec<Sym>>,
    /// Structural type constraint (e.g., `T: { name: String, .. }`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structural_bound: Option<TypeExpr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pattern {
    Wildcard,
    Ident { name: Sym },
    Literal { value: Box<Expr> },
    Constructor { name: Sym, args: Vec<Pattern> },
    RecordPattern { name: Sym, fields: Vec<FieldPattern>, rest: bool },
    Tuple { elements: Vec<Pattern> },
    Some { inner: Box<Pattern> },
    None,
    Ok { inner: Box<Pattern> },
    Err { inner: Box<Pattern> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldPattern {
    pub name: Sym,
    pub pattern: Option<Pattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StringPart {
    Lit { value: String },
    Expr { expr: Box<Expr> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    Int { value: serde_json::Value, raw: String, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Float { value: f64, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    String { value: String, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    InterpolatedString { parts: Vec<StringPart>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Bool { value: bool, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Ident { name: Sym, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    TypeName { name: Sym, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    List { elements: Vec<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    MapLiteral { entries: Vec<(Expr, Expr)>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    EmptyMap { #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Record { name: Option<Sym>, fields: Vec<FieldInit>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    SpreadRecord { base: Box<Expr>, fields: Vec<FieldInit>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Call { callee: Box<Expr>, args: Vec<Expr>, #[serde(default, skip_serializing_if = "Vec::is_empty")] named_args: Vec<(Sym, Expr)>, #[serde(default)] type_args: Option<Vec<TypeExpr>>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Member { object: Box<Expr>, field: Sym, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    TupleIndex { object: Box<Expr>, index: usize, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    IndexAccess { object: Box<Expr>, index: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Pipe { left: Box<Expr>, right: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Compose { left: Box<Expr>, right: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    If { cond: Box<Expr>, then: Box<Expr>, else_: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Match { subject: Box<Expr>, arms: Vec<MatchArm>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Block { stmts: Vec<Stmt>, expr: Option<Box<Expr>>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Fan { exprs: Vec<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    ForIn { var: Sym, var_tuple: Option<Vec<Sym>>, iterable: Box<Expr>, body: Vec<Stmt>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    While { cond: Box<Expr>, body: Vec<Stmt>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Lambda { params: Vec<LambdaParam>, body: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Hole { #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Todo { message: String, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Try { expr: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Unwrap { expr: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    UnwrapOr { expr: Box<Expr>, fallback: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    ToOption { expr: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    OptionalChain { expr: Box<Expr>, field: Sym, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Await { expr: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Binary { op: Sym, left: Box<Expr>, right: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Unary { op: Sym, operand: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Paren { expr: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Tuple { elements: Vec<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Break { #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Continue { #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Placeholder { #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Unit { #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    None { #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Some { expr: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Ok { expr: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Err { expr: Box<Expr>, #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    /// Placeholder for a parse error — allows partial AST construction.
    Error { #[serde(skip)] id: ExprId, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
}

impl Expr {
    pub fn id(&self) -> ExprId {
        match self {
            Expr::Int { id, .. } | Expr::Float { id, .. } | Expr::String { id, .. }
            | Expr::InterpolatedString { id, .. } | Expr::Bool { id, .. }
            | Expr::Ident { id, .. } | Expr::TypeName { id, .. }
            | Expr::List { id, .. } | Expr::MapLiteral { id, .. } | Expr::EmptyMap { id, .. }
            | Expr::Record { id, .. }
            | Expr::SpreadRecord { id, .. } | Expr::Call { id, .. }
            | Expr::Member { id, .. } | Expr::TupleIndex { id, .. } | Expr::IndexAccess { id, .. } | Expr::Pipe { id, .. } | Expr::Compose { id, .. }
            | Expr::If { id, .. } | Expr::Match { id, .. }
            | Expr::Block { id, .. } | Expr::Fan { id, .. }
            | Expr::ForIn { id, .. } | Expr::While { id, .. } | Expr::Lambda { id, .. }
            | Expr::Hole { id, .. } | Expr::Todo { id, .. }
            | Expr::Try { id, .. } | Expr::Unwrap { id, .. } | Expr::UnwrapOr { id, .. } | Expr::ToOption { id, .. } | Expr::OptionalChain { id, .. } | Expr::Await { id, .. }
            | Expr::Binary { id, .. } | Expr::Unary { id, .. }
            | Expr::Paren { id, .. } | Expr::Tuple { id, .. }
            | Expr::Range { id, .. } | Expr::Placeholder { id, .. }
            | Expr::Break { id, .. } | Expr::Continue { id, .. }
            | Expr::Unit { id, .. } | Expr::None { id, .. }
            | Expr::Some { id, .. } | Expr::Ok { id, .. }
            | Expr::Err { id, .. }
            | Expr::Error { id, .. } => *id,
        }
    }

    pub fn span(&self) -> Option<Span> {
        match self {
            Expr::Int { span, .. } | Expr::Float { span, .. } | Expr::String { span, .. }
            | Expr::InterpolatedString { span, .. } | Expr::Bool { span, .. }
            | Expr::Ident { span, .. } | Expr::TypeName { span, .. }
            | Expr::List { span, .. } | Expr::MapLiteral { span, .. } | Expr::EmptyMap { span, .. }
            | Expr::Record { span, .. }
            | Expr::SpreadRecord { span, .. } | Expr::Call { span, .. }
            | Expr::Member { span, .. } | Expr::TupleIndex { span, .. } | Expr::IndexAccess { span, .. } | Expr::Pipe { span, .. } | Expr::Compose { span, .. }
            | Expr::If { span, .. } | Expr::Match { span, .. }
            | Expr::Block { span, .. } | Expr::Fan { span, .. }
            | Expr::ForIn { span, .. } | Expr::While { span, .. } | Expr::Lambda { span, .. }
            | Expr::Hole { span, .. } | Expr::Todo { span, .. }
            | Expr::Try { span, .. } | Expr::Unwrap { span, .. } | Expr::UnwrapOr { span, .. } | Expr::ToOption { span, .. } | Expr::OptionalChain { span, .. } | Expr::Await { span, .. }
            | Expr::Binary { span, .. } | Expr::Unary { span, .. }
            | Expr::Paren { span, .. } | Expr::Tuple { span, .. }
            | Expr::Range { span, .. } | Expr::Placeholder { span, .. }
            | Expr::Break { span, .. } | Expr::Continue { span, .. }
            | Expr::Unit { span, .. } | Expr::None { span, .. }
            | Expr::Some { span, .. } | Expr::Ok { span, .. }
            | Expr::Err { span, .. }
            | Expr::Error { span, .. } => *span,
        }
    }

    pub fn resolved_type(&self) -> Option<ResolvedType> {
        match self {
            Expr::Int { resolved_type, .. } | Expr::Float { resolved_type, .. } | Expr::String { resolved_type, .. }
            | Expr::InterpolatedString { resolved_type, .. } | Expr::Bool { resolved_type, .. }
            | Expr::Ident { resolved_type, .. } | Expr::TypeName { resolved_type, .. }
            | Expr::List { resolved_type, .. } | Expr::MapLiteral { resolved_type, .. } | Expr::EmptyMap { resolved_type, .. }
            | Expr::Record { resolved_type, .. }
            | Expr::SpreadRecord { resolved_type, .. } | Expr::Call { resolved_type, .. }
            | Expr::Member { resolved_type, .. } | Expr::TupleIndex { resolved_type, .. } | Expr::IndexAccess { resolved_type, .. } | Expr::Pipe { resolved_type, .. } | Expr::Compose { resolved_type, .. }
            | Expr::If { resolved_type, .. } | Expr::Match { resolved_type, .. }
            | Expr::Block { resolved_type, .. } | Expr::Fan { resolved_type, .. }
            | Expr::ForIn { resolved_type, .. } | Expr::While { resolved_type, .. } | Expr::Lambda { resolved_type, .. }
            | Expr::Hole { resolved_type, .. } | Expr::Todo { resolved_type, .. }
            | Expr::Try { resolved_type, .. } | Expr::Unwrap { resolved_type, .. } | Expr::UnwrapOr { resolved_type, .. } | Expr::ToOption { resolved_type, .. } | Expr::OptionalChain { resolved_type, .. } | Expr::Await { resolved_type, .. }
            | Expr::Binary { resolved_type, .. } | Expr::Unary { resolved_type, .. }
            | Expr::Paren { resolved_type, .. } | Expr::Tuple { resolved_type, .. }
            | Expr::Range { resolved_type, .. } | Expr::Placeholder { resolved_type, .. }
            | Expr::Unit { resolved_type, .. } | Expr::None { resolved_type, .. }
            | Expr::Some { resolved_type, .. } | Expr::Ok { resolved_type, .. }
            | Expr::Err { resolved_type, .. }
            | Expr::Break { resolved_type, .. } | Expr::Continue { resolved_type, .. }
            | Expr::Error { resolved_type, .. } => *resolved_type,
        }
    }

    pub fn set_resolved_type(&mut self, ty: ResolvedType) {
        match self {
            Expr::Int { resolved_type, .. } | Expr::Float { resolved_type, .. } | Expr::String { resolved_type, .. }
            | Expr::InterpolatedString { resolved_type, .. } | Expr::Bool { resolved_type, .. }
            | Expr::Ident { resolved_type, .. } | Expr::TypeName { resolved_type, .. }
            | Expr::List { resolved_type, .. } | Expr::MapLiteral { resolved_type, .. } | Expr::EmptyMap { resolved_type, .. }
            | Expr::Record { resolved_type, .. }
            | Expr::SpreadRecord { resolved_type, .. } | Expr::Call { resolved_type, .. }
            | Expr::Member { resolved_type, .. } | Expr::TupleIndex { resolved_type, .. } | Expr::IndexAccess { resolved_type, .. } | Expr::Pipe { resolved_type, .. } | Expr::Compose { resolved_type, .. }
            | Expr::If { resolved_type, .. } | Expr::Match { resolved_type, .. }
            | Expr::Block { resolved_type, .. } | Expr::Fan { resolved_type, .. }
            | Expr::ForIn { resolved_type, .. } | Expr::While { resolved_type, .. } | Expr::Lambda { resolved_type, .. }
            | Expr::Hole { resolved_type, .. } | Expr::Todo { resolved_type, .. }
            | Expr::Try { resolved_type, .. } | Expr::Unwrap { resolved_type, .. } | Expr::UnwrapOr { resolved_type, .. } | Expr::ToOption { resolved_type, .. } | Expr::OptionalChain { resolved_type, .. } | Expr::Await { resolved_type, .. }
            | Expr::Binary { resolved_type, .. } | Expr::Unary { resolved_type, .. }
            | Expr::Paren { resolved_type, .. } | Expr::Tuple { resolved_type, .. }
            | Expr::Range { resolved_type, .. } | Expr::Placeholder { resolved_type, .. }
            | Expr::Unit { resolved_type, .. } | Expr::None { resolved_type, .. }
            | Expr::Some { resolved_type, .. } | Expr::Ok { resolved_type, .. }
            | Expr::Err { resolved_type, .. }
            | Expr::Break { resolved_type, .. } | Expr::Continue { resolved_type, .. }
            | Expr::Error { resolved_type, .. } => *resolved_type = Some(ty),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldInit {
    pub name: Sym,
    pub value: Expr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    /// Leading comments before this arm
    #[serde(skip)]
    pub comments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaParam {
    pub name: Sym,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuple_names: Option<Vec<Sym>>,
    #[serde(rename = "type")]
    pub ty: Option<TypeExpr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stmt {
    Let { name: Sym, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr, #[serde(skip)] span: Option<Span> },
    LetDestructure { pattern: Pattern, value: Expr, #[serde(skip)] span: Option<Span> },
    Var { name: Sym, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr, #[serde(skip)] span: Option<Span> },
    Assign { name: Sym, value: Expr, #[serde(skip)] span: Option<Span> },
    IndexAssign { target: Sym, index: Box<Expr>, value: Expr, #[serde(skip)] span: Option<Span> },
    FieldAssign { target: Sym, field: Sym, value: Expr, #[serde(skip)] span: Option<Span> },
    Guard { cond: Expr, else_: Expr, #[serde(skip)] span: Option<Span> },
    Expr { expr: Expr, #[serde(skip)] span: Option<Span> },
    Comment { text: String },
    /// Placeholder for a parse error — allows partial AST construction.
    Error { #[serde(skip)] span: Option<Span> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: Sym,
    #[serde(rename = "type")]
    pub ty: TypeExpr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Box<Expr>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,   // default — anyone can access
    Mod,      // same project only, not external packages
    Local,    // this file only
}

/// @extern(target, "module", "function") annotation for FFI declarations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternAttr {
    pub target: Sym,     // "rust" or "ts"
    pub module: Sym,     // e.g., "fast_lib"
    pub function: Sym,   // e.g., "reverse"
}

impl Default for Visibility {
    fn default() -> Self { Visibility::Public }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Decl {
    Module { path: Vec<Sym>, #[serde(skip)] span: Option<Span> },
    Import { path: Vec<Sym>, names: Option<Vec<Sym>>, alias: Option<Sym>, #[serde(skip)] span: Option<Span> },
    Type { name: Sym, #[serde(rename = "type")] ty: TypeExpr, deriving: Option<Vec<Sym>>, #[serde(default)] visibility: Visibility, #[serde(default)] generics: Option<Vec<GenericParam>>, #[serde(skip)] span: Option<Span> },
    Fn {
        name: Sym,
        #[serde(default)] effect: Option<bool>,
        #[serde(default)] r#async: Option<bool>,
        #[serde(default)] visibility: Visibility,
        #[serde(default)] extern_attrs: Vec<ExternAttr>,
        #[serde(default)] generics: Option<Vec<GenericParam>>,
        params: Vec<Param>,
        #[serde(rename = "returnType")] return_type: TypeExpr,
        body: Option<Expr>,
        #[serde(skip)] span: Option<Span>,
    },
    TopLet { name: Sym, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr, #[serde(default)] visibility: Visibility, #[serde(skip)] span: Option<Span> },
    Protocol { name: Sym, #[serde(default)] generics: Option<Vec<GenericParam>>, methods: Vec<ProtocolMethod>, #[serde(skip)] span: Option<Span> },
    Impl { trait_: Sym, for_: Sym, #[serde(default)] generics: Option<Vec<GenericParam>>, methods: Vec<Decl>, #[serde(skip)] span: Option<Span> },
    Strict { mode: String, #[serde(skip)] span: Option<Span> },
    Test { name: String, body: Expr, #[serde(skip)] span: Option<Span> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub module: Option<Decl>,
    pub imports: Vec<Decl>,
    pub decls: Vec<Decl>,
    /// Leading comments for each section:
    /// - index 0: comments before module/first import
    /// - index 1..=imports.len(): comments before each import (1-indexed)
    /// - remaining: comments before each decl
    #[serde(skip)]
    pub comment_map: Vec<Vec<String>>,
}
