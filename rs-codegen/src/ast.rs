use serde::Deserialize;

// Almide AST types — mirrors src/ast.ts

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeExpr {
    Simple { name: String },
    Generic { name: String, args: Vec<TypeExpr> },
    Record { fields: Vec<FieldType> },
    Fn { params: Vec<TypeExpr>, ret: Box<TypeExpr> },
    Newtype { inner: Box<TypeExpr> },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldType {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pattern {
    Wildcard,
    Ident { name: String },
    Literal { value: Box<Expr> },
    Constructor { name: String, args: Vec<Pattern> },
    RecordPattern { name: String, fields: Vec<FieldPattern> },
    Some { inner: Box<Pattern> },
    None,
    Ok { inner: Box<Pattern> },
    Err { inner: Box<Pattern> },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldPattern {
    pub name: String,
    pub pattern: Option<Pattern>,
}

#[derive(Debug, Clone, Deserialize)]
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
    Lambda { params: Vec<LambdaParam>, body: Box<Expr> },
    Hole,
    Todo { message: String },
    Try { expr: Box<Expr> },
    Await { expr: Box<Expr> },
    Binary { op: String, left: Box<Expr>, right: Box<Expr> },
    Unary { op: String, operand: Box<Expr> },
    Paren { expr: Box<Expr> },
    Placeholder,
    Unit,
    None,
    Some { expr: Box<Expr> },
    Ok { expr: Box<Expr> },
    Err { expr: Box<Expr> },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldInit {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LambdaParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Option<TypeExpr>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stmt {
    Let { name: String, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr },
    LetDestructure { fields: Vec<String>, value: Expr },
    Var { name: String, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr },
    Assign { name: String, value: Expr },
    Guard { cond: Expr, else_: Expr },
    Expr { expr: Expr },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct Program {
    pub module: Option<Decl>,
    pub imports: Vec<Decl>,
    pub decls: Vec<Decl>,
}
