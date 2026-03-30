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
use crate::intern::Sym;
use crate::types::Ty;

mod unknown;
mod fold;
mod use_count;
mod verify;
pub mod visit;
pub mod result;
pub mod substitute;

pub use unknown::*;
pub use fold::*;
pub use use_count::*;
pub use result::is_ir_result_expr;
pub use verify::{verify_program, IrVerifyError};
pub use visit::{IrVisitor, walk_expr, walk_stmt, walk_pattern};
pub use substitute::{substitute_var_in_expr, substitute_var_in_stmt};

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
    PowInt, PowFloat,
    MulMatrix, AddMatrix, SubMatrix, ScaleMatrix,
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
    pub name: Sym,
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
    pub(crate) entries: Vec<VarInfo>,
}

impl VarTable {
    pub fn new() -> Self { VarTable { entries: Vec::new() } }

    pub fn alloc(&mut self, name: Sym, ty: Ty, mutability: Mutability, span: Option<Span>) -> VarId {
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
    Bind { var: VarId, ty: Ty },
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
    Named { name: Sym },
    /// Resolved module function: stdlib `string.trim(s)` or UFCS `s.trim()`
    Module { module: Sym, func: Sym },
    /// Unresolved method call: `obj.method(args)` — emitter decides UFCS vs method
    Method { object: Box<IrExpr>, method: Sym },
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

impl Default for IrExpr {
    fn default() -> Self {
        IrExpr {
            kind: IrExprKind::Unit,
            ty: Ty::Unit,
            span: None,
        }
    }
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
    /// Reference to a named function used as a value (e.g., `list.map(xs, double)`)
    FnRef { name: Sym },

    // ── Operators (type-dispatched) ──
    BinOp { op: BinOp, left: Box<IrExpr>, right: Box<IrExpr> },
    UnOp { op: UnOp, operand: Box<IrExpr> },

    // ── Control flow ──
    If { cond: Box<IrExpr>, then: Box<IrExpr>, else_: Box<IrExpr> },
    Match { subject: Box<IrExpr>, arms: Vec<IrMatchArm> },
    Block { stmts: Vec<IrStmt>, expr: Option<Box<IrExpr>> },
    Fan { exprs: Vec<IrExpr> },

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
    Record { name: Option<Sym>, fields: Vec<(Sym, IrExpr)> },
    SpreadRecord { base: Box<IrExpr>, fields: Vec<(Sym, IrExpr)> },
    Tuple { elements: Vec<IrExpr> },
    Range { start: Box<IrExpr>, end: Box<IrExpr>, inclusive: bool },

    // ── Access ──
    Member { object: Box<IrExpr>, field: Sym },
    TupleIndex { object: Box<IrExpr>, index: usize },
    IndexAccess { object: Box<IrExpr>, index: Box<IrExpr> },
    /// Map key lookup: `map[key]` → returns Option<V>. Distinct from IndexAccess (list).
    MapAccess { object: Box<IrExpr>, key: Box<IrExpr> },

    // ── Functions ──
    Lambda { params: Vec<(VarId, Ty)>, body: Box<IrExpr>, lambda_id: Option<u32> },

    // ── Strings ──
    StringInterp { parts: Vec<IrStringPart> },

    // ── Result / Option ──
    ResultOk { expr: Box<IrExpr> },
    ResultErr { expr: Box<IrExpr> },
    OptionSome { expr: Box<IrExpr> },
    OptionNone,
    Try { expr: Box<IrExpr> },
    /// expr! — unwrap with error propagation (effect fn only)
    Unwrap { expr: Box<IrExpr> },
    /// expr ?? fallback — unwrap with default value
    UnwrapOr { expr: Box<IrExpr>, fallback: Box<IrExpr> },
    /// expr? — convert Result to Option (identity for Option)
    ToOption { expr: Box<IrExpr> },
    /// expr?.field — optional chaining
    OptionalChain { expr: Box<IrExpr>, field: Sym },
    Await { expr: Box<IrExpr> },

    // ── Codegen-specific (inserted by Nanopass passes) ──
    /// Explicit clone: `expr.clone()` (Rust)
    Clone { expr: Box<IrExpr> },
    /// Explicit deref: `*expr` (Box'd pattern bindings)
    Deref { expr: Box<IrExpr> },
    /// Explicit borrow: `&expr`, `&*expr`, or `&mut expr`
    Borrow { expr: Box<IrExpr>, as_str: bool, #[serde(default)] mutable: bool },
    /// Box wrapping: `Box::new(expr)`
    BoxNew { expr: Box<IrExpr> },
    /// Macro invocation: `name!(args)` (Rust assert_eq!, println!, etc.)
    RustMacro { name: Sym, args: Vec<IrExpr> },
    /// ToVec: `(expr).to_vec()`
    ToVec { expr: Box<IrExpr> },

    /// Pre-rendered code string (produced by StdlibLoweringPass).
    /// Walker outputs this verbatim — no further processing.
    RenderedCall { code: String },

    // ── Closure Conversion (inserted by ClosureConversionPass, WASM target) ──
    /// Create a closure object: lifted function + captured environment.
    ClosureCreate {
        func_name: Sym,
        captures: Vec<(VarId, Ty)>,
    },
    /// Load a captured variable from the closure environment pointer (first param of lifted fn).
    EnvLoad {
        env_var: VarId,
        index: u32,
    },

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
    /// Map key insertion: `map[key] = value`. Distinct from IndexAssign (list).
    MapInsert { target: VarId, key: IrExpr, value: IrExpr },
    FieldAssign { target: VarId, field: Sym, value: IrExpr },
    Guard { cond: IrExpr, else_: IrExpr },
    Expr { expr: IrExpr },
    Comment { text: String },
    // ── Peephole-optimized list operations (inserted by PeepholePass) ──
    /// xs.swap(a, b)
    ListSwap { target: VarId, a: IrExpr, b: IrExpr },
    /// xs[..=end].reverse()
    ListReverse { target: VarId, end: IrExpr },
    /// xs[..=end].rotate_left(1)
    ListRotateLeft { target: VarId, end: IrExpr },
    /// dst[..n].copy_from_slice(&src[..n])
    ListCopySlice { dst: VarId, src: VarId, len: IrExpr },
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
    pub name: Sym,
    pub ty: Ty,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<IrExpr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<Sym>,
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
    pub name: Sym,
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
    pub name: Sym,
    pub kind: IrTypeDeclKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deriving: Option<Vec<Sym>>,
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
    pub name: Sym,
    pub ty: Ty,
}

/// Info about an open record parameter (destructured struct fields as params).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRecordInfo {
    pub struct_name: Sym,
    pub fields: Vec<OpenFieldInfo>,
}

/// A fully-resolved function parameter in the IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrParam {
    pub var: VarId,
    pub ty: Ty,
    pub name: Sym,
    pub borrow: ParamBorrow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_record: Option<OpenRecordInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Box<IrExpr>>,
}

// ── Top-level structures ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrFunction {
    pub name: Sym,
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
    pub name: Sym,
    /// Versioned name for diamond dependency aliases (PkgId.mod_name()), if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versioned_name: Option<Sym>,
    /// Type declarations in this module
    pub type_decls: Vec<IrTypeDecl>,
    /// Functions in this module
    pub functions: Vec<IrFunction>,
    /// Top-level let bindings in this module
    pub top_lets: Vec<IrTopLet>,
    /// Variable table for this module
    pub var_table: VarTable,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IrProgram {
    pub functions: Vec<IrFunction>,
    pub top_lets: Vec<IrTopLet>,
    pub type_decls: Vec<IrTypeDecl>,
    pub var_table: VarTable,
    /// Imported user modules, lowered to IR
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<IrModule>,
    /// Type constructor registry with kind info and algebraic laws (HKT foundation).
    /// Populated during lowering with user-defined types.
    #[serde(skip)]
    pub type_registry: crate::types::TypeConstructorRegistry,
    /// Names of all effect functions (user-defined + stdlib).
    /// Populated during lowering from TypeEnv. Used by LICM to avoid hoisting effect calls.
    #[serde(skip)]
    pub effect_fn_names: std::collections::HashSet<Sym>,
    /// Effect inference results: per-function capability analysis.
    /// Populated by EffectInferencePass during codegen pipeline.
    #[serde(skip)]
    pub effect_map: crate::codegen::pass_effect_inference::EffectMap,
    /// Codegen annotations populated by BoxDerefPass (recursive enums, boxed fields, defaults).
    /// Read by the walker during template rendering.
    #[serde(skip)]
    pub codegen_annotations: crate::codegen::annotations::CodegenAnnotations,
}
