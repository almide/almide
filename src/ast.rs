use serde::{Deserialize, Serialize};

// Almide AST types — mirrors src/ast.ts

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Span {
    pub line: usize,
    pub col: usize,
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
    List,
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
    Simple { name: String },
    Generic { name: String, args: Vec<TypeExpr> },
    Record { fields: Vec<FieldType> },
    Fn { params: Vec<TypeExpr>, ret: Box<TypeExpr> },
    Tuple { elements: Vec<TypeExpr> },
    Newtype { inner: Box<TypeExpr> },
    Variant { cases: Vec<VariantCase> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VariantCase {
    Unit { name: String },
    Tuple { name: String, fields: Vec<TypeExpr> },
    Record { name: String, fields: Vec<FieldType> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldType {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: String,
    pub bounds: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pattern {
    Wildcard,
    Ident { name: String },
    Literal { value: Box<Expr> },
    Constructor { name: String, args: Vec<Pattern> },
    RecordPattern { name: String, fields: Vec<FieldPattern> },
    Tuple { elements: Vec<Pattern> },
    Some { inner: Box<Pattern> },
    None,
    Ok { inner: Box<Pattern> },
    Err { inner: Box<Pattern> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldPattern {
    pub name: String,
    pub pattern: Option<Pattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    Int { value: serde_json::Value, raw: String, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Float { value: f64, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    String { value: String, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    InterpolatedString { value: String, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Bool { value: bool, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Ident { name: String, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    TypeName { name: String, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    List { elements: Vec<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Record { fields: Vec<FieldInit>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    SpreadRecord { base: Box<Expr>, fields: Vec<FieldInit>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Call { callee: Box<Expr>, args: Vec<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Member { object: Box<Expr>, field: String, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Pipe { left: Box<Expr>, right: Box<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    If { cond: Box<Expr>, then: Box<Expr>, else_: Box<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Match { subject: Box<Expr>, arms: Vec<MatchArm>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Block { stmts: Vec<Stmt>, expr: Option<Box<Expr>>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    DoBlock { stmts: Vec<Stmt>, expr: Option<Box<Expr>>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    ForIn { var: String, var_tuple: Option<Vec<String>>, iterable: Box<Expr>, body: Vec<Stmt>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Lambda { params: Vec<LambdaParam>, body: Box<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Hole { #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Todo { message: String, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Try { expr: Box<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Await { expr: Box<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Binary { op: String, left: Box<Expr>, right: Box<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Unary { op: String, operand: Box<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Paren { expr: Box<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Tuple { elements: Vec<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Placeholder { #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Unit { #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    None { #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Some { expr: Box<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Ok { expr: Box<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
    Err { expr: Box<Expr>, #[serde(skip)] span: Option<Span>, #[serde(skip)] resolved_type: Option<ResolvedType> },
}

impl Expr {
    pub fn span(&self) -> Option<Span> {
        match self {
            Expr::Int { span, .. } | Expr::Float { span, .. } | Expr::String { span, .. }
            | Expr::InterpolatedString { span, .. } | Expr::Bool { span, .. }
            | Expr::Ident { span, .. } | Expr::TypeName { span, .. }
            | Expr::List { span, .. } | Expr::Record { span, .. }
            | Expr::SpreadRecord { span, .. } | Expr::Call { span, .. }
            | Expr::Member { span, .. } | Expr::Pipe { span, .. }
            | Expr::If { span, .. } | Expr::Match { span, .. }
            | Expr::Block { span, .. } | Expr::DoBlock { span, .. }
            | Expr::ForIn { span, .. } | Expr::Lambda { span, .. }
            | Expr::Hole { span, .. } | Expr::Todo { span, .. }
            | Expr::Try { span, .. } | Expr::Await { span, .. }
            | Expr::Binary { span, .. } | Expr::Unary { span, .. }
            | Expr::Paren { span, .. } | Expr::Tuple { span, .. }
            | Expr::Range { span, .. } | Expr::Placeholder { span, .. }
            | Expr::Unit { span, .. } | Expr::None { span, .. }
            | Expr::Some { span, .. } | Expr::Ok { span, .. }
            | Expr::Err { span, .. } => *span,
        }
    }

    pub fn resolved_type(&self) -> Option<ResolvedType> {
        match self {
            Expr::Int { resolved_type, .. } | Expr::Float { resolved_type, .. } | Expr::String { resolved_type, .. }
            | Expr::InterpolatedString { resolved_type, .. } | Expr::Bool { resolved_type, .. }
            | Expr::Ident { resolved_type, .. } | Expr::TypeName { resolved_type, .. }
            | Expr::List { resolved_type, .. } | Expr::Record { resolved_type, .. }
            | Expr::SpreadRecord { resolved_type, .. } | Expr::Call { resolved_type, .. }
            | Expr::Member { resolved_type, .. } | Expr::Pipe { resolved_type, .. }
            | Expr::If { resolved_type, .. } | Expr::Match { resolved_type, .. }
            | Expr::Block { resolved_type, .. } | Expr::DoBlock { resolved_type, .. }
            | Expr::ForIn { resolved_type, .. } | Expr::Lambda { resolved_type, .. }
            | Expr::Hole { resolved_type, .. } | Expr::Todo { resolved_type, .. }
            | Expr::Try { resolved_type, .. } | Expr::Await { resolved_type, .. }
            | Expr::Binary { resolved_type, .. } | Expr::Unary { resolved_type, .. }
            | Expr::Paren { resolved_type, .. } | Expr::Tuple { resolved_type, .. }
            | Expr::Range { resolved_type, .. } | Expr::Placeholder { resolved_type, .. }
            | Expr::Unit { resolved_type, .. } | Expr::None { resolved_type, .. }
            | Expr::Some { resolved_type, .. } | Expr::Ok { resolved_type, .. }
            | Expr::Err { resolved_type, .. } => *resolved_type,
        }
    }

    pub fn set_resolved_type(&mut self, ty: ResolvedType) {
        match self {
            Expr::Int { resolved_type, .. } | Expr::Float { resolved_type, .. } | Expr::String { resolved_type, .. }
            | Expr::InterpolatedString { resolved_type, .. } | Expr::Bool { resolved_type, .. }
            | Expr::Ident { resolved_type, .. } | Expr::TypeName { resolved_type, .. }
            | Expr::List { resolved_type, .. } | Expr::Record { resolved_type, .. }
            | Expr::SpreadRecord { resolved_type, .. } | Expr::Call { resolved_type, .. }
            | Expr::Member { resolved_type, .. } | Expr::Pipe { resolved_type, .. }
            | Expr::If { resolved_type, .. } | Expr::Match { resolved_type, .. }
            | Expr::Block { resolved_type, .. } | Expr::DoBlock { resolved_type, .. }
            | Expr::ForIn { resolved_type, .. } | Expr::Lambda { resolved_type, .. }
            | Expr::Hole { resolved_type, .. } | Expr::Todo { resolved_type, .. }
            | Expr::Try { resolved_type, .. } | Expr::Await { resolved_type, .. }
            | Expr::Binary { resolved_type, .. } | Expr::Unary { resolved_type, .. }
            | Expr::Paren { resolved_type, .. } | Expr::Tuple { resolved_type, .. }
            | Expr::Range { resolved_type, .. } | Expr::Placeholder { resolved_type, .. }
            | Expr::Unit { resolved_type, .. } | Expr::None { resolved_type, .. }
            | Expr::Some { resolved_type, .. } | Expr::Ok { resolved_type, .. }
            | Expr::Err { resolved_type, .. } => *resolved_type = Some(ty),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldInit {
    pub name: String,
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
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Option<TypeExpr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stmt {
    Let { name: String, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr, #[serde(skip)] span: Option<Span> },
    LetDestructure { fields: Vec<String>, value: Expr, #[serde(skip)] span: Option<Span> },
    Var { name: String, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr, #[serde(skip)] span: Option<Span> },
    Assign { name: String, value: Expr, #[serde(skip)] span: Option<Span> },
    Guard { cond: Expr, else_: Expr, #[serde(skip)] span: Option<Span> },
    Expr { expr: Expr, #[serde(skip)] span: Option<Span> },
    Comment { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Decl {
    Module { path: Vec<String>, #[serde(skip)] span: Option<Span> },
    Import { path: Vec<String>, names: Option<Vec<String>>, #[serde(skip)] span: Option<Span> },
    Type { name: String, #[serde(rename = "type")] ty: TypeExpr, deriving: Option<Vec<String>>, #[serde(skip)] span: Option<Span> },
    Fn {
        name: String,
        #[serde(default)] effect: Option<bool>,
        #[serde(default)] r#async: Option<bool>,
        params: Vec<Param>,
        #[serde(rename = "returnType")] return_type: TypeExpr,
        body: Expr,
        #[serde(skip)] span: Option<Span>,
    },
    Trait { name: String, methods: Vec<serde_json::Value>, #[serde(skip)] span: Option<Span> },
    Impl { trait_: String, for_: String, methods: Vec<Decl>, #[serde(skip)] span: Option<Span> },
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
