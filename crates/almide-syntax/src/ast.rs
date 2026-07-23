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
    /// Compile-time literal value in type argument position (e.g., `Array[Float, 128]`).
    ConstLit { value: i64 },
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
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
    List { elements: Vec<Pattern> },
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
    /// `if let name = scrutinee { then } else { else_ }` — Swift-style implicit unwrap:
    /// `name` binds the value INSIDE the scrutinee's Option/Result (the frontend desugars
    /// to `match scrutinee { Some(name)|Ok(name) => then, _ => else_ }` once the scrutinee
    /// type is known). Kept as a distinct node so the formatter preserves the surface form.
    IfLet { name: Sym, scrutinee: Box<Expr>, then: Box<Expr>, else_: Box<Expr> },
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
    /// Type ascription: `expr: Type` (e.g. `[]: List[Int]` in call args).
    TypeAscription { expr: Box<Expr>, #[serde(rename = "type")] ty: TypeExpr },
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
    /// `guard let name = scrutinee else { else_ }` — Swift-style: `name` binds the value
    /// inside the scrutinee's Option/Result and stays in scope for the REST of the block;
    /// the else branch must diverge. The frontend desugars the block tail into the Some/Ok
    /// arm of a match on the scrutinee.
    GuardLet { name: Sym, scrutinee: Expr, else_: Expr, #[serde(skip)] span: Option<Span> },
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attribute>,
    /// `mut` parameter modifier — the function may mutate this argument in place.
    /// Caller must pass a `var` binding (not `let` or temporary).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_mut: bool,
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

/// Generic `@name(args)` attribute on a declaration.
///
/// Hosts the stdlib unification attributes (`@inline_rust`,
/// `@wasm_intrinsic`, `@pure`, `@schedule`, `@rewrite`) and any
/// future metadata. The more rigid `@extern` / `@export` shapes are
/// still parsed into `ExternAttr` / `ExportAttr` for backward
/// compatibility; new attributes live here.
///
/// `args` preserves the source order of positional and named
/// arguments so that formatter round-trip matches input byte-for-byte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    pub name: Sym,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<AttrArg>,
    #[serde(skip)]
    pub span: Option<Span>,
}

/// One argument inside `@name(...)`. `name` is `None` for positional
/// arguments and `Some(sym)` for `name=value` pairs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttrArg {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Sym>,
    pub value: AttrValue,
}

/// Literal kinds allowed inside attribute argument positions. The
/// enum is intentionally narrow: attributes describe compile-time
/// metadata, not arbitrary expressions, so we avoid pulling in the
/// full `Expr` grammar and its recursive dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttrValue {
    /// `"literal"` — for code templates (`@inline_rust`) and
    /// arbitrary string payloads.
    String { value: String },
    /// `42`, `-1`, `0xff` — for numeric tuning values.
    Int { value: i64 },
    /// `true` / `false` — for boolean flags.
    Bool { value: bool },
    /// Unquoted identifier, e.g. `gpu` in `@schedule(device=gpu)`.
    /// Parsers should not interpret this as a reference to a variable;
    /// it is an attribute-level enum tag.
    Ident { name: Sym },
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
        /// Generic `@name(args)` attributes that are not `@extern` or
        /// `@export`. Stdlib unification attributes live here.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attrs: Vec<Attribute>,
        #[serde(default)] generics: Option<Vec<GenericParam>>,
        params: Vec<Param>,
        #[serde(rename = "returnType")] return_type: TypeExpr,
        body: Option<Expr>,
        #[serde(skip)] span: Option<Span>,
    },
    TopLet { name: Sym, #[serde(rename = "type")] ty: Option<TypeExpr>, value: Expr, #[serde(default)] mutable: bool, #[serde(default)] visibility: Visibility, #[serde(skip)] span: Option<Span> },
    Protocol { name: Sym, #[serde(default)] generics: Option<Vec<GenericParam>>, methods: Vec<ProtocolMethod>, #[serde(skip)] span: Option<Span> },
    Strict { mode: String, #[serde(skip)] span: Option<Span> },
    Test { name: String, body: Expr, #[serde(default)] where_clauses: Vec<TestWhere>, #[serde(skip)] span: Option<Span> },
    /// `local test where { ... }` — file-scoped test environment
    TestWhereDef { scope: TestWhereScope, clauses: Vec<TestWhere>, #[serde(skip)] span: Option<Span> },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TestWhereScope { Local, Module }

/// A `where` clause in a test declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestWhere {
    /// `where name = expr` — value binding
    Bind { name: Sym, value: Expr },
    /// `where path.to.name = expr` — reference override
    Override { path: Vec<Sym>, value: Expr },
    /// `where target(args...) => expr` — call pattern response
    CallResponse { target: Vec<Sym>, params: Vec<Pattern>, response: Expr },
    /// `where "case name" { bindings... }` — table-driven test case
    Case { name: String, bindings: Vec<TestWhere> },
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
    /// Names of fn declarations whose body parse failed. Lets the checker
    /// suppress cascading "undefined function" diagnostics from call sites.
    #[serde(skip)]
    pub failed_fn_names: std::collections::HashSet<String>,
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
        Decl::Test { body, where_clauses, .. } => {
            for wc in where_clauses.iter_mut() { visit_test_where_exprs_mut(wc, f); }
            visit_expr_mut(body, f);
        }
        Decl::TestWhereDef { clauses, .. } => {
            for wc in clauses.iter_mut() { visit_test_where_exprs_mut(wc, f); }
        }
        Decl::Module { .. } | Decl::Import { .. } | Decl::Type { .. } |
        Decl::Protocol { .. } | Decl::Strict { .. } => {}
    }
}

fn visit_test_where_exprs_mut(wc: &mut TestWhere, f: &mut impl FnMut(&mut Expr)) {
    match wc {
        TestWhere::Bind { value, .. } | TestWhere::Override { value, .. } => visit_expr_mut(value, f),
        TestWhere::CallResponse { response, .. } => visit_expr_mut(response, f),
        TestWhere::Case { bindings, .. } => { for b in bindings.iter_mut() { visit_test_where_exprs_mut(b, f); } }
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
        Stmt::GuardLet { scrutinee, else_, .. } => { visit_expr_mut(scrutinee, f); visit_expr_mut(else_, f); }
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
        Pattern::Tuple { elements } | Pattern::List { elements } => { for e in elements.iter_mut() { visit_pattern_exprs_mut(e, f); } }
        Pattern::Some { inner } | Pattern::Ok { inner } | Pattern::Err { inner } => visit_pattern_exprs_mut(inner, f),
        Pattern::Wildcard | Pattern::Ident { .. } | Pattern::None => {}
    }
}

/// Visits each element of an expression slice (List/Tuple/Fan payloads).
fn visit_exprs_slice_mut(exprs: &mut [Expr], f: &mut impl FnMut(&mut Expr)) {
    for e in exprs.iter_mut() { visit_expr_mut(e, f); }
}

/// Visits each key/value pair of a map literal.
fn visit_map_entries_mut(entries: &mut [(Expr, Expr)], f: &mut impl FnMut(&mut Expr)) {
    for (k, v) in entries.iter_mut() { visit_expr_mut(k, f); visit_expr_mut(v, f); }
}

/// Visits each field value of a record literal (Record/SpreadRecord payloads).
fn visit_field_inits_mut(fields: &mut [FieldInit], f: &mut impl FnMut(&mut Expr)) {
    for fi in fields.iter_mut() { visit_expr_mut(&mut fi.value, f); }
}

/// Visits pattern/guard/body of each match arm.
fn visit_match_arms_mut(arms: &mut [MatchArm], f: &mut impl FnMut(&mut Expr)) {
    for arm in arms.iter_mut() {
        visit_pattern_exprs_mut(&mut arm.pattern, f);
        if let Some(ref mut g) = arm.guard { visit_expr_mut(g, f); }
        visit_expr_mut(&mut arm.body, f);
    }
}

/// Visits a statement list (Block/ForIn/While bodies).
fn visit_stmts_mut(stmts: &mut [Stmt], f: &mut impl FnMut(&mut Expr)) {
    for s in stmts.iter_mut() { visit_stmt_exprs_mut(s, f); }
}

/// Visits the embedded expressions of an interpolated string.
fn visit_string_parts_mut(parts: &mut [StringPart], f: &mut impl FnMut(&mut Expr)) {
    for part in parts.iter_mut() {
        if let StringPart::Expr { expr: e } = part { visit_expr_mut(e, f); }
    }
}

pub fn visit_expr_mut(expr: &mut Expr, f: &mut impl FnMut(&mut Expr)) {
    f(expr);
    match &mut expr.kind {
        ExprKind::List { elements } | ExprKind::Tuple { elements } => visit_exprs_slice_mut(elements, f),
        ExprKind::Fan { exprs } => visit_exprs_slice_mut(exprs, f),
        ExprKind::MapLiteral { entries } => visit_map_entries_mut(entries, f),
        ExprKind::Record { fields, .. } => visit_field_inits_mut(fields, f),
        ExprKind::SpreadRecord { base, fields } => {
            visit_expr_mut(base, f);
            visit_field_inits_mut(fields, f);
        }
        ExprKind::Call { callee, args, named_args, .. } => {
            visit_expr_mut(callee, f);
            visit_exprs_slice_mut(args, f);
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
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            visit_expr_mut(scrutinee, f); visit_expr_mut(then, f); visit_expr_mut(else_, f);
        }
        ExprKind::Match { subject, arms } => {
            visit_expr_mut(subject, f);
            visit_match_arms_mut(arms, f);
        }
        ExprKind::Block { stmts, expr: tail } => {
            visit_stmts_mut(stmts, f);
            if let Some(e) = tail { visit_expr_mut(e, f); }
        }
        ExprKind::ForIn { iterable, body, .. } => {
            visit_expr_mut(iterable, f);
            visit_stmts_mut(body, f);
        }
        ExprKind::While { cond, body } => {
            visit_expr_mut(cond, f);
            visit_stmts_mut(body, f);
        }
        ExprKind::Lambda { body, .. } => visit_expr_mut(body, f),
        ExprKind::Try { expr } | ExprKind::Unwrap { expr } | ExprKind::ToOption { expr } |
        ExprKind::Await { expr } | ExprKind::Paren { expr } |
        ExprKind::Some { expr } | ExprKind::Ok { expr } | ExprKind::Err { expr } |
        ExprKind::OptionalChain { expr, .. } => visit_expr_mut(expr, f),
        ExprKind::Range { start, end, .. } => { visit_expr_mut(start, f); visit_expr_mut(end, f); }
        ExprKind::InterpolatedString { parts } => visit_string_parts_mut(parts, f),
        ExprKind::TypeAscription { expr, .. } => visit_expr_mut(expr, f),
        ExprKind::Int { .. } | ExprKind::Float { .. } | ExprKind::String { .. } |
        ExprKind::Bool { .. } | ExprKind::Ident { .. } | ExprKind::TypeName { .. } |
        ExprKind::EmptyMap | ExprKind::Hole | ExprKind::Todo { .. } |
        ExprKind::Break | ExprKind::Continue | ExprKind::Placeholder |
        ExprKind::Unit | ExprKind::None | ExprKind::Error => {}
    }
}
