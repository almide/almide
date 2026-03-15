/// Typed IR data types.
///
/// Owns:     IR node definitions, use-count computation
/// Does NOT: construction (lower.rs), consumption (codegen)
///
/// Design goals:
/// - Every node carries full `Ty` (no runtime type queries during codegen)
/// - VarId for all variables (eliminates shadowing bugs)
/// - Type-dispatched operators (emitters never re-derive arithmetic variant)
/// - Pipes, UFCS, string interpolation desugared once
/// - Patterns compiled with VarId bindings
/// - Call targets resolved (module calls, constructors, free functions)

use std::collections::HashSet;
use serde::{Serialize, Deserialize};
use crate::ast::Span;
use crate::types::Ty;

// ── Identifiers ─────────────────────────────────────────────────

/// Unique variable identifier. Eliminates shadowing ambiguity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VarId(pub u32);

// ── Operators (type-dispatched) ─────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinOp {
    AddInt, AddFloat,
    SubInt, SubFloat,
    MulInt, MulFloat,
    DivInt, DivFloat,
    ModInt, ModFloat,
    PowFloat,
    XorInt,
    ConcatStr, ConcatList,
    Eq, Neq,
    Lt, Gt, Lte, Gte,
    And, Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnOp {
    NegInt, NegFloat, Not,
}

// ── Variable metadata ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mutability { Let, Var }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarInfo {
    pub name: String,
    pub ty: Ty,
    pub mutability: Mutability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// Number of times this variable is referenced in the IR.
    /// Computed as a post-pass after lowering.
    #[serde(default)]
    pub use_count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VarTable {
    entries: Vec<VarInfo>,
}

impl VarTable {
    pub fn new() -> Self { VarTable { entries: Vec::new() } }

    pub fn alloc(&mut self, name: String, ty: Ty, mutability: Mutability, span: Option<Span>) -> VarId {
        debug_assert!(self.entries.len() < u32::MAX as usize, "too many variables");
        let id = VarId(self.entries.len() as u32);
        self.entries.push(VarInfo { name, ty, mutability, span, use_count: 0 });
        id
    }

    pub fn get(&self, id: VarId) -> &VarInfo { &self.entries[id.0 as usize] }

    pub fn len(&self) -> usize { self.entries.len() }

    /// Increment the use count for a variable.
    pub fn increment_use(&mut self, id: VarId) {
        self.entries[id.0 as usize].use_count += 1;
    }

    /// Get the use count for a variable.
    pub fn use_count(&self, id: VarId) -> u32 {
        self.entries[id.0 as usize].use_count
    }
}

// ── String interpolation ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IrStringPart {
    Lit { value: String },
    Expr { expr: IrExpr },
}

// ── Patterns (for match arms) ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IrPattern {
    Wildcard,
    Bind { var: VarId },
    Literal { expr: IrExpr },
    Constructor { name: String, args: Vec<IrPattern> },
    RecordPattern { name: String, fields: Vec<IrFieldPattern>, rest: bool },
    Tuple { elements: Vec<IrPattern> },
    Some { inner: Box<IrPattern> },
    None,
    Ok { inner: Box<IrPattern> },
    Err { inner: Box<IrPattern> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrFieldPattern {
    pub name: String,
    pub pattern: Option<IrPattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrMatchArm {
    pub pattern: IrPattern,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guard: Option<IrExpr>,
    pub body: IrExpr,
}

// ── Call targets (resolved) ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallTarget {
    /// Free function: `foo(x)`, `println(x)`, variant constructor `Some(x)`
    Named { name: String },
    /// Resolved module function: stdlib `string.trim(s)` or UFCS `s.trim()`
    Module { module: String, func: String },
    /// Unresolved method call: `obj.method(args)` — emitter decides UFCS vs method
    Method { object: Box<IrExpr>, method: String },
    /// Computed callee: `(fn_expr)(args)`
    Computed { callee: Box<IrExpr> },
}

// ── Expressions ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrExpr {
    #[serde(flatten)]
    pub kind: IrExprKind,
    pub ty: Ty,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IrExprKind {
    // ── Literals ──
    LitInt { value: i64 },
    LitFloat { value: f64 },
    LitStr { value: String },
    LitBool { value: bool },
    Unit,

    // ── Variables ──
    Var { id: VarId },

    // ── Operators (type-dispatched) ──
    BinOp { op: BinOp, left: Box<IrExpr>, right: Box<IrExpr> },
    UnOp { op: UnOp, operand: Box<IrExpr> },

    // ── Control flow ──
    If { cond: Box<IrExpr>, then: Box<IrExpr>, else_: Box<IrExpr> },
    Match { subject: Box<IrExpr>, arms: Vec<IrMatchArm> },
    Block { stmts: Vec<IrStmt>, expr: Option<Box<IrExpr>> },
    DoBlock { stmts: Vec<IrStmt>, expr: Option<Box<IrExpr>> },

    // ── Loops ──
    ForIn {
        var: VarId,
        var_tuple: Option<Vec<VarId>>,
        iterable: Box<IrExpr>,
        body: Vec<IrStmt>,
    },
    While { cond: Box<IrExpr>, body: Vec<IrStmt> },
    Break,
    Continue,

    // ── Calls (fully resolved) ──
    Call { target: CallTarget, args: Vec<IrExpr>, #[serde(default, skip_serializing_if = "Vec::is_empty")] type_args: Vec<Ty> },

    // ── Collections ──
    List { elements: Vec<IrExpr> },
    MapLiteral { entries: Vec<(IrExpr, IrExpr)> },
    EmptyMap,
    Record { name: Option<String>, fields: Vec<(String, IrExpr)> },
    SpreadRecord { base: Box<IrExpr>, fields: Vec<(String, IrExpr)> },
    Tuple { elements: Vec<IrExpr> },
    Range { start: Box<IrExpr>, end: Box<IrExpr>, inclusive: bool },

    // ── Access ──
    Member { object: Box<IrExpr>, field: String },
    TupleIndex { object: Box<IrExpr>, index: usize },
    IndexAccess { object: Box<IrExpr>, index: Box<IrExpr> },

    // ── Functions ──
    Lambda { params: Vec<(VarId, Ty)>, body: Box<IrExpr> },

    // ── Strings ──
    StringInterp { parts: Vec<IrStringPart> },

    // ── Result / Option ──
    ResultOk { expr: Box<IrExpr> },
    ResultErr { expr: Box<IrExpr> },
    OptionSome { expr: Box<IrExpr> },
    OptionNone,
    Try { expr: Box<IrExpr> },
    Await { expr: Box<IrExpr> },

    // ── Misc ──
    Hole,
    Todo { message: String },
}

// ── Statements ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrStmt {
    #[serde(flatten)]
    pub kind: IrStmtKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IrStmtKind {
    Bind { var: VarId, mutability: Mutability, ty: Ty, value: IrExpr },
    BindDestructure { pattern: IrPattern, value: IrExpr },
    Assign { var: VarId, value: IrExpr },
    IndexAssign { target: VarId, index: IrExpr, value: IrExpr },
    FieldAssign { target: VarId, field: String, value: IrExpr },
    Guard { cond: IrExpr, else_: IrExpr },
    Expr { expr: IrExpr },
    Comment { text: String },
}

// ── Type declarations ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrVisibility {
    Public,
    /// Same project only (pub(crate) in Rust)
    Mod,
    Private,
}

fn default_ir_visibility() -> IrVisibility { IrVisibility::Public }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrFieldDecl {
    pub name: String,
    pub ty: Ty,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<IrExpr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IrVariantKind {
    Unit,
    Tuple { fields: Vec<Ty> },
    Record { fields: Vec<IrFieldDecl> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrVariantDecl {
    pub name: String,
    pub kind: IrVariantKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IrTypeDeclKind {
    Record { fields: Vec<IrFieldDecl> },
    Variant {
        cases: Vec<IrVariantDecl>,
        is_generic: bool,
        /// Constructor args that need Box wrapping (recursive variants): (ctor_name, arg_index)
        boxed_args: HashSet<(String, usize)>,
        /// Record variant fields that need Box wrapping: (ctor_name, field_name)
        boxed_record_fields: HashSet<(String, String)>,
    },
    Alias { target: Ty },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrTypeDecl {
    pub name: String,
    pub kind: IrTypeDeclKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deriving: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generics: Option<Vec<crate::ast::GenericParam>>,
    pub visibility: IrVisibility,
}

// ── Function parameter metadata ─────────────────────────────────

/// Borrow classification for a function parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamBorrow {
    /// Parameter is owned (String, Vec<T>)
    Own,
    /// Parameter can be borrowed as &T
    Ref,
    /// Parameter can be borrowed as &str (for String params)
    RefStr,
    /// Parameter can be borrowed as &[T] (for Vec<T> params)
    RefSlice,
}

/// Info about an open record field (destructured from a record param).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenFieldInfo {
    pub name: String,
    pub ty: Ty,
}

/// Info about an open record parameter (destructured struct fields as params).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRecordInfo {
    pub struct_name: String,
    pub fields: Vec<OpenFieldInfo>,
}

/// A fully-resolved function parameter in the IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrParam {
    pub var: VarId,
    pub ty: Ty,
    pub name: String,
    pub borrow: ParamBorrow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_record: Option<OpenRecordInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Box<IrExpr>>,
}

// ── Top-level structures ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<IrParam>,
    pub ret_ty: Ty,
    pub body: IrExpr,
    pub is_effect: bool,
    pub is_async: bool,
    pub is_test: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generics: Option<Vec<crate::ast::GenericParam>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extern_attrs: Vec<crate::ast::ExternAttr>,
    #[serde(default = "default_ir_visibility")]
    pub visibility: IrVisibility,
}

/// Classification of top-level let bindings for codegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopLetKind {
    /// Simple literal value (int, float, bool) — emits as `const` in Rust.
    Const,
    /// Non-literal expression — emits as `LazyLock` in Rust.
    Lazy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrTopLet {
    pub var: VarId,
    pub ty: Ty,
    pub value: IrExpr,
    #[serde(default = "default_top_let_kind")]
    pub kind: TopLetKind,
}

fn default_top_let_kind() -> TopLetKind { TopLetKind::Lazy }

/// An imported module lowered to IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrModule {
    /// Module name (e.g., "mylib" or "mylib.parser")
    pub name: String,
    /// Versioned name for diamond dependency aliases (PkgId.mod_name()), if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versioned_name: Option<String>,
    /// Type declarations in this module
    pub type_decls: Vec<IrTypeDecl>,
    /// Functions in this module
    pub functions: Vec<IrFunction>,
    /// Top-level let bindings in this module
    pub top_lets: Vec<IrTopLet>,
    /// Variable table for this module
    pub var_table: VarTable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrProgram {
    pub functions: Vec<IrFunction>,
    pub top_lets: Vec<IrTopLet>,
    pub type_decls: Vec<IrTypeDecl>,
    pub var_table: VarTable,
    /// Imported user modules, lowered to IR
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<IrModule>,
}

// ── Unknown type detection (post-pass) ──────────────────────────

/// A warning about Ty::Unknown surviving into the IR.
#[derive(Debug)]
pub struct UnknownTypeWarning {
    pub fn_name: String,
    pub span: Option<Span>,
    pub ty: Ty,
    pub context: &'static str,
}

/// Scan an IR program for any Ty::Unknown that survived lowering.
/// Returns a list of warnings (not errors) for diagnostic reporting.
pub fn collect_unknown_warnings(program: &IrProgram) -> Vec<UnknownTypeWarning> {
    let mut warnings = Vec::new();
    for f in &program.functions {
        check_expr_for_unknown(&f.body, &f.name, &mut warnings);
        for p in &f.params {
            if p.ty.contains_unknown() {
                warnings.push(UnknownTypeWarning {
                    fn_name: f.name.clone(),
                    span: None,
                    ty: p.ty.clone(),
                    context: "function parameter",
                });
            }
        }
        if f.ret_ty.contains_unknown() {
            warnings.push(UnknownTypeWarning {
                fn_name: f.name.clone(),
                span: None,
                ty: f.ret_ty.clone(),
                context: "function return type",
            });
        }
    }
    for tl in &program.top_lets {
        if tl.ty.contains_unknown() {
            warnings.push(UnknownTypeWarning {
                fn_name: "<top-level>".to_string(),
                span: None,
                ty: tl.ty.clone(),
                context: "top-level let binding",
            });
        }
    }
    warnings
}

fn check_expr_for_unknown(expr: &IrExpr, fn_name: &str, warnings: &mut Vec<UnknownTypeWarning>) {
    if expr.ty.contains_unknown() {
        warnings.push(UnknownTypeWarning {
            fn_name: fn_name.to_string(),
            span: expr.span,
            ty: expr.ty.clone(),
            context: "expression",
        });
        // Don't recurse into children — one warning per subtree is enough
        return;
    }
    // Recurse into children
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } | IrExprKind::DoBlock { stmts, expr: tail } => {
            for s in stmts { check_stmt_for_unknown(s, fn_name, warnings); }
            if let Some(t) = tail { check_expr_for_unknown(t, fn_name, warnings); }
        }
        IrExprKind::Call { target, args, .. } => {
            if let CallTarget::Computed { callee } = target {
                check_expr_for_unknown(callee, fn_name, warnings);
            }
            if let CallTarget::Method { object, .. } = target {
                check_expr_for_unknown(object, fn_name, warnings);
            }
            for a in args { check_expr_for_unknown(a, fn_name, warnings); }
        }
        IrExprKind::If { cond, then, else_ } => {
            check_expr_for_unknown(cond, fn_name, warnings);
            check_expr_for_unknown(then, fn_name, warnings);
            check_expr_for_unknown(else_, fn_name, warnings);
        }
        IrExprKind::BinOp { left, right, .. } => {
            check_expr_for_unknown(left, fn_name, warnings);
            check_expr_for_unknown(right, fn_name, warnings);
        }
        IrExprKind::UnOp { operand, .. } => {
            check_expr_for_unknown(operand, fn_name, warnings);
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { check_expr_for_unknown(e, fn_name, warnings); }
        }
        IrExprKind::Lambda { body, .. } => {
            check_expr_for_unknown(body, fn_name, warnings);
        }
        IrExprKind::Match { subject, arms } => {
            check_expr_for_unknown(subject, fn_name, warnings);
            for a in arms { check_expr_for_unknown(&a.body, fn_name, warnings); }
        }
        IrExprKind::IndexAccess { object, index } => {
            check_expr_for_unknown(object, fn_name, warnings);
            check_expr_for_unknown(index, fn_name, warnings);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            check_expr_for_unknown(object, fn_name, warnings);
        }
        IrExprKind::Try { expr } | IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Await { expr } => {
            check_expr_for_unknown(expr, fn_name, warnings);
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { check_expr_for_unknown(v, fn_name, warnings); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            check_expr_for_unknown(base, fn_name, warnings);
            for (_, v) in fields { check_expr_for_unknown(v, fn_name, warnings); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                check_expr_for_unknown(k, fn_name, warnings);
                check_expr_for_unknown(v, fn_name, warnings);
            }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                match p {
                    IrStringPart::Expr { expr } => check_expr_for_unknown(expr, fn_name, warnings),
                    IrStringPart::Lit { .. } => {}
                }
            }
        }
        IrExprKind::Range { start, end, .. } => {
            check_expr_for_unknown(start, fn_name, warnings);
            check_expr_for_unknown(end, fn_name, warnings);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            check_expr_for_unknown(iterable, fn_name, warnings);
            for s in body { check_stmt_for_unknown(s, fn_name, warnings); }
        }
        IrExprKind::While { cond, body } => {
            check_expr_for_unknown(cond, fn_name, warnings);
            for s in body { check_stmt_for_unknown(s, fn_name, warnings); }
        }
        // Leaf nodes — no children
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. }
        | IrExprKind::EmptyMap | IrExprKind::OptionNone | IrExprKind::Break
        | IrExprKind::Continue | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
}

fn check_stmt_for_unknown(stmt: &IrStmt, fn_name: &str, warnings: &mut Vec<UnknownTypeWarning>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, ty, .. } => {
            if ty.contains_unknown() {
                warnings.push(UnknownTypeWarning {
                    fn_name: fn_name.to_string(),
                    span: stmt.span,
                    ty: ty.clone(),
                    context: "let binding",
                });
            }
            check_expr_for_unknown(value, fn_name, warnings);
        }
        IrStmtKind::BindDestructure { value, .. } => {
            check_expr_for_unknown(value, fn_name, warnings);
        }
        IrStmtKind::Assign { value, .. } => {
            check_expr_for_unknown(value, fn_name, warnings);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            check_expr_for_unknown(index, fn_name, warnings);
            check_expr_for_unknown(value, fn_name, warnings);
        }
        IrStmtKind::FieldAssign { value, .. } => {
            check_expr_for_unknown(value, fn_name, warnings);
        }
        IrStmtKind::Guard { cond, else_ } => {
            check_expr_for_unknown(cond, fn_name, warnings);
            check_expr_for_unknown(else_, fn_name, warnings);
        }
        IrStmtKind::Expr { expr } => {
            check_expr_for_unknown(expr, fn_name, warnings);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

// ── Constant folding (post-pass) ─────────────────────────────────

/// Fold constant expressions in the IR program.
/// e.g. LitInt(1) + LitInt(2) → LitInt(3)
pub fn constant_fold(program: &mut IrProgram) {
    for f in &mut program.functions {
        fold_expr(&mut f.body);
    }
    for tl in &mut program.top_lets {
        fold_expr(&mut tl.value);
    }
}

fn fold_expr(expr: &mut IrExpr) {
    // Recurse first (bottom-up)
    match &mut expr.kind {
        IrExprKind::BinOp { left, right, .. } => {
            fold_expr(left);
            fold_expr(right);
        }
        IrExprKind::UnOp { operand, .. } => fold_expr(operand),
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { fold_stmt(s); }
            if let Some(t) = tail { fold_expr(t); }
        }
        IrExprKind::DoBlock { stmts, expr: tail } => {
            for s in stmts { fold_stmt(s); }
            if let Some(t) = tail { fold_expr(t); }
        }
        IrExprKind::If { cond, then, else_ } => {
            fold_expr(cond);
            fold_expr(then);
            fold_expr(else_);
        }
        IrExprKind::Call { args, .. } => {
            for a in args { fold_expr(a); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { fold_expr(e); }
        }
        IrExprKind::Lambda { body, .. } => fold_expr(body),
        IrExprKind::Match { subject, arms } => {
            fold_expr(subject);
            for a in arms { fold_expr(&mut a.body); }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } => fold_expr(expr),
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { fold_expr(v); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            fold_expr(base);
            for (_, v) in fields { fold_expr(v); }
        }
        IrExprKind::Range { start, end, .. } => {
            fold_expr(start);
            fold_expr(end);
        }
        IrExprKind::IndexAccess { object, index } => {
            fold_expr(object);
            fold_expr(index);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => fold_expr(object),
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { fold_expr(k); fold_expr(v); }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr: e } = p { fold_expr(e); }
            }
        }
        _ => {}
    }

    // Now try to fold this node
    let folded = match &expr.kind {
        IrExprKind::BinOp { op, left, right } => {
            match (&left.kind, &right.kind) {
                (IrExprKind::LitInt { value: a }, IrExprKind::LitInt { value: b }) => {
                    match op {
                        BinOp::AddInt => Some(IrExprKind::LitInt { value: a.wrapping_add(*b) }),
                        BinOp::SubInt => Some(IrExprKind::LitInt { value: a.wrapping_sub(*b) }),
                        BinOp::MulInt => Some(IrExprKind::LitInt { value: a.wrapping_mul(*b) }),
                        BinOp::DivInt if *b != 0 => Some(IrExprKind::LitInt { value: a / b }),
                        BinOp::ModInt if *b != 0 => Some(IrExprKind::LitInt { value: a % b }),
                        _ => None,
                    }
                }
                (IrExprKind::LitFloat { value: a }, IrExprKind::LitFloat { value: b }) => {
                    match op {
                        BinOp::AddFloat => Some(IrExprKind::LitFloat { value: a + b }),
                        BinOp::SubFloat => Some(IrExprKind::LitFloat { value: a - b }),
                        BinOp::MulFloat => Some(IrExprKind::LitFloat { value: a * b }),
                        BinOp::DivFloat if *b != 0.0 => Some(IrExprKind::LitFloat { value: a / b }),
                        _ => None,
                    }
                }
                (IrExprKind::LitStr { value: a }, IrExprKind::LitStr { value: b }) => {
                    match op {
                        BinOp::ConcatStr => Some(IrExprKind::LitStr { value: format!("{}{}", a, b) }),
                        _ => None,
                    }
                }
                (IrExprKind::LitBool { value: a }, IrExprKind::LitBool { value: b }) => {
                    match op {
                        BinOp::And => Some(IrExprKind::LitBool { value: *a && *b }),
                        BinOp::Or => Some(IrExprKind::LitBool { value: *a || *b }),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        IrExprKind::UnOp { op, operand } => {
            match (&op, &operand.kind) {
                (UnOp::NegInt, IrExprKind::LitInt { value }) => Some(IrExprKind::LitInt { value: -value }),
                (UnOp::NegFloat, IrExprKind::LitFloat { value }) => Some(IrExprKind::LitFloat { value: -value }),
                (UnOp::Not, IrExprKind::LitBool { value }) => Some(IrExprKind::LitBool { value: !value }),
                _ => None,
            }
        }
        _ => None,
    };

    if let Some(kind) = folded {
        expr.kind = kind;
    }
}

fn fold_stmt(stmt: &mut IrStmt) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => fold_expr(value),
        IrStmtKind::IndexAssign { index, value, .. } => {
            fold_expr(index);
            fold_expr(value);
        }
        IrStmtKind::Guard { cond, else_ } => {
            fold_expr(cond);
            fold_expr(else_);
        }
        IrStmtKind::Expr { expr } => fold_expr(expr),
        IrStmtKind::Comment { .. } => {}
    }
}

// ── Use-count computation (post-pass) ───────────────────────────

/// Walk the entire IR program and count variable uses, storing results in VarTable.
pub fn compute_use_counts(program: &mut IrProgram) {
    // Reset all counts
    for i in 0..program.var_table.len() {
        program.var_table.entries[i].use_count = 0;
    }

    // Count uses in all function bodies
    for func in &program.functions {
        count_uses_in_expr(&func.body, &mut program.var_table);
    }

    // Count uses in top-level let values
    for tl in &program.top_lets {
        count_uses_in_expr(&tl.value, &mut program.var_table);
    }
}

fn count_uses_in_expr(expr: &IrExpr, table: &mut VarTable) {
    match &expr.kind {
        IrExprKind::Var { id } => {
            table.increment_use(*id);
        }
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Hole | IrExprKind::Todo { .. }
        | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::EmptyMap => {}

        IrExprKind::BinOp { left, right, .. } => {
            count_uses_in_expr(left, table);
            count_uses_in_expr(right, table);
        }
        IrExprKind::UnOp { operand, .. } => {
            count_uses_in_expr(operand, table);
        }
        IrExprKind::If { cond, then, else_ } => {
            count_uses_in_expr(cond, table);
            count_uses_in_expr(then, table);
            count_uses_in_expr(else_, table);
        }
        IrExprKind::Match { subject, arms } => {
            count_uses_in_expr(subject, table);
            for arm in arms {
                if let Some(g) = &arm.guard { count_uses_in_expr(g, table); }
                count_uses_in_expr(&arm.body, table);
            }
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { count_uses_in_stmt(s, table); }
            if let Some(e) = expr { count_uses_in_expr(e, table); }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } => count_uses_in_expr(object, table),
                CallTarget::Computed { callee } => count_uses_in_expr(callee, table),
                _ => {}
            }
            for a in args { count_uses_in_expr(a, table); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { count_uses_in_expr(e, table); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { count_uses_in_expr(e, table); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            count_uses_in_expr(base, table);
            for (_, e) in fields { count_uses_in_expr(e, table); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                count_uses_in_expr(k, table);
                count_uses_in_expr(v, table);
            }
        }
        IrExprKind::Range { start, end, .. } => {
            count_uses_in_expr(start, table);
            count_uses_in_expr(end, table);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            count_uses_in_expr(object, table);
        }
        IrExprKind::IndexAccess { object, index } => {
            count_uses_in_expr(object, table);
            count_uses_in_expr(index, table);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            count_uses_in_expr(iterable, table);
            for s in body { count_uses_in_stmt(s, table); }
        }
        IrExprKind::While { cond, body } => {
            count_uses_in_expr(cond, table);
            for s in body { count_uses_in_stmt(s, table); }
        }
        IrExprKind::Lambda { body, .. } => {
            count_uses_in_expr(body, table);
        }
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    count_uses_in_expr(expr, table);
                }
            }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } => {
            count_uses_in_expr(expr, table);
        }
    }
}

fn count_uses_in_stmt(stmt: &IrStmt, table: &mut VarTable) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } => {
            count_uses_in_expr(value, table);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            count_uses_in_expr(index, table);
            count_uses_in_expr(value, table);
        }
        IrStmtKind::FieldAssign { value, .. } => {
            count_uses_in_expr(value, table);
        }
        IrStmtKind::Expr { expr } => {
            count_uses_in_expr(expr, table);
        }
        IrStmtKind::Guard { cond, else_ } => {
            count_uses_in_expr(cond, table);
            count_uses_in_expr(else_, table);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

/// Collect warnings for unused variables.
/// Skips: `_` prefixed names, function parameters, pattern bindings (span is None).
pub fn collect_unused_var_warnings(program: &IrProgram, file: &str) -> Vec<crate::diagnostic::Diagnostic> {
    // Collect all parameter VarIds to exclude them
    let mut param_ids: HashSet<u32> = HashSet::new();
    for func in &program.functions {
        for p in &func.params {
            param_ids.insert(p.var.0);
        }
    }

    let mut warnings = Vec::new();
    for i in 0..program.var_table.len() {
        let info = &program.var_table.entries[i];

        // Skip _ prefixed (intentionally unused)
        if info.name.starts_with('_') { continue; }

        // Skip parameters
        if param_ids.contains(&(i as u32)) { continue; }

        // Skip variables without span (pattern bindings, loop vars, etc.)
        if info.span.is_none() { continue; }

        // Skip if used
        if info.use_count > 0 { continue; }

        let span = info.span.unwrap();
        let diag = crate::diagnostic::Diagnostic::warning(
            format!("unused variable '{}'", info.name),
            format!("Prefix with '_' to suppress: _{}", info.name),
            "",
        ).at(file, span.line);
        warnings.push(diag);
    }
    warnings
}

/// Classify a top-level let value: simple literals are `Const`, everything else is `Lazy`.
pub fn classify_top_let_kind(expr: &IrExpr) -> TopLetKind {
    match &expr.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit => TopLetKind::Const,
        _ => TopLetKind::Lazy,
    }
}
