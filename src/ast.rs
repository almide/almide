use serde::{Deserialize, Serialize};

// Almide AST types — mirrors src/ast.ts

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
    Int { value: serde_json::Value, raw: String },
    Float { value: f64 },
    String { value: String },
    InterpolatedString { value: String },
    Bool { value: bool },
    Ident { name: String },
    TypeName { name: String },
    List { elements: Vec<Expr> },
    Record { fields: Vec<FieldInit> },
    SpreadRecord { base: Box<Expr>, fields: Vec<FieldInit> },
    Call { callee: Box<Expr>, args: Vec<Expr> },
    Member { object: Box<Expr>, field: String },
    Pipe { left: Box<Expr>, right: Box<Expr> },
    If { cond: Box<Expr>, then: Box<Expr>, else_: Box<Expr> },
    Match { subject: Box<Expr>, arms: Vec<MatchArm> },
    Block { stmts: Vec<Stmt>, expr: Option<Box<Expr>> },
    DoBlock { stmts: Vec<Stmt>, expr: Option<Box<Expr>> },
    ForIn { var: String, var_tuple: Option<Vec<String>>, iterable: Box<Expr>, body: Vec<Stmt> },
    Lambda { params: Vec<LambdaParam>, body: Box<Expr> },
    Hole,
    Todo { message: String },
    Try { expr: Box<Expr> },
    Await { expr: Box<Expr> },
    Binary { op: String, left: Box<Expr>, right: Box<Expr> },
    Unary { op: String, operand: Box<Expr> },
    Paren { expr: Box<Expr> },
    Tuple { elements: Vec<Expr> },
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool },
    Placeholder,
    Unit,
    None,
    Some { expr: Box<Expr> },
    Ok { expr: Box<Expr> },
    Err { expr: Box<Expr> },
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
    Let { name: String, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr },
    LetDestructure { fields: Vec<String>, value: Expr },
    Var { name: String, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr },
    Assign { name: String, value: Expr },
    Guard { cond: Expr, else_: Expr },
    Expr { expr: Expr },
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
    Module { path: Vec<String> },
    Import { path: Vec<String>, names: Option<Vec<String>> },
    Type { name: String, #[serde(rename = "type")] ty: TypeExpr, deriving: Option<Vec<String>> },
    Fn {
        name: String,
        #[serde(default)] effect: Option<bool>,
        #[serde(default)] r#async: Option<bool>,
        params: Vec<Param>,
        #[serde(rename = "returnType")] return_type: TypeExpr,
        body: Expr,
    },
    Trait { name: String, methods: Vec<serde_json::Value> },
    Impl { trait_: String, for_: String, methods: Vec<Decl> },
    Strict { mode: String },
    Test { name: String, body: Expr },
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
