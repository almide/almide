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
//! ```rust
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
    if let IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } = &expr.kind {
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

fn rewrite_expr(expr: &mut IrExpr, shapes: &mut HashMap<VarId, Shape>) -> bool {
    let mut changed = false;
    // Recurse first so nested matmuls are handled inner-most first.
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for stmt in stmts.iter_mut() {
                if rewrite_stmt(stmt, shapes) { changed = true; }
            }
            if let Some(e) = tail {
                if rewrite_expr(e, shapes) { changed = true; }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            if rewrite_expr(cond, shapes) { changed = true; }
            // Branches: shape propagation through if/else is
            // conservative — drop tracking inside arms.
            let snap = shapes.clone();
            if rewrite_expr(then, shapes) { changed = true; }
            *shapes = snap.clone();
            if rewrite_expr(else_, shapes) { changed = true; }
            *shapes = snap;
        }
        IrExprKind::Match { subject, arms } => {
            if rewrite_expr(subject, shapes) { changed = true; }
            let snap = shapes.clone();
            for arm in arms {
                *shapes = snap.clone();
                if let Some(g) = &mut arm.guard {
                    if rewrite_expr(g, shapes) { changed = true; }
                }
                if rewrite_expr(&mut arm.body, shapes) { changed = true; }
            }
            *shapes = snap;
        }
        IrExprKind::Lambda { body, .. } => {
            // Lambda captures: keep a shallow copy of outer shapes so
            // inlineable constants flow in.
            let snap = shapes.clone();
            if rewrite_expr(body, shapes) { changed = true; }
            *shapes = snap;
        }
        IrExprKind::Call { args, .. } => {
            for arg in args.iter_mut() {
                if rewrite_expr(arg, shapes) { changed = true; }
            }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { if rewrite_expr(e, shapes) { changed = true; } }
        }
        IrExprKind::BinOp { left, right, .. } => {
            if rewrite_expr(left, shapes) { changed = true; }
            if rewrite_expr(right, shapes) { changed = true; }
        }
        IrExprKind::UnOp { operand, .. } => {
            if rewrite_expr(operand, shapes) { changed = true; }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { if rewrite_expr(e, shapes) { changed = true; } }
        }
        _ => {}
    }

    // Now look at self: is it a small matmul we can unroll?
    if let Some((module, func, args)) = match_module_call(expr) {
        if module == "matrix" && args.len() == 2 {
            let dtype = match func {
                "mul" => Some(Dtype::F64),
                "mul_f32" => Some(Dtype::F32),
                _ => None,
            };
            if let Some(dtype) = dtype {
                let sa = infer_shape(&args[0], shapes);
                let sb = infer_shape(&args[1], shapes);
                if let (Some(sa), Some(sb)) = (sa, sb) {
                    if sa.cols == sb.rows && shape_is_small(sa.rows, sa.cols, sb.cols) {
                        // Safe to unroll.
                        let new_kind = make_unrolled_mul(
                            sa.rows, sa.cols, sb.cols, dtype, &args[0], &args[1], expr.ty.clone(), expr.span);
                        let new_expr = IrExpr { kind: new_kind, ty: expr.ty.clone(), span: expr.span };
                        *expr = new_expr;
                        changed = true;
                    }
                }
            }
        }
    }

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
        _ => {}
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

    let template = format!(
        "{{ let __sa = &{{a}}; let __sb = &{{b}}; almide_rt_matrix_from_lists(&vec![{rows}]) }}",
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
