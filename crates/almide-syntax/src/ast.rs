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

pub use almide_base::span::Span;

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
pub struct Expr {
    #[serde(skip)]
    pub id: ExprId,
    #[serde(skip)]
    pub span: Option<Span>,
    #[serde(flatten)]
    pub kind: ExprKind,
}

impl Expr {
    pub fn new(id: ExprId, span: Option<Span>, kind: ExprKind) -> Self {
        Expr { id, span, kind }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExprKind {
    Int { value: serde_json::Value, raw: String },
    Float { value: f64 },
    String { value: String },
    InterpolatedString { parts: Vec<StringPart> },
    Bool { value: bool },
    Ident { name: Sym },
    TypeName { name: Sym },
    List { elements: Vec<Expr> },
    MapLiteral { entries: Vec<(Expr, Expr)> },
    EmptyMap,
    Record { name: Option<Sym>, fields: Vec<FieldInit> },
    SpreadRecord { base: Box<Expr>, fields: Vec<FieldInit> },
    Call { callee: Box<Expr>, args: Vec<Expr>, #[serde(default, skip_serializing_if = "Vec::is_empty")] named_args: Vec<(Sym, Expr)>, #[serde(default)] type_args: Option<Vec<TypeExpr>> },
    Member { object: Box<Expr>, field: Sym },
    TupleIndex { object: Box<Expr>, index: usize },
    IndexAccess { object: Box<Expr>, index: Box<Expr> },
    Pipe { left: Box<Expr>, right: Box<Expr> },
    Compose { left: Box<Expr>, right: Box<Expr> },
    If { cond: Box<Expr>, then: Box<Expr>, else_: Box<Expr> },
    Match { subject: Box<Expr>, arms: Vec<MatchArm> },
    Block { stmts: Vec<Stmt>, expr: Option<Box<Expr>> },
    Fan { exprs: Vec<Expr> },
    ForIn { var: Sym, var_tuple: Option<Vec<Sym>>, iterable: Box<Expr>, body: Vec<Stmt> },
    While { cond: Box<Expr>, body: Vec<Stmt> },
    Lambda { params: Vec<LambdaParam>, body: Box<Expr> },
    Hole,
    Todo { message: String },
    Try { expr: Box<Expr> },
    Unwrap { expr: Box<Expr> },
    UnwrapOr { expr: Box<Expr>, fallback: Box<Expr> },
    ToOption { expr: Box<Expr> },
    OptionalChain { expr: Box<Expr>, field: Sym },
    Await { expr: Box<Expr> },
    Binary { op: Sym, left: Box<Expr>, right: Box<Expr> },
    Unary { op: Sym, operand: Box<Expr> },
    Paren { expr: Box<Expr> },
    Tuple { elements: Vec<Expr> },
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool },
    Break,
    Continue,
    Placeholder,
    Unit,
    None,
    Some { expr: Box<Expr> },
    Ok { expr: Box<Expr> },
    Err { expr: Box<Expr> },
    /// Placeholder for a parse error — allows partial AST construction.
    Error,
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

/// @export(c, "symbol") annotation — export function with C ABI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportAttr {
    pub target: Sym,     // "c"
    pub symbol: Sym,     // e.g., "bridge_add"
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
        #[serde(default)] export_attrs: Vec<ExportAttr>,
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
    /// Doc comments (`///`) for each decl (parallel to `decls`).
    #[serde(skip)]
    pub doc_map: Vec<Option<String>>,
    /// Number of blank lines before each decl (parallel to `decls`).
    #[serde(skip)]
    pub blank_lines_map: Vec<u32>,
}

// ── Generic AST visitor ──

/// Apply `f` to every `Expr` node reachable from a `Program`.
pub fn visit_exprs_mut(program: &mut Program, f: &mut impl FnMut(&mut Expr)) {
    for decl in program.decls.iter_mut() { visit_decl_exprs_mut(decl, f); }
}

pub fn visit_decl_exprs_mut(decl: &mut Decl, f: &mut impl FnMut(&mut Expr)) {
    match decl {
        Decl::Fn { params, body, .. } => {
            for p in params.iter_mut() {
                if let Some(ref mut default) = p.default { visit_expr_mut(default, f); }
            }
            if let Some(b) = body { visit_expr_mut(b, f); }
        }
        Decl::TopLet { value, .. } => visit_expr_mut(value, f),
        Decl::Test { body, .. } => visit_expr_mut(body, f),
        Decl::Impl { methods, .. } => { for m in methods.iter_mut() { visit_decl_exprs_mut(m, f); } }
        Decl::Module { .. } | Decl::Import { .. } | Decl::Type { .. } |
        Decl::Protocol { .. } | Decl::Strict { .. } => {}
    }
}

fn visit_stmt_exprs_mut(stmt: &mut Stmt, f: &mut impl FnMut(&mut Expr)) {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Var { value, .. } | Stmt::Assign { value, .. } => visit_expr_mut(value, f),
        Stmt::LetDestructure { pattern, value, .. } => {
            visit_pattern_exprs_mut(pattern, f);
            visit_expr_mut(value, f);
        }
        Stmt::IndexAssign { index, value, .. } => { visit_expr_mut(index, f); visit_expr_mut(value, f); }
        Stmt::FieldAssign { value, .. } => visit_expr_mut(value, f),
        Stmt::Guard { cond, else_, .. } => { visit_expr_mut(cond, f); visit_expr_mut(else_, f); }
        Stmt::Expr { expr, .. } => visit_expr_mut(expr, f),
        Stmt::Comment { .. } | Stmt::Error { .. } => {}
    }
}

fn visit_pattern_exprs_mut(pat: &mut Pattern, f: &mut impl FnMut(&mut Expr)) {
    match pat {
        Pattern::Literal { value } => visit_expr_mut(value, f),
        Pattern::Constructor { args, .. } => { for a in args.iter_mut() { visit_pattern_exprs_mut(a, f); } }
        Pattern::RecordPattern { fields, .. } => {
            for fp in fields.iter_mut() { if let Some(ref mut p) = fp.pattern { visit_pattern_exprs_mut(p, f); } }
        }
        Pattern::Tuple { elements } => { for e in elements.iter_mut() { visit_pattern_exprs_mut(e, f); } }
        Pattern::Some { inner } | Pattern::Ok { inner } | Pattern::Err { inner } => visit_pattern_exprs_mut(inner, f),
        Pattern::Wildcard | Pattern::Ident { .. } | Pattern::None => {}
    }
}

pub fn visit_expr_mut(expr: &mut Expr, f: &mut impl FnMut(&mut Expr)) {
    f(expr);
    match &mut expr.kind {
        ExprKind::List { elements } | ExprKind::Tuple { elements } => {
            for e in elements.iter_mut() { visit_expr_mut(e, f); }
        }
        ExprKind::Fan { exprs } => { for e in exprs.iter_mut() { visit_expr_mut(e, f); } }
        ExprKind::MapLiteral { entries } => {
            for (k, v) in entries.iter_mut() { visit_expr_mut(k, f); visit_expr_mut(v, f); }
        }
        ExprKind::Record { fields, .. } => { for fi in fields.iter_mut() { visit_expr_mut(&mut fi.value, f); } }
        ExprKind::SpreadRecord { base, fields } => {
            visit_expr_mut(base, f);
            for fi in fields.iter_mut() { visit_expr_mut(&mut fi.value, f); }
        }
        ExprKind::Call { callee, args, named_args, .. } => {
            visit_expr_mut(callee, f);
            for a in args.iter_mut() { visit_expr_mut(a, f); }
            for (_, a) in named_args.iter_mut() { visit_expr_mut(a, f); }
        }
        ExprKind::Member { object, .. } | ExprKind::TupleIndex { object, .. } => visit_expr_mut(object, f),
        ExprKind::IndexAccess { object, index } => { visit_expr_mut(object, f); visit_expr_mut(index, f); }
        ExprKind::Binary { left, right, .. } | ExprKind::Pipe { left, right } |
        ExprKind::Compose { left, right } | ExprKind::UnwrapOr { expr: left, fallback: right } => {
            visit_expr_mut(left, f); visit_expr_mut(right, f);
        }
        ExprKind::Unary { operand, .. } => visit_expr_mut(operand, f),
        ExprKind::If { cond, then, else_ } => {
            visit_expr_mut(cond, f); visit_expr_mut(then, f); visit_expr_mut(else_, f);
        }
        ExprKind::Match { subject, arms } => {
            visit_expr_mut(subject, f);
            for arm in arms.iter_mut() {
                visit_pattern_exprs_mut(&mut arm.pattern, f);
                if let Some(ref mut g) = arm.guard { visit_expr_mut(g, f); }
                visit_expr_mut(&mut arm.body, f);
            }
        }
        ExprKind::Block { stmts, expr: tail } => {
            for s in stmts.iter_mut() { visit_stmt_exprs_mut(s, f); }
            if let Some(e) = tail { visit_expr_mut(e, f); }
        }
        ExprKind::ForIn { iterable, body, .. } => {
            visit_expr_mut(iterable, f);
            for s in body.iter_mut() { visit_stmt_exprs_mut(s, f); }
        }
        ExprKind::While { cond, body } => {
            visit_expr_mut(cond, f);
            for s in body.iter_mut() { visit_stmt_exprs_mut(s, f); }
        }
        ExprKind::Lambda { body, .. } => visit_expr_mut(body, f),
        ExprKind::Try { expr } | ExprKind::Unwrap { expr } | ExprKind::ToOption { expr } |
        ExprKind::Await { expr } | ExprKind::Paren { expr } |
        ExprKind::Some { expr } | ExprKind::Ok { expr } | ExprKind::Err { expr } |
        ExprKind::OptionalChain { expr, .. } => visit_expr_mut(expr, f),
        ExprKind::Range { start, end, .. } => { visit_expr_mut(start, f); visit_expr_mut(end, f); }
        ExprKind::InterpolatedString { parts } => {
            for part in parts.iter_mut() {
                if let StringPart::Expr { expr: e } = part { visit_expr_mut(e, f); }
            }
        }
        ExprKind::Int { .. } | ExprKind::Float { .. } | ExprKind::String { .. } |
        ExprKind::Bool { .. } | ExprKind::Ident { .. } | ExprKind::TypeName { .. } |
        ExprKind::EmptyMap | ExprKind::Hole | ExprKind::Todo { .. } |
        ExprKind::Break | ExprKind::Continue | ExprKind::Placeholder |
        ExprKind::Unit | ExprKind::None | ExprKind::Error => {}
    }
}
