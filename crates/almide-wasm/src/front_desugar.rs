//! #2747 — two surface forms the structural leg lowers by rewriting them,
//! before lowering, into forms its arms already lower (one lowering each,
//! never a second copy of one — the #2397 lesson).
//!
//! 1. `for e in m` over a Map binds `e` to each `(key, value)` entry
//! as a tuple (native iterates the map's pairs). The structural leg's map
//! walk loads the key and the value into two locals and never builds the
//! pair, so the single-binder form is rewritten, before lowering, into the
//! destructured form it already lowers:
//!
//!   for e in m { body }   =>   for (k, v) in m { let e = (k, v); body }
//!
//! The pair is an ordinary tuple literal bound by an ordinary `let`: it
//! takes its shares of the entry's key and value, and the loop body's frame
//! releases it each iteration like any other local.
//!
//! 2. `o?.f` (optional chaining) is `option.map(o, (p) => p.f)` — the
//! option evaluates once, `none` stays `none`, `some(p)` becomes
//! `some(p.f)`, and the field read of the borrowed payload takes its share
//! in the one option.map lowering.
//!
//! 3. A Matrix operator is the `matrix` module call native renders it as:
//! `a * b` → `matrix.mul(a, b)`, `a + b` / `a - b` → `matrix.add` /
//! `matrix.sub`, and `m * k` / `k * m` → `matrix.scale(m, k)` with the
//! Matrix first (an Int `k` is converted, as native's `as f64`). The
//! incumbent makes the same rewrite (almide-mir `matrix_binop_rewrite`).
//!
//! 4. A `fan { … }` arm whose top-level node is a `!` (the writer's, or the
//! one auto-try puts on a Result call arm of a tail or `let` fan) over a
//! `Result[_, String]` is that arm's own marker, not an exit from the frame
//! (C-199 / ADR-0024 D1, #3463): the arm IS its Result, and the block joins
//! every arm, the lowest-index Err being the block's — the reading native's
//! FanLowering takes (it strips the marker and joins). Lowered as a `!`, an
//! early arm's Err left the frame (or aborted `main`) before a later arm ran.
//!
//!   fan { f(x)!, g(y) }   =>   fan { f(x), g(y) }
//!
//! A `!` over an Option has no Result arm to become and stays. In a frame
//! whose error channel is `String` (`-> Result[_, String]`, or an effect fn
//! not declared `Result`), a `!` over any other error — a typed `E` the
//! channel carries as its repr (ADR-0021 D2), a `List[String]` it joins — is
//! the marker too: the block converts the lowest-index Err on the way out,
//! as that `!` would have (fan_block.rs). A `!` over
//! the frame's own typed error `E` (`-> Result[_, E]`) is the marker too
//! (#3467): the block's Err is returned whole, in that `E`. In `main`'s own
//! frame every Result arm's `!` is the marker (#3470): the block aborts with
//! the lowest-index Err after every arm ran, whatever its error type.
//!
//! 5. A `match` that takes `string.split_once`'s pair apart in place
//! never builds the pair (split_match.rs).

#[path = "split_match.rs"]
mod split_match;
pub(crate) use split_match::{AT as SPLIT_AT, HEAD as SPLIT_HEAD, TAIL as SPLIT_TAIL};

use almide_base::intern::sym;
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrProgram, IrStmt, IrStmtKind, Mutability, VarTable};
use almide_types::types::constructor::TypeConstructorId;
use almide_types::types::Ty;

/// The rewritten program, or `None` when no such loop exists (no clone).
pub(crate) fn desugar(ir: &IrProgram) -> Option<IrProgram> {
    let mut out = ir.clone();
    let mut changed = false;
    {
        let mut v = Rewriter { vars: &mut out.var_table, changed: &mut changed, frame_err: None, in_main: false, string_channel: false };
        for f in out.functions.iter_mut() {
            v.frame_err = f.ret_ty.result_err_ty();
            v.in_main = f.name.as_str() == "main";
            v.string_channel = string_channel(f);
            v.visit_expr_mut(&mut f.body);
        }
        v.frame_err = None;
        v.in_main = false;
        v.string_channel = false;
        for tl in out.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
    for m in out.modules.iter_mut() {
        let mut v = Rewriter { vars: &mut m.var_table, changed: &mut changed, frame_err: None, in_main: false, string_channel: false };
        for f in m.functions.iter_mut() {
            v.frame_err = f.ret_ty.result_err_ty();
            v.string_channel = string_channel(f);
            v.visit_expr_mut(&mut f.body);
        }
        v.frame_err = None;
        v.string_channel = false;
        for tl in m.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
    changed.then_some(out)
}

struct Rewriter<'a> {
    vars: &'a mut VarTable,
    changed: &'a mut bool,
    /// The error type of the fn being rewritten, when it returns a Result.
    frame_err: Option<Ty>,
    /// Lowering `main`'s own frame (not a lambda inside it), whose fan
    /// block aborts with any error type.
    in_main: bool,
    /// Lowering the own frame (not a lambda inside it) of a non-main fn
    /// whose error channel is `String`, which converts any arm's error.
    string_channel: bool,
}

/// Is `f`'s error channel `String` — a `-> Result[_, String]` fn, or an
/// effect fn not declared `Result` (its channel) — outside `main` (which
/// aborts) and test blocks?
fn string_channel(f: &almide_ir::IrFunction) -> bool {
    if f.name.as_str() == "main" || f.is_test {
        return false;
    }
    match f.ret_ty.result_err_ty() {
        Some(err) => err == Ty::String,
        None => f.is_effect,
    }
}

impl IrMutVisitor for Rewriter<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        // A lambda is a frame of its own, never `main`'s.
        let (in_main, string_channel) = (self.in_main, self.string_channel);
        if matches!(e.kind, IrExprKind::Lambda { .. }) {
            self.in_main = false;
            self.string_channel = false;
        }
        walk_expr_mut(self, e);
        self.in_main = in_main;
        self.string_channel = string_channel;
        self.optional_chain(e);
        self.map_loop(e);
        self.matrix_op(e);
        self.fan_arm_markers(e);
        *self.changed |= split_match::rewrite(self.vars, e);
    }
}

fn is_matrix(t: &Ty) -> bool {
    matches!(t, Ty::Matrix) || matches!(t, Ty::Applied(TypeConstructorId::Matrix, _))
}

fn module_call(module: &str, func: &str, args: Vec<IrExpr>) -> IrExprKind {
    IrExprKind::Call {
        target: CallTarget::Module { module: sym(module), func: sym(func), def_id: None },
        args,
        type_args: vec![],
    }
}

impl Rewriter<'_> {
    fn fan_arm_markers(&mut self, e: &mut IrExpr) {
        let IrExprKind::Fan { exprs } = &mut e.kind else { return };
        for arm in exprs.iter_mut() {
            if let IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } = &mut arm.kind
                && let Some(err) = expr.ty.result_err_ty()
                && (err == Ty::String || self.in_main || self.string_channel || self.frame_err.as_ref() == Some(&err))
            {
                *arm = std::mem::take(&mut **expr);
                *self.changed = true;
            }
        }
    }

    fn matrix_op(&mut self, e: &mut IrExpr) {
        use almide_ir::BinOp as B;
        let IrExprKind::BinOp { op, left, right } = &mut e.kind else { return };
        let func = match op {
            B::ScaleMatrix => "scale",
            _ if !(is_matrix(&left.ty) && is_matrix(&right.ty)) => return,
            B::MulMatrix | B::MulInt | B::MulFloat => "mul",
            B::AddMatrix | B::AddInt | B::AddFloat => "add",
            B::SubMatrix | B::SubInt | B::SubFloat => "sub",
            B::DivInt | B::DivFloat => "div",
            _ => return,
        };
        let (mut a, mut b) = (std::mem::take(&mut **left), std::mem::take(&mut **right));
        if func == "scale" && !is_matrix(&a.ty) {
            std::mem::swap(&mut a, &mut b);
        }
        if func == "scale" && b.ty == Ty::Int {
            let span = b.span;
            b = IrExpr { kind: module_call("int", "to_float", vec![b]), ty: Ty::Float, span, def_id: None };
        }
        e.kind = module_call("matrix", func, vec![a, b]);
        *self.changed = true;
    }

    fn optional_chain(&mut self, e: &mut IrExpr) {
        let IrExprKind::OptionalChain { expr, field } = &mut e.kind else { return };
        let Ty::Applied(TypeConstructorId::Option, inner) = &expr.ty else { return };
        let [payload_ty] = inner.as_slice() else { return };
        let Ty::Applied(TypeConstructorId::Option, out) = &e.ty else { return };
        let [field_ty] = out.as_slice() else { return };
        let (payload_ty, field_ty, field, span) = (payload_ty.clone(), field_ty.clone(), *field, e.span);
        let p = self.vars.alloc(sym("__chained"), payload_ty.clone(), Mutability::Let, span);
        let read = IrExpr {
            kind: IrExprKind::Member {
                object: Box::new(IrExpr { kind: IrExprKind::Var { id: p }, ty: payload_ty.clone(), span, def_id: None }),
                field,
            },
            ty: field_ty.clone(),
            span,
            def_id: None,
        };
        let lambda = IrExpr {
            kind: IrExprKind::Lambda { params: vec![(p, payload_ty.clone())], body: Box::new(read), lambda_id: None },
            ty: Ty::Fn { params: vec![payload_ty], ret: Box::new(field_ty), is_effect: false },
            span,
            def_id: None,
        };
        let subject = std::mem::take(&mut **expr);
        e.kind = module_call("option", "map", vec![subject, lambda]);
        *self.changed = true;
    }

    fn map_loop(&mut self, e: &mut IrExpr) {
        let IrExprKind::ForIn { var, var_tuple: var_tuple @ None, iterable, body } = &mut e.kind else {
            return;
        };
        let Ty::Applied(TypeConstructorId::Map, kv) = &iterable.ty else { return };
        let [kt, vt] = kv.as_slice() else { return };
        let (kt, vt) = (kt.clone(), vt.clone());
        let pair_ty = Ty::Tuple(vec![kt.clone(), vt.clone()]);
        let span = iterable.span;
        let k = self.vars.alloc(sym("__entry_key"), kt.clone(), Mutability::Let, span);
        let v = self.vars.alloc(sym("__entry_value"), vt.clone(), Mutability::Let, span);
        let cursor = self.vars.alloc(sym("__entry"), pair_ty.clone(), Mutability::Let, span);
        let read = |id, ty| IrExpr { kind: IrExprKind::Var { id }, ty, span, def_id: None };
        let pair = IrExpr {
            kind: IrExprKind::Tuple { elements: vec![read(k, kt), read(v, vt)] },
            ty: pair_ty.clone(),
            span,
            def_id: None,
        };
        let bind = IrStmt {
            kind: IrStmtKind::Bind { var: *var, mutability: Mutability::Let, ty: pair_ty, value: pair },
            span,
        };
        body.insert(0, bind);
        *var = cursor;
        *var_tuple = Some(vec![k, v]);
        *self.changed = true;
    }
}
