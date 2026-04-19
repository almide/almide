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
use almide_base::Span;
use almide_base::intern::Sym;
use almide_lang::types::Ty;

mod unknown;
mod fold;
mod use_count;
mod verify;
pub mod visit;
pub mod visit_mut;
pub mod result;
pub mod substitute;
pub mod effect;
pub mod annotations;

mod wasm_repr;

pub use unknown::*;
pub use fold::*;
pub use use_count::*;
pub use result::is_ir_result_expr;
pub use verify::{verify_program, IrVerifyError};
pub use wasm_repr::wasm_types_compatible;
pub use visit::{IrVisitor, walk_expr, walk_stmt, walk_pattern};
pub use visit_mut::{IrMutVisitor, walk_expr_mut, walk_stmt_mut, walk_pattern_mut};
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

impl BinOp {
    /// The result type of this operator, when it can be determined from the
    /// operator alone. Returns `None` for `ConcatList` (result type = operand type,
    /// which must be resolved from context).
    pub fn result_ty(&self) -> Option<Ty> {
        match self {
            BinOp::AddInt | BinOp::SubInt | BinOp::MulInt | BinOp::DivInt
            | BinOp::ModInt | BinOp::PowInt => Some(Ty::Int),
            BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat | BinOp::DivFloat
            | BinOp::ModFloat | BinOp::PowFloat => Some(Ty::Float),
            BinOp::MulMatrix | BinOp::AddMatrix | BinOp::SubMatrix | BinOp::ScaleMatrix => Some(Ty::Matrix),
            BinOp::ConcatStr => Some(Ty::String),
            BinOp::ConcatList => None,
            BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte
            | BinOp::And | BinOp::Or => Some(Ty::Bool),
        }
    }
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
    pub entries: Vec<VarInfo>,
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
    List { elements: Vec<IrPattern> },
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
    /// Tail call: same as Call but emits `return_call` in WASM.
    /// Inserted by TailCallMarkPass for calls in tail position.
    TailCall { target: CallTarget, args: Vec<IrExpr> },
    /// Fully resolved runtime-fn call. Emitted by `pass_intrinsic_lowering`
    /// from `@intrinsic(symbol)`-annotated stdlib fns. Downstream emit
    /// (Rust walker, WASM emitter) looks up `symbol` directly — borrow /
    /// clone decoration is derived from each arg's `IrExpr.ty`. See
    /// `docs/roadmap/active/dispatch-unification-plan.md` §Phase 1e-2.
    RuntimeCall { symbol: Sym, args: Vec<IrExpr> },

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
    /// Rc wrapping: `std::rc::Rc::new(expr)` — for List[Fn] elements in Rust.
    RcWrap { expr: Box<IrExpr>, cast_ty: Option<Box<almide_lang::types::Ty>> },
    /// Macro invocation: `name!(args)` (Rust assert_eq!, println!, etc.)
    RustMacro { name: Sym, args: Vec<IrExpr> },
    /// ToVec: `(expr).to_vec()`
    ToVec { expr: Box<IrExpr> },

    /// Pre-rendered code string (produced by StdlibLoweringPass).
    /// Walker outputs this verbatim — no further processing.
    RenderedCall { code: String },

    /// Rust-target inline template dispatch, produced by
    /// `StdlibLoweringPass` when a call target's IrFunction carries
    /// an `@inline_rust("...")` attribute. The walker renders each
    /// `args` element to its Rust source form, substitutes `{name}`
    /// placeholders in `template` with the rendered string, and emits
    /// the result verbatim.
    ///
    /// Unlike `RenderedCall`, the args are NOT pre-rendered at pass
    /// time: they ride the IR through later passes (clone insertion,
    /// borrow insertion, ...) and get rendered at the final walker
    /// step.
    InlineRust {
        template: String,
        /// Pairs of `(param_name, arg_expr)`. The order matches the
        /// original call's positional argument order; `param_name` is
        /// used for placeholder substitution in `template`.
        args: Vec<(Sym, IrExpr)>,
    },

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

    // ── Iterator chain (inserted by StdlibLoweringPass, Rust target) ──
    /// Replaces runtime function calls for list operations with Rust iterator chains.
    /// `source.into_iter().step1().step2()...collector()`
    IterChain {
        source: Box<IrExpr>,
        /// true = into_iter() (consumes Vec), false = iter() (borrows Vec)
        consume: bool,
        steps: Vec<IterStep>,
        collector: IterCollector,
    },

    // ── Misc ──
    Hole,
    Todo { message: String },
}

/// A single step in an iterator chain (map, filter, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IterStep {
    Map { lambda: Box<IrExpr> },
    Filter { lambda: Box<IrExpr> },
    FlatMap { lambda: Box<IrExpr> },
    FilterMap { lambda: Box<IrExpr> },
}

/// The terminal operation of an iterator chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IterCollector {
    /// `.collect::<Vec<_>>()` — materialize into Vec
    Collect,
    /// `.fold(init, |acc, x| body)`
    Fold { init: Box<IrExpr>, lambda: Box<IrExpr> },
    /// `.any(|x| body)` — returns bool
    Any { lambda: Box<IrExpr> },
    /// `.all(|x| body)` — returns bool
    All { lambda: Box<IrExpr> },
    /// `.find(|x| body)` — returns Option<T>
    Find { lambda: Box<IrExpr> },
    /// `.filter(|x| body).count() as i64`
    Count { lambda: Box<IrExpr> },
}

// ── Structural recursion helpers ────────────────────────────────
//
// `map_children` applies `f` to every direct child `IrExpr`.
// All variants are listed explicitly (no wildcard) so that adding
// a new IrExprKind variant causes a compile error here — forcing
// the author to decide how its children should be traversed.

impl IrExpr {
    /// Apply `f` to every immediate child expression, returning a new `IrExpr`.
    /// Leaf nodes (literals, Var, Unit, …) are returned unchanged.
    pub fn map_children(self, f: &mut impl FnMut(IrExpr) -> IrExpr) -> IrExpr {
        let ty = self.ty;
        let span = self.span;
        let kind = match self.kind {
            // ── Leaves (no child expressions) ──
            IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
            | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
            | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
            | IrExprKind::Break | IrExprKind::Continue
            | IrExprKind::OptionNone | IrExprKind::EmptyMap
            | IrExprKind::RenderedCall { .. }
            | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
            | IrExprKind::Hole | IrExprKind::Todo { .. } => self.kind,

            // ── Unary wrappers ──
            IrExprKind::UnOp { op, operand } => IrExprKind::UnOp { op, operand: Box::new(f(*operand)) },
            IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(f(*expr)) },
            IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(f(*expr)) },
            IrExprKind::Borrow { expr, as_str, mutable } => IrExprKind::Borrow { expr: Box::new(f(*expr)), as_str, mutable },
            IrExprKind::BoxNew { expr } => IrExprKind::BoxNew { expr: Box::new(f(*expr)) },
            IrExprKind::RcWrap { expr, cast_ty } => IrExprKind::RcWrap { expr: Box::new(f(*expr)), cast_ty },
            IrExprKind::ToVec { expr } => IrExprKind::ToVec { expr: Box::new(f(*expr)) },
            IrExprKind::Await { expr } => IrExprKind::Await { expr: Box::new(f(*expr)) },
            IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(f(*expr)) },
            IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(f(*expr)) },
            IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(f(*expr)) },
            IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(f(*expr)) },
            IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(f(*expr)) },
            IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(f(*expr)) },

            // ── Binary / access ──
            IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
                op, left: Box::new(f(*left)), right: Box::new(f(*right)),
            },
            IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
                expr: Box::new(f(*expr)), fallback: Box::new(f(*fallback)),
            },
            IrExprKind::Member { object, field } => IrExprKind::Member { object: Box::new(f(*object)), field },
            IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain { expr: Box::new(f(*expr)), field },
            IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex { object: Box::new(f(*object)), index },
            IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
                object: Box::new(f(*object)), index: Box::new(f(*index)),
            },
            IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
                object: Box::new(f(*object)), key: Box::new(f(*key)),
            },
            IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
                start: Box::new(f(*start)), end: Box::new(f(*end)), inclusive,
            },

            // ── Control flow ──
            IrExprKind::If { cond, then, else_ } => IrExprKind::If {
                cond: Box::new(f(*cond)), then: Box::new(f(*then)), else_: Box::new(f(*else_)),
            },
            IrExprKind::Match { subject, arms } => IrExprKind::Match {
                subject: Box::new(f(*subject)),
                arms: arms.into_iter().map(|arm| IrMatchArm {
                    pattern: arm.pattern,
                    guard: arm.guard.map(|g| f(g)),
                    body: f(arm.body),
                }).collect(),
            },
            IrExprKind::Block { stmts, expr } => IrExprKind::Block {
                stmts: stmts.into_iter().map(|s| s.map_exprs(f)).collect(),
                expr: expr.map(|e| Box::new(f(*e))),
            },

            // ── Loops ──
            IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
                var, var_tuple,
                iterable: Box::new(f(*iterable)),
                body: body.into_iter().map(|s| s.map_exprs(f)).collect(),
            },
            IrExprKind::While { cond, body } => IrExprKind::While {
                cond: Box::new(f(*cond)),
                body: body.into_iter().map(|s| s.map_exprs(f)).collect(),
            },

            // ── Calls ──
            IrExprKind::Call { target, args, type_args } => {
                let args = args.into_iter().map(|a| f(a)).collect();
                let target = match target {
                    CallTarget::Method { object, method } => CallTarget::Method { object: Box::new(f(*object)), method },
                    CallTarget::Computed { callee } => CallTarget::Computed { callee: Box::new(f(*callee)) },
                    other => other,
                };
                IrExprKind::Call { target, args, type_args }
            }
            IrExprKind::RuntimeCall { symbol, args } => {
                let args = args.into_iter().map(|a| f(a)).collect();
                IrExprKind::RuntimeCall { symbol, args }
            }
            IrExprKind::TailCall { target, args } => {
                let args = args.into_iter().map(|a| f(a)).collect();
                let target = match target {
                    CallTarget::Method { object, method } => CallTarget::Method { object: Box::new(f(*object)), method },
                    CallTarget::Computed { callee } => CallTarget::Computed { callee: Box::new(f(*callee)) },
                    other => other,
                };
                IrExprKind::TailCall { target, args }
            }

            // ── Collections ──
            IrExprKind::List { elements } => IrExprKind::List {
                elements: elements.into_iter().map(|e| f(e)).collect(),
            },
            IrExprKind::Tuple { elements } => IrExprKind::Tuple {
                elements: elements.into_iter().map(|e| f(e)).collect(),
            },
            IrExprKind::Fan { exprs } => IrExprKind::Fan {
                exprs: exprs.into_iter().map(|e| f(e)).collect(),
            },
            IrExprKind::Record { name, fields } => IrExprKind::Record {
                name, fields: fields.into_iter().map(|(k, v)| (k, f(v))).collect(),
            },
            IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
                base: Box::new(f(*base)),
                fields: fields.into_iter().map(|(k, v)| (k, f(v))).collect(),
            },
            IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
                entries: entries.into_iter().map(|(k, v)| (f(k), f(v))).collect(),
            },

            // ── Functions ──
            IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
                params, body: Box::new(f(*body)), lambda_id,
            },
            IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
                name, args: args.into_iter().map(|a| f(a)).collect(),
            },
            IrExprKind::InlineRust { template, args } => IrExprKind::InlineRust {
                template, args: args.into_iter().map(|(n, a)| (n, f(a))).collect(),
            },

            // ── Strings ──
            IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
                parts: parts.into_iter().map(|p| match p {
                    IrStringPart::Expr { expr } => IrStringPart::Expr { expr: f(expr) },
                    other => other,
                }).collect(),
            },

            // ── Iterator chain ──
            IrExprKind::IterChain { source, consume, steps, collector } => IrExprKind::IterChain {
                source: Box::new(f(*source)),
                consume,
                steps: steps.into_iter().map(|s| s.map_exprs(f)).collect(),
                collector: collector.map_exprs(f),
            },
        };
        IrExpr { kind, ty, span }
    }
}

impl IrStmt {
    /// Apply `f` to every expression contained in this statement.
    pub fn map_exprs(self, f: &mut impl FnMut(IrExpr) -> IrExpr) -> IrStmt {
        let kind = match self.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind { var, mutability, ty, value: f(value) },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure { pattern, value: f(value) },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: f(value) },
            IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign { target, index: f(index), value: f(value) },
            IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert { target, key: f(key), value: f(value) },
            IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign { target, field, value: f(value) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard { cond: f(cond), else_: f(else_) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: f(expr) },
            IrStmtKind::Comment { .. } => self.kind,
            IrStmtKind::ListSwap { target, a, b } => IrStmtKind::ListSwap { target, a: f(a), b: f(b) },
            IrStmtKind::ListReverse { target, end } => IrStmtKind::ListReverse { target, end: f(end) },
            IrStmtKind::ListRotateLeft { target, end } => IrStmtKind::ListRotateLeft { target, end: f(end) },
            IrStmtKind::ListCopySlice { dst, src, len } => IrStmtKind::ListCopySlice { dst, src, len: f(len) },
        };
        IrStmt { kind, span: self.span }
    }
}

impl IterStep {
    pub fn map_exprs(self, f: &mut impl FnMut(IrExpr) -> IrExpr) -> IterStep {
        match self {
            IterStep::Map { lambda } => IterStep::Map { lambda: Box::new(f(*lambda)) },
            IterStep::Filter { lambda } => IterStep::Filter { lambda: Box::new(f(*lambda)) },
            IterStep::FlatMap { lambda } => IterStep::FlatMap { lambda: Box::new(f(*lambda)) },
            IterStep::FilterMap { lambda } => IterStep::FilterMap { lambda: Box::new(f(*lambda)) },
        }
    }
}

impl IterCollector {
    pub fn map_exprs(self, f: &mut impl FnMut(IrExpr) -> IrExpr) -> IterCollector {
        match self {
            IterCollector::Collect => IterCollector::Collect,
            IterCollector::Fold { init, lambda } => IterCollector::Fold { init: Box::new(f(*init)), lambda: Box::new(f(*lambda)) },
            IterCollector::Any { lambda } => IterCollector::Any { lambda: Box::new(f(*lambda)) },
            IterCollector::All { lambda } => IterCollector::All { lambda: Box::new(f(*lambda)) },
            IterCollector::Find { lambda } => IterCollector::Find { lambda: Box::new(f(*lambda)) },
            IterCollector::Count { lambda } => IterCollector::Count { lambda: Box::new(f(*lambda)) },
        }
    }
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
    pub generics: Option<Vec<almide_lang::ast::GenericParam>>,
    pub visibility: IrVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default)]
    pub blank_lines_before: u32,
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
    /// Parameter is mutably borrowed as &mut T (for mutating intrinsics)
    RefMut,
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
    pub generics: Option<Vec<almide_lang::ast::GenericParam>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extern_attrs: Vec<almide_lang::ast::ExternAttr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub export_attrs: Vec<almide_lang::ast::ExportAttr>,
    /// Generic `@name(args)` attributes on the source fn. Preserved
    /// verbatim from AST for downstream passes (Stdlib Unification:
    /// `@inline_rust`, `@wasm_intrinsic`, `@pure`, `@schedule`,
    /// `@rewrite`). `@extern` / `@export` still live in their typed
    /// vecs above and are NOT duplicated here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<almide_lang::ast::Attribute>,
    #[serde(default = "default_ir_visibility")]
    pub visibility: IrVisibility,
    /// Doc comment from source (`///` lines).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// Number of blank lines before this declaration in source.
    #[serde(default)]
    pub blank_lines_before: u32,
}

/// Prefix applied to test function names in lowering to guarantee
/// uniqueness against same-named user fns (`fn foo` + `test "foo"`).
/// All downstream passes see a pre-normalized, unique `func.name`.
pub const TEST_NAME_PREFIX: &str = "__test_almd_";

impl IrFunction {
    /// Source-visible name. For test blocks this strips the
    /// `TEST_NAME_PREFIX` so reporters (test runner output, diagnostics)
    /// show the user's original `test "name"` string.
    pub fn display_name(&self) -> &str {
        let n = self.name.as_str();
        if self.is_test {
            n.strip_prefix(TEST_NAME_PREFIX).unwrap_or(n)
        } else {
            n
        }
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default)]
    pub blank_lines_before: u32,
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
    pub type_registry: almide_lang::types::TypeConstructorRegistry,
    /// Names of all effect functions (user-defined + stdlib).
    /// Populated during lowering from TypeEnv. Used by LICM to avoid hoisting effect calls.
    #[serde(skip)]
    pub effect_fn_names: std::collections::HashSet<Sym>,
    /// Effect inference results: per-function capability analysis.
    /// Populated by EffectInferencePass during codegen pipeline.
    #[serde(skip)]
    pub effect_map: crate::effect::EffectMap,
    /// Codegen annotations populated by BoxDerefPass (recursive enums, boxed fields, defaults).
    /// Read by the walker during template rendering.
    #[serde(skip)]
    pub codegen_annotations: crate::annotations::CodegenAnnotations,
}
