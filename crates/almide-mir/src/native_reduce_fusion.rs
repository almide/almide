//! A `list.map` / `list.filter` chain that ends in a reduction (`list.fold`,
//! an Int `list.sum`, `list.len`) as ONE `list.fold` over the chain's source
//! on the native render: no stage builds its list. Spelled literally each
//! stage fills a list the next one walks, and the reduction walks the last —
//! `range |> map |> filter |> sum` over ten million elements wrote and read
//! back eighty megabytes to add numbers up. The standard render runs the same
//! chain as one iterator (`pass_stream_fusion.rs`).
//!
//! `fold(filter(map(xs, (p) => e), (q) => c), init, (a, x) => b)` becomes
//! `fold(xs, init, (a, p) => { let q = e; if c then { let x = q; b } else a })`:
//! each stage's parameter is bound to the element as that stage received it,
//! in stage order. `sum` folds `a + x` from 0 and `len` folds `a + 1` from 0,
//! which is what they compute over the list the chain would have built.
//!
//! Equivalence with the staged form. Staged, every element passes one stage
//! before the next stage starts; fused, one element passes every stage before
//! the next element starts. The two orders are indistinguishable exactly when
//! `almide_ir::fusion::stages_mergeable` holds over the callbacks as the
//! shared purity analysis judges them — every callback is pure and at most
//! one of them can abort — the rule the map-into-loop fusion of the lowering
//! already applies. Only scalar elements and a scalar accumulator fuse, so no
//! stage hands a heap value on; a callback that is not a lambda literal, a
//! heap element and a refused chain keep the staged form.
//!
//! The one thing the staged form does that the fold does not is build lists,
//! and a `list.range` source too long to be a list aborts there. The fused
//! form keeps that abort (`Fuse::counted_source`).
use almide_codegen::pass_stream_fusion::Purity;
use almide_ir::{fusion::{stages_mergeable, Stage}, visit_mut::{walk_expr_mut, IrMutVisitor}, *};
use almide_lang::{intern::{sym, Sym}, types::Ty};
use std::collections::HashSet;

pub(crate) fn expand(ir: &mut IrProgram) {
    // A metered region charges what its plan names, and the plan names the
    // calls of the source: such a program keeps them.
    if almide_ir::meter_plan::program_has_regions(ir) { return; }
    let facts = Purity::of(ir);
    let sums = ir.functions.iter().filter(|f| forwards_int_sum(f)).map(|f| f.name).collect();
    let mut pass = Fuse { facts: &facts, sums, vars: &mut ir.var_table };
    for f in &mut ir.functions { pass.visit_expr_mut(&mut f.body); }
}

/// `sums`: the functions that are the Int `list.sum` ([`forwards_int_sum`]).
struct Fuse<'a> { facts: &'a Purity, sums: HashSet<Sym>, vars: &'a mut VarTable }

/// `f` only hands its one `List[Int]` parameter to the Int sum intrinsic —
/// what monomorphization leaves of `list.sum` at `Int`, under a name of its
/// own: a call to `f` is that sum.
fn forwards_int_sum(f: &IrFunction) -> bool {
    let [p] = f.params.as_slice() else { return false; };
    let body = match &f.body.kind {
        IrExprKind::Block { stmts, expr: Some(e) } if stmts.is_empty() => e,
        _ => &f.body,
    };
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &body.kind else { return false; };
    module.as_str() == "int" && func.as_str() == "__sum" && f.ret_ty == Ty::Int
        && matches!(args.as_slice(), [IrExpr { kind: IrExprKind::Var { id }, .. }] if *id == p.var)
}

impl IrMutVisitor for Fuse<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        if let Some(fused) = self.rewrite(e) { *e = fused; }
    }
}

/// One `map` (`keeps` false: the element becomes `body`) or `filter`
/// (`keeps`: the element stays when `body` holds) of the chain.
struct Step { param: VarId, body: IrExpr, keeps: bool }

/// The reduction the chain ends in, as fold parts: the accumulator, the
/// parameter the element is bound to (none: `len` reads no element), the
/// start value and the step.
struct Reduce { acc: VarId, elem: Option<VarId>, init: IrExpr, step: IrExpr }

impl Fuse<'_> {
    /// The reduction `e` is, by its `list` name, and its arguments.
    fn reducer_call<'e>(&self, e: &'e IrExpr) -> Option<(&'e str, &'e [IrExpr])> {
        if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind {
            return self.sums.contains(name).then_some(("sum", args.as_slice()));
        }
        list_call(e).filter(|(func, _)| matches!(*func, "fold" | "len"))
    }

    fn rewrite(&mut self, e: &IrExpr) -> Option<IrExpr> {
        let (func, args) = self.reducer_call(e)?;
        let chain = args.first()?;
        let (source, steps) = peel(chain);
        if steps.is_empty() || !scalar(&e.ty) || !scalar(list_elem(&source.ty)?) { return None; }
        let reduce_stage = match (func, args) {
            ("fold", [_, init, f]) => {
                let IrExprKind::Lambda { body, .. } = &f.kind else { return None; };
                Stage { pure: self.facts.pure(body) && self.facts.pure(init), total: self.facts.total(body) && self.facts.total(init), short_circuits: false }
            }
            ("sum", [_]) if e.ty == Ty::Int => Stage { pure: true, total: true, short_circuits: false },
            ("len", [_]) => Stage { pure: true, total: true, short_circuits: false },
            _ => return None,
        };
        let stages: Vec<Stage> = steps.iter()
            .map(|s| Stage { pure: self.facts.pure(&s.body), total: self.facts.total(&s.body), short_circuits: false })
            .chain([reduce_stage])
            .collect();
        if !stages_mergeable(&stages) { return None; }
        let reduce = self.reduction(func, args, &e.ty)?;
        let first = steps.first()?.param;
        let elem_ty = self.vars.get(first).ty.clone();
        let acc_ty = e.ty.clone();
        let (ahead, source) = self.counted_source(source)?;
        let body = fused_body(&steps, &reduce, self.vars, var(first, elem_ty.clone()));
        let closure = Ty::Fn { params: vec![acc_ty.clone(), elem_ty.clone()], ret: Box::new(acc_ty.clone()), is_effect: false };
        let lambda = node(IrExprKind::Lambda { params: vec![(reduce.acc, acc_ty.clone()), (first, elem_ty)], body: Box::new(body), lambda_id: None }, closure);
        let target = CallTarget::Module { module: sym("list"), func: sym("fold"), def_id: None };
        let fold = IrExpr { kind: IrExprKind::Call { target, args: vec![source, reduce.init, lambda], type_args: Vec::new() }, ty: acc_ty.clone(), span: e.span, def_id: None };
        if ahead.is_empty() { return Some(fold); }
        Some(node(IrExprKind::Block { stmts: ahead, expr: Some(Box::new(fold)) }, acc_ty))
    }

    /// The source the fold walks, and the statements that go ahead of it.
    /// A list is walked as it is. A `list.range` is not built by the fold,
    /// where the first stage of the chain sized a list from it: one too long
    /// to be a list stays an out-of-memory abort (`native_range_loops`), read
    /// from bounds bound once. Any other range keeps the staged form.
    fn counted_source(&mut self, source: &IrExpr) -> Option<(Vec<IrStmt>, IrExpr)> {
        use crate::native_range_loops::{build_guard, range_call, unbuildable};
        if matches!(source.kind, IrExprKind::Range { .. }) { return None; }
        let Some((start, end)) = range_call(source) else { return Some((Vec::new(), source.clone())); };
        match unbuildable(start, end) {
            Some(false) => return Some((Vec::new(), source.clone())),
            Some(true) => return None,
            None => {}
        }
        let mut ahead = Vec::new();
        let mut once = |e: &IrExpr, vars: &mut VarTable| {
            let v = vars.alloc_fresh("range_bound", Ty::Int, Mutability::Let, None);
            ahead.push(IrStmt { kind: IrStmtKind::Bind { var: v, mutability: Mutability::Let, ty: Ty::Int, value: e.clone() }, span: None });
            var(v, Ty::Int)
        };
        let (start, end) = (once(start, self.vars), once(end, self.vars));
        ahead.push(build_guard(&start, &end, self.vars));
        let IrExprKind::Call { target, type_args, .. } = &source.kind else { return None; };
        let call = IrExprKind::Call { target: target.clone(), args: vec![start, end], type_args: type_args.clone() };
        Some((ahead, IrExpr { kind: call, ty: source.ty.clone(), span: source.span, def_id: source.def_id }))
    }

    fn reduction(&mut self, func: &str, args: &[IrExpr], ty: &Ty) -> Option<Reduce> {
        if let ("fold", [_, init, f]) = (func, args) {
            let IrExprKind::Lambda { params, body, .. } = &f.kind else { return None; };
            let [(acc, _), (elem, _)] = params.as_slice() else { return None; };
            return Some(Reduce { acc: *acc, elem: Some(*elem), init: init.clone(), step: (**body).clone() });
        }
        let acc = self.vars.alloc_fresh("fused_acc", Ty::Int, Mutability::Let, None);
        let zero = node(IrExprKind::LitInt { value: 0 }, Ty::Int);
        let add = |right: IrExpr| node(IrExprKind::BinOp { op: BinOp::AddInt, left: Box::new(var(acc, Ty::Int)), right: Box::new(right) }, Ty::Int);
        if func == "sum" {
            let elem = self.vars.alloc_fresh("fused_elem", ty.clone(), Mutability::Let, None);
            return Some(Reduce { acc, elem: Some(elem), init: zero, step: add(var(elem, Ty::Int)) });
        }
        Some(Reduce { acc, elem: None, init: zero, step: add(node(IrExprKind::LitInt { value: 1 }, Ty::Int)) })
    }
}

/// The fold body from `steps` on: `elem` is the element as the next stage
/// receives it.
fn fused_body(steps: &[Step], reduce: &Reduce, vars: &mut VarTable, elem: IrExpr) -> IrExpr {
    let Some((step, rest)) = steps.split_first() else {
        return match reduce.elem {
            Some(p) => bound(p, elem, reduce.step.clone(), vars),
            // The element is still computed: the stage that made it may abort.
            None if matches!(elem.kind, IrExprKind::Var { .. }) => reduce.step.clone(),
            None => {
                let unread = vars.alloc_fresh("fused_elem", elem.ty.clone(), Mutability::Let, None);
                bound(unread, elem, reduce.step.clone(), vars)
            }
        };
    };
    let received = var(step.param, vars.get(step.param).ty.clone());
    let then = if step.keeps {
        let kept = fused_body(rest, reduce, vars, received);
        let acc_ty = kept.ty.clone();
        let skipped = var(reduce.acc, acc_ty.clone());
        node(IrExprKind::If { cond: Box::new(step.body.clone()), then: Box::new(kept), else_: Box::new(skipped) }, acc_ty)
    } else {
        fused_body(rest, reduce, vars, step.body.clone())
    };
    bound(step.param, elem, then, vars)
}

/// `{ let p = value; rest }`, or `rest` itself when `value` already is `p`.
fn bound(p: VarId, value: IrExpr, rest: IrExpr, vars: &VarTable) -> IrExpr {
    if matches!(value.kind, IrExprKind::Var { id } if id == p) { return rest; }
    let ty = rest.ty.clone();
    let bind = IrStmt { kind: IrStmtKind::Bind { var: p, mutability: Mutability::Let, ty: vars.get(p).ty.clone(), value }, span: None };
    node(IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(rest)) }, ty)
}

/// The source under the `map` / `filter` calls of `chain`, and those calls
/// in the order they run. A stage whose callback is not a one-parameter
/// lambda literal over scalars ends the chain: it is the source.
fn peel(chain: &IrExpr) -> (&IrExpr, Vec<Step>) {
    let mut steps = Vec::new();
    let mut at = chain;
    while let Some((func @ ("map" | "filter"), [inner, f])) = list_call(at) {
        let IrExprKind::Lambda { params, body, .. } = &f.kind else { break; };
        let [(param, param_ty)] = params.as_slice() else { break; };
        if !scalar(param_ty) || !list_elem(&at.ty).is_some_and(scalar) { break; }
        steps.push(Step { param: *param, body: (**body).clone(), keeps: func == "filter" });
        at = inner;
    }
    steps.reverse();
    (at, steps)
}

/// The name and arguments of `e` when it is a call into the `list` module.
fn list_call(e: &IrExpr) -> Option<(&str, &[IrExpr])> {
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind else { return None; };
    (module.as_str() == "list").then(|| (func.as_str(), args.as_slice()))
}

fn list_elem(t: &Ty) -> Option<&Ty> {
    match t {
        Ty::Applied(almide_lang::types::TypeConstructorId::List, a) => a.first(),
        _ => None,
    }
}

fn scalar(t: &Ty) -> bool { matches!(t, Ty::Int | Ty::Float | Ty::Bool) }

fn var(id: VarId, ty: Ty) -> IrExpr { node(IrExprKind::Var { id }, ty) }

fn node(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}
