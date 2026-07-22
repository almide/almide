//! MatrixShapeSpecializationPass — NumPy 超え の起点。
//!
//! Detects `matrix.mul_f32 / mul_f64` calls whose inputs have a
//! compile-time known **small** shape (both dims ≤ 8) and rewrites
//! them into a fully-unrolled `InlineRust` expression. NumPy's baseline
//! for small matmul is ~1μs dispatch overhead; a fully-unrolled
//! 3×3 f32 matmul drops to <50ns under LLVM register allocation.
//!
//! ## Shape propagation
//!
//! The pass tracks `VarId → (rows, cols)` via a side-table populated
//! per block. We recognise a shape producer when:
//!
//! - `matrix.ones(r, c)` / `matrix.ones_f32(r, c)` — shape (r, c)
//! - `matrix.zeros(r, c)` / `matrix.zeros_f32(r, c)` — shape (r, c)
//!
//! Shape-preserving ops (matrix.scale, matrix.neg) propagate the
//! source shape. `matrix.transpose(m)` inverts it. Other operations
//! are treated as shape-opaque and drop the binding.
//!
//! ## Rewrite
//!
//! `matrix.mul_<dtype>(a, b)` where `a: (r, k)`, `b: (k, c)`, and
//! `max(r, k, c) ≤ 8` becomes:
//!
//! ```text
//! {
//!     let __sa = &a; let __sb = &b;
//!     let mut __sc: Vec<Vec<f32>> = Vec::with_capacity(r);
//!     __sc.push(vec![__sa[0][0] * __sb[0][0] + ..., ...]);
//!     ...
//!     __sc
//! }
//! ```
//!
//! LLVM fully unrolls, hoists loads, and emits register-level SIMD.
//! Zero function call overhead, zero dispatch.
//!
//! ## Scope
//!
//! - **Rust target only** (WASM SIMD emission follows separately)
//! - Threshold `SMALL_LIMIT = 8` picked from the `_bench_matmul_small`
//!   sweep: 8² is where the current path spends 0.39μs — mostly on
//!   heap allocation that an unrolled emit elides.
//! - f64 (`matrix.mul`) and f32 (`matrix.mul_f32`) both covered.

use almide_base::intern::sym;
use almide_ir::*;
use almide_lang::types::Ty;
use std::collections::HashMap;
use super::pass::{NanoPass, PassResult, Target};

/// Shapes up to and including this many rows/cols/inner-dim are
/// candidates for unrolled emission. `8` keeps each unrolled mul
/// under ~512 ops — well within LLVM's register-allocation budget.
const SMALL_LIMIT: i64 = 8;

#[derive(Debug)]
pub struct MatrixShapeSpecPass;

impl NanoPass for MatrixShapeSpecPass {
    fn name(&self) -> &str { "MatrixShapeSpec" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec![] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            let mut shapes = HashMap::new();
            if rewrite_expr(&mut func.body, &mut shapes) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                let mut shapes = HashMap::new();
                if rewrite_expr(&mut func.body, &mut shapes) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

/// Matrix dtype label — threads through rewrite emission so we pick
/// the right Rust float width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dtype { F32, F64 }

impl Dtype {
    fn rust_type(self) -> &'static str {
        match self { Dtype::F32 => "f32", Dtype::F64 => "f64" }
    }
}

/// Statically-known matrix shape.
#[derive(Debug, Clone, Copy)]
struct Shape { rows: i64, cols: i64, dtype: Dtype }

/// Extract `matrix.<func>(args)`-style Module calls.
fn match_module_call<'a>(expr: &'a IrExpr) -> Option<(&'a str, &'a str, &'a [IrExpr])> {
    if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &expr.kind {
        return Some((module.as_str(), func.as_str(), args.as_slice()));
    }
    None
}

/// Return Some(Shape) when `expr` is a constructor with literal
/// dimensions. Also handles a few shape-preserving / shape-inverting
/// wrappers so chained ops still land in the small-shape bucket.
fn infer_shape(expr: &IrExpr, shapes: &HashMap<VarId, Shape>) -> Option<Shape> {
    // Var → look up.
    if let IrExprKind::Var { id } = &expr.kind {
        return shapes.get(id).copied();
    }
    let (module, func, args) = match_module_call(expr)?;
    if module != "matrix" { return None; }
    match func {
        // Constructors with (rows, cols) literals.
        "ones" | "zeros" if args.len() == 2 => {
            let r = lit_int(&args[0])?;
            let c = lit_int(&args[1])?;
            Some(Shape { rows: r, cols: c, dtype: Dtype::F64 })
        }
        "ones_f32" | "zeros_f32" if args.len() == 2 => {
            let r = lit_int(&args[0])?;
            let c = lit_int(&args[1])?;
            Some(Shape { rows: r, cols: c, dtype: Dtype::F32 })
        }
        // Shape-preserving unary / binary ops.
        "scale" | "neg" | "gelu" | "softmax_rows" if !args.is_empty() => {
            infer_shape(&args[0], shapes)
        }
        "add" | "sub" | "div" | "fma" | "mul" if args.len() >= 2 => {
            // For element-wise binary, both sides must share shape. We
            // trust the checker — taking either side is sufficient for
            // propagation.
            infer_shape(&args[0], shapes)
        }
        "transpose" if !args.is_empty() => {
            let s = infer_shape(&args[0], shapes)?;
            Some(Shape { rows: s.cols, cols: s.rows, dtype: s.dtype })
        }
        _ => None,
    }
}

fn lit_int(expr: &IrExpr) -> Option<i64> {
    match &expr.kind {
        IrExprKind::LitInt { value } => Some(*value),
        _ => None,
    }
}

/// True when the shape triple `(m, k, n)` for an `m×k · k×n` matmul
/// fits under the unroll threshold.
fn shape_is_small(m: i64, k: i64, n: i64) -> bool {
    m > 0 && k > 0 && n > 0
        && m <= SMALL_LIMIT && k <= SMALL_LIMIT && n <= SMALL_LIMIT
}

/// `IrExprKind::Block` case of `rewrite_expr`, extracted verbatim (cog>30
/// decomposition, pattern 2: uniform match arms, mirrors the
/// `lower_expr`/`infer_expr_inner` extraction shape).
fn rewrite_expr_block(expr: &mut IrExpr, shapes: &mut HashMap<VarId, Shape>) -> bool {
    let IrExprKind::Block { stmts, expr: tail } = &mut expr.kind else { unreachable!() };
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= rewrite_stmt(stmt, shapes);
    }
    if let Some(e) = tail {
        changed |= rewrite_expr(e, shapes);
    }
    changed
}

/// `IrExprKind::If` case of `rewrite_expr`, extracted verbatim.
fn rewrite_expr_if(expr: &mut IrExpr, shapes: &mut HashMap<VarId, Shape>) -> bool {
    let IrExprKind::If { cond, then, else_ } = &mut expr.kind else { unreachable!() };
    let mut changed = rewrite_expr(cond, shapes);
    // Branches: shape propagation through if/else is
    // conservative — drop tracking inside arms.
    let snap = shapes.clone();
    changed |= rewrite_expr(then, shapes);
    *shapes = snap.clone();
    changed |= rewrite_expr(else_, shapes);
    *shapes = snap;
    changed
}

/// `IrExprKind::Match` case of `rewrite_expr`, extracted verbatim.
fn rewrite_expr_match(expr: &mut IrExpr, shapes: &mut HashMap<VarId, Shape>) -> bool {
    let IrExprKind::Match { subject, arms } = &mut expr.kind else { unreachable!() };
    let mut changed = rewrite_expr(subject, shapes);
    let snap = shapes.clone();
    for arm in arms {
        *shapes = snap.clone();
        if let Some(g) = &mut arm.guard {
            changed |= rewrite_expr(g, shapes);
        }
        changed |= rewrite_expr(&mut arm.body, shapes);
    }
    *shapes = snap;
    changed
}

/// `IrExprKind::Lambda` case of `rewrite_expr`, extracted verbatim.
fn rewrite_expr_lambda(expr: &mut IrExpr, shapes: &mut HashMap<VarId, Shape>) -> bool {
    let IrExprKind::Lambda { body, .. } = &mut expr.kind else { unreachable!() };
    // Lambda captures: keep a shallow copy of outer shapes so
    // inlineable constants flow in.
    let snap = shapes.clone();
    let changed = rewrite_expr(body, shapes);
    *shapes = snap;
    changed
}

/// After bottom-up rewriting, check if `expr` itself is a small matmul we
/// can unroll into scalar ops. Extracted from `rewrite_expr`'s trailing
/// self-check.
fn try_unroll_small_matmul(expr: &mut IrExpr, shapes: &HashMap<VarId, Shape>) -> bool {
    let Some((module, func, args)) = match_module_call(expr) else { return false; };
    if module != "matrix" || args.len() != 2 { return false; }
    let dtype = match func {
        "mul" => Some(Dtype::F64),
        "mul_f32" => Some(Dtype::F32),
        _ => None,
    };
    let Some(dtype) = dtype else { return false; };
    let sa = infer_shape(&args[0], shapes);
    let sb = infer_shape(&args[1], shapes);
    let (Some(sa), Some(sb)) = (sa, sb) else { return false; };
    if sa.cols != sb.rows || !shape_is_small(sa.rows, sa.cols, sb.cols) {
        return false;
    }
    // Safe to unroll.
    let new_kind = make_unrolled_mul(
        sa.rows, sa.cols, sb.cols, dtype, &args[0], &args[1], expr.ty.clone(), expr.span);
    let new_expr = IrExpr { kind: new_kind, ty: expr.ty.clone(), span: expr.span, def_id: None };
    *expr = new_expr;
    true
}

fn rewrite_expr(expr: &mut IrExpr, shapes: &mut HashMap<VarId, Shape>) -> bool {
    // Recurse first so nested matmuls are handled inner-most first.
    let mut changed = match &mut expr.kind {
        IrExprKind::Block { .. } => rewrite_expr_block(expr, shapes),
        IrExprKind::If { .. } => rewrite_expr_if(expr, shapes),
        IrExprKind::Match { .. } => rewrite_expr_match(expr, shapes),
        IrExprKind::Lambda { .. } => rewrite_expr_lambda(expr, shapes),
        IrExprKind::Call { args, .. } => {
            let mut c = false;
            for arg in args.iter_mut() { c |= rewrite_expr(arg, shapes); }
            c
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            let mut c = false;
            for e in elements { c |= rewrite_expr(e, shapes); }
            c
        }
        IrExprKind::BinOp { left, right, .. } => rewrite_expr(left, shapes) | rewrite_expr(right, shapes),
        IrExprKind::UnOp { operand, .. } => rewrite_expr(operand, shapes),
        IrExprKind::Record { fields, .. } => {
            let mut c = false;
            for (_, e) in fields { c |= rewrite_expr(e, shapes); }
            c
        }
        // Explicit-preserve: every other node kind is left untouched by this
        // shape-specialization pass. Listing them all (instead of `_ => {}`)
        // makes the descent total-by-construction — a new IrExprKind variant
        // becomes a compile error here, forcing a deliberate traversal choice
        // rather than silently dropping the subtree.
        IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::Unit
        | IrExprKind::Var { .. }
        | IrExprKind::FnRef { .. }
        | IrExprKind::Fan { .. }
        | IrExprKind::ForIn { .. }
        | IrExprKind::While { .. }
        | IrExprKind::Break
        | IrExprKind::Continue
        | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. }
        | IrExprKind::MapLiteral { .. }
        | IrExprKind::EmptyMap
        | IrExprKind::SpreadRecord { .. }
        | IrExprKind::Range { .. }
        | IrExprKind::Member { .. }
        | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. }
        | IrExprKind::StringInterp { .. }
        | IrExprKind::ResultOk { .. }
        | IrExprKind::ResultErr { .. }
        | IrExprKind::OptionSome { .. }
        | IrExprKind::OptionNone
        | IrExprKind::Try { .. }
        | IrExprKind::Unwrap { .. }
        | IrExprKind::UnwrapOr { .. }
        | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. }
        | IrExprKind::Await { .. }
        | IrExprKind::Clone { .. }
        | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. }
        | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. }
        | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. }
        | IrExprKind::Hole
        | IrExprKind::Todo { .. } => false,
    };

    // Now look at self: is it a small matmul we can unroll?
    changed |= try_unroll_small_matmul(expr, shapes);

    changed
}

fn rewrite_stmt(stmt: &mut IrStmt, shapes: &mut HashMap<VarId, Shape>) -> bool {
    let mut changed = false;
    match &mut stmt.kind {
        IrStmtKind::Bind { var, value, .. } => {
            if rewrite_expr(value, shapes) { changed = true; }
            // After rewrite, re-infer the value's shape and store.
            if let Some(s) = infer_shape(value, shapes) {
                shapes.insert(*var, s);
            } else {
                shapes.remove(var);
            }
        }
        IrStmtKind::Assign { var, value } => {
            if rewrite_expr(value, shapes) { changed = true; }
            // Assignment clobbers the binding — drop shape (the var
            // might now hold a different matrix).
            shapes.remove(var);
        }
        IrStmtKind::Expr { expr } => {
            if rewrite_expr(expr, shapes) { changed = true; }
        }
        // Explicit-preserve: the shape-specialization pass only descends into
        // bind/assign/expr statements (the only ones that introduce or carry a
        // matmul-bearing expression whose shape table we track). All other
        // statement kinds are left untouched. Listing them (instead of `_ => {}`)
        // makes the descent total-by-construction — a new IrStmtKind variant is
        // a compile error here, not a silent subtree drop.
        IrStmtKind::BindDestructure { .. }
        | IrStmtKind::IndexAssign { .. }
        | IrStmtKind::MapInsert { .. }
        | IrStmtKind::FieldAssign { .. }
        | IrStmtKind::Guard { .. }
        | IrStmtKind::Comment { .. }
        | IrStmtKind::RcInc { .. }
        | IrStmtKind::RcDec { .. }
        | IrStmtKind::ListSwap { .. }
        | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. }
        | IrStmtKind::ListCopySlice { .. } => {}
    }
    changed
}

/// Build the `InlineRust` expression that replaces a small-shape
/// `matrix.mul_<dtype>` call.
///
/// We route through `almide_rt_matrix_get` / `almide_rt_matrix_from_lists`
/// because those signatures exist in both runtime backends — the Vec-based
/// one that `almide test` links against, and the burn-backed enum
/// runtime that `almide run`/`build` uses (injected by
/// `src/cli::replace_matrix_runtime`). With `lto = true` +
/// `codegen-units = 1` in the generated Cargo.toml, the two `get` and
/// one `from_lists` calls per cell inline in release mode, so the emitted
/// code still collapses to straight-line constant-index arithmetic under
/// LLVM. Dispatch and allocation are gone before the loop body sees
/// anything.
///
/// Shape arguments `(m, k, n)` mean `a` is `m × k`, `b` is `k × n`, and
/// the result is `m × n`.
fn make_unrolled_mul(
    m: i64,
    k: i64,
    n: i64,
    dtype: Dtype,
    a: &IrExpr,
    b: &IrExpr,
    _ty: Ty,
    _span: Option<almide_base::Span>,
) -> IrExprKind {
    let ty_str = dtype.rust_type();

    let mut rows = Vec::with_capacity(m as usize);
    for i in 0..m {
        let mut cells = Vec::with_capacity(n as usize);
        for j in 0..n {
            let mut terms = Vec::with_capacity(k as usize);
            for p in 0..k {
                terms.push(format!(
                    "almide_rt_matrix_get(__sa, {i}i64, {p}i64) * almide_rt_matrix_get(__sb, {p}i64, {j}i64)",
                ));
            }
            let sum = if terms.is_empty() {
                format!("0.0{ty_str}")
            } else {
                terms.join(" + ")
            };
            cells.push(sum);
        }
        rows.push(format!("vec![{}]", cells.join(", ")));
    }

    // #617: Matrix is the RcCow value type in generated code — the raw
    // `from_lists` result wraps at this InlineRust boundary (the `&{a}`/`&{b}`
    // reads deref-coerce through RcCow unchanged).
    let template = format!(
        "{{ let __sa = &{{a}}; let __sb = &{{b}}; RcCow::from(almide_rt_matrix_from_lists(&vec![{rows}])) }}",
        rows = rows.join(", "),
    );

    IrExprKind::InlineRust {
        template: template.into(),
        args: vec![
            (sym("a"), a.clone()),
            (sym("b"), b.clone()),
        ],
    }
}
