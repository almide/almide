//! Tail Call Optimization pass: converts self-recursive tail calls into loops.
//!
//! Transforms:
//! ```text
//! fn sum_to(n: Int, acc: Int) -> Int =
//!   if n <= 0 then acc else sum_to(n - 1, acc + n)
//! ```
//! Into (conceptually):
//! ```text
//! fn sum_to(n: Int, acc: Int) -> Int {
//!   var __tco_result = <default>
//!   while true {
//!     if n <= 0 { __tco_result = acc; break }
//!     else { let __t0 = n - 1; let __t1 = acc + n; n = __t0; acc = __t1; continue }
//!   }
//!   __tco_result
//! }
//! ```
//!
//! This eliminates stack growth for self-recursive tail calls, critical for
//! WASM where the stack is limited and there is no native tail call support.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use almide_ir::*;
use almide_lang::types::Ty;
use almide_lang::types::constructor::TypeConstructorId;
use super::pass::{NanoPass, PassResult, Target};

// Param indices for the currently-being-rewritten TCO function whose borrow
// should be preserved across loop iterations (currently: Bytes params).
// Filled in `rewrite_to_loop`, read by `emit_tail_call_replacement` to decide
// whether to strip a `Borrow` wrapper from that arg position.
thread_local! {
    static TCO_BORROWED_PARAMS: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
}

#[derive(Debug)]
pub struct TailCallOptPass;

impl NanoPass for TailCallOptPass {
    fn name(&self) -> &str { "TailCallOpt" }

    fn targets(&self) -> Option<Vec<Target>> {
        None // All targets benefit from TCO
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Collect: TCO'd function name → param positions whose borrow annotation
        // was forced back to Own by the loop rewrite (i.e. NOT in the
        // Bytes-borrow-preserved set). External call sites targeting these
        // functions need their Borrow wrappers stripped to match the new
        // signature — otherwise a &str arg is passed where String is expected.
        let mut reverted: HashMap<almide_base::intern::Sym, HashSet<usize>> = HashMap::new();
        let IrProgram { functions, modules, var_table, .. } = &mut program;
        run_tco(functions, var_table, &mut reverted);
        for module in modules.iter_mut() {
            run_tco(&mut module.functions, var_table, &mut reverted);
        }
        if !reverted.is_empty() {
            strip_borrows_at_tco_calls(&mut program, &reverted);
        }
        PassResult { program, changed: true }
    }
}

fn run_tco(
    functions: &mut [IrFunction],
    var_table: &mut VarTable,
    reverted: &mut HashMap<almide_base::intern::Sym, HashSet<usize>>,
) {
    for func in functions.iter_mut() {
        if is_tco_candidate(func) {
            let fn_name = func.name.clone();
            let reverted_here = rewrite_to_loop(func, var_table);
            if !reverted_here.is_empty() {
                reverted.insert(fn_name, reverted_here);
            }
        } else if is_binary_rec_candidate(func) {
            rewrite_binary_rec(func, var_table);
        }
    }
}

/// Check if function has the pattern: if cond then base else self(a) + self(b)
/// where + is an associative/commutative binary op (Add, Mul, etc.)
fn is_binary_rec_candidate(func: &IrFunction) -> bool {
    if func.params.len() != 1 { return false; }
    match &func.ret_ty {
        Ty::Int | Ty::Float => {}
        _ => return false,
    }
    match &func.body.kind {
        IrExprKind::If { cond: _, then: _, else_ } => {
            matches_binary_self_call(else_, func.name.as_str())
        }
        _ => false,
    }
}

/// Check if expr is BinOp(Add/Mul, self_call(...), self_call(...))
fn matches_binary_self_call(expr: &IrExpr, fn_name: &str) -> bool {
    if let IrExprKind::BinOp { op, left, right } = &expr.kind {
        use almide_ir::BinOp::*;
        match op {
            AddInt | AddFloat => {}
            _ => return false,
        }
        is_self_call(left, fn_name) && is_self_call(right, fn_name)
    } else {
        false
    }
}

fn is_self_call(expr: &IrExpr, fn_name: &str) -> bool {
    matches!(&expr.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. } if name.as_str() == fn_name)
}

/// Rewrite binary recursion: f(n) = if n<=1 then n else f(n-1) + f(n-2)
/// Into: f(n) = { var acc = 0; while n > 1 { acc += f(n-1); n -= step }; acc + base(n) }
fn rewrite_binary_rec(func: &mut IrFunction, var_table: &mut VarTable) {
    let fn_name = func.name.clone();
    let param = func.params[0].clone();
    let ret_ty = func.ret_ty.clone();

    let (cond, base_val, binop, left_call, right_call) = {
        if let IrExprKind::If { cond, then, else_ } = std::mem::replace(
            &mut func.body.kind,
            IrExprKind::LitInt { value: 0 },
        ) {
            if let IrExprKind::BinOp { op, left, right } = else_.kind {
                (*cond, *then, op, *left, *right)
            } else {
                // Restore and bail
                func.body.kind = IrExprKind::If { cond, then, else_ };
                return;
            }
        } else {
            return;
        }
    };

    // Extract step from right_call: self(n - step) → step value
    // left_call: self(n - 1), right_call: self(n - 2) typically
    let step = extract_subtraction_const(&right_call, param.var);

    if step.is_none() {
        // Restore original
        func.body.kind = IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(base_val),
            else_: Box::new(IrExpr {
                kind: IrExprKind::BinOp { op: binop.clone(), left: Box::new(left_call), right: Box::new(right_call) },
                ty: ret_ty.clone(), span: None, def_id: None,
            }),
        };
        return;
    }
    let step_val = step.unwrap();

    // Create: var n_var = n; var acc = 0;
    let n_var = var_table.alloc(
        almide_base::intern::sym("__br_n"), ret_ty.clone(),
        almide_ir::Mutability::Var, None,
    );
    let acc_var = var_table.alloc(
        almide_base::intern::sym("__br_acc"), ret_ty.clone(),
        almide_ir::Mutability::Var, None,
    );

    let span = func.body.span;
    let zero = match &ret_ty {
        Ty::Int => IrExpr { kind: IrExprKind::LitInt { value: 0 }, ty: Ty::Int, span, def_id: None },
        Ty::Float => IrExpr { kind: IrExprKind::LitFloat { value: 0.0 }, ty: Ty::Float, span, def_id: None },
        _ => return,
    };

    // Bind n_var = param
    let bind_n = IrStmt {
        kind: IrStmtKind::Bind {
            var: n_var, mutability: almide_ir::Mutability::Var,
            ty: ret_ty.clone(),
            value: IrExpr { kind: IrExprKind::Var { id: param.var }, ty: ret_ty.clone(), span, def_id: None },
        },
        span,
    };

    // Bind acc = 0
    let bind_acc = IrStmt {
        kind: IrStmtKind::Bind {
            var: acc_var, mutability: almide_ir::Mutability::Var,
            ty: ret_ty.clone(),
            value: zero.clone(),
        },
        span,
    };

    // Substitute param.var → n_var in cond
    let mut loop_cond = cond.clone();
    substitute_var(&mut loop_cond, param.var, n_var);
    // Negate: while !(base_cond) → while n > 1
    let negated_cond = IrExpr {
        kind: IrExprKind::UnOp { op: almide_ir::UnOp::Not, operand: Box::new(loop_cond) },
        ty: Ty::Bool, span, def_id: None,
    };

    // Loop body: acc = acc + self(n_var - 1); n_var = n_var - step
    let mut call_expr = left_call.clone();
    substitute_var(&mut call_expr, param.var, n_var);

    let acc_update = IrStmt {
        kind: IrStmtKind::Assign {
            var: acc_var,
            value: IrExpr {
                kind: IrExprKind::BinOp {
                    op: binop.clone(),
                    left: Box::new(IrExpr { kind: IrExprKind::Var { id: acc_var }, ty: ret_ty.clone(), span, def_id: None }),
                    right: Box::new(call_expr),
                },
                ty: ret_ty.clone(), span, def_id: None,
            },
        },
        span,
    };

    let n_update = IrStmt {
        kind: IrStmtKind::Assign {
            var: n_var,
            value: IrExpr {
                kind: IrExprKind::BinOp {
                    op: almide_ir::BinOp::SubInt,
                    left: Box::new(IrExpr { kind: IrExprKind::Var { id: n_var }, ty: ret_ty.clone(), span, def_id: None }),
                    right: Box::new(IrExpr { kind: IrExprKind::LitInt { value: step_val }, ty: Ty::Int, span, def_id: None }),
                },
                ty: ret_ty.clone(), span, def_id: None,
            },
        },
        span,
    };

    // while loop
    let while_expr = IrExpr {
        kind: IrExprKind::While {
            cond: Box::new(negated_cond),
            body: vec![acc_update, n_update],
        },
        ty: Ty::Unit, span, def_id: None,
    };

    // Return: acc + base_val(n_var)
    let mut final_base = base_val.clone();
    substitute_var(&mut final_base, param.var, n_var);

    let result = IrExpr {
        kind: IrExprKind::BinOp {
            op: binop,
            left: Box::new(IrExpr { kind: IrExprKind::Var { id: acc_var }, ty: ret_ty.clone(), span, def_id: None }),
            right: Box::new(final_base),
        },
        ty: ret_ty.clone(), span, def_id: None,
    };

    func.body = IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![bind_n, bind_acc, IrStmt { kind: IrStmtKind::Expr { expr: while_expr }, span }],
            expr: Some(Box::new(result)),
        },
        ty: ret_ty, span, def_id: None,
    };
}

fn extract_subtraction_const(call_expr: &IrExpr, param_var: VarId) -> Option<i64> {
    if let IrExprKind::Call { args, .. } = &call_expr.kind {
        if let Some(arg) = args.first() {
            if let IrExprKind::BinOp { op: almide_ir::BinOp::SubInt, left, right } = &arg.kind {
                if let IrExprKind::Var { id } = &left.kind {
                    if *id == param_var {
                        if let IrExprKind::LitInt { value: n } = &right.kind {
                            return Some(*n);
                        }
                    }
                }
            }
        }
    }
    None
}

fn substitute_var(expr: &mut IrExpr, from: VarId, to: VarId) {
    match &mut expr.kind {
        IrExprKind::Var { id } if *id == from => { *id = to; }
        IrExprKind::BinOp { left, right, .. } => {
            substitute_var(left, from, to);
            substitute_var(right, from, to);
        }
        IrExprKind::UnOp { operand, .. } => substitute_var(operand, from, to),
        IrExprKind::If { cond, then, else_ } => {
            substitute_var(cond, from, to);
            substitute_var(then, from, to);
            substitute_var(else_, from, to);
        }
        IrExprKind::Call { args, .. } => {
            for arg in args { substitute_var(arg, from, to); }
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts {
                match &mut s.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => substitute_var(value, from, to),
                    IrStmtKind::Expr { expr } => substitute_var(expr, from, to),
                    // Explicit-preserve: substitute_var only descends into the
                    // statement forms produced by the binary-rec rewrite
                    // (Bind / Assign / Expr). Other statement kinds are never
                    // traversed here — no-op, total-by-construction.
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
            }
            if let Some(e) = expr { substitute_var(e, from, to); }
        }
        // Explicit-preserve: substitute_var performs a surgical single-VarId
        // replacement over only the forms the binary-rec rewrite constructs.
        // Recursing more would risk double-processing. Leaves and every other
        // variant are no-ops — total-by-construction.
        IrExprKind::Var { .. }
        | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::FnRef { .. }
        | IrExprKind::Match { .. } | IrExprKind::Fan { .. }
        | IrExprKind::ForIn { .. } | IrExprKind::While { .. }
        | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::TailCall { .. } | IrExprKind::RuntimeCall { .. }
        | IrExprKind::List { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::EmptyMap | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::Tuple { .. }
        | IrExprKind::Range { .. } | IrExprKind::Member { .. }
        | IrExprKind::TupleIndex { .. } | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. } | IrExprKind::Lambda { .. }
        | IrExprKind::StringInterp { .. }
        | IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
        | IrExprKind::OptionSome { .. } | IrExprKind::OptionNone
        | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. }
        | IrExprKind::UnwrapOr { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. } | IrExprKind::Await { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. } | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
}

/// Visit every Call/TailCall in the program; for any target that was TCO'd and
/// had borrows reverted, strip the Borrow wrapper at the affected arg positions.
struct TcoCallStripper<'a> {
    reverted: &'a HashMap<almide_base::intern::Sym, HashSet<usize>>,
}

impl<'a> IrMutVisitor for TcoCallStripper<'a> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            | IrExprKind::TailCall { target: CallTarget::Named { name }, args } = &mut expr.kind
        {
            if let Some(positions) = self.reverted.get(name) {
                for (i, arg) in args.iter_mut().enumerate() {
                    if positions.contains(&i) {
                        if let IrExprKind::Borrow { expr: inner, .. } = &mut arg.kind {
                            let taken = std::mem::replace(inner.as_mut(), IrExpr {
                                kind: IrExprKind::Unit,
                                ty: Ty::Unit,
                                span: None, def_id: None,
                            });
                            *arg = taken;
                        }
                    }
                }
            }
        }
        walk_expr_mut(self, expr);
    }
}

fn strip_borrows_at_tco_calls(
    program: &mut IrProgram,
    reverted: &HashMap<almide_base::intern::Sym, HashSet<usize>>,
) {
    let mut stripper = TcoCallStripper { reverted };
    for func in &mut program.functions {
        stripper.visit_expr_mut(&mut func.body);
    }
    for module in &mut program.modules {
        for func in &mut module.functions {
            stripper.visit_expr_mut(&mut func.body);
        }
    }
}

/// Returns true if the function is eligible for TCO:
/// - Has at least one self-recursive call
/// - ALL self-recursive calls are in tail position
/// - Not a test helper (name starts with `__test_`)
/// - Return type can be default-initialized (primitives, tuples of primitives, etc.)
fn is_tco_candidate(func: &IrFunction) -> bool {
    if func.name.starts_with("__test_") {
        return false;
    }
    if !can_default_init(&func.ret_ty) {
        return false;
    }
    let (has_any, all_in_tail) = all_self_calls_in_tail_pos(&func.body, &func.name, func.is_effect);
    has_any && all_in_tail
}

/// Scan an expression tree, returning (has_any_self_call, all_self_calls_in_tail_position).
///
/// "Tail position" means:
/// - The expression itself (top-level body)
/// - Last expression in a Block
/// - Both branches of an If
/// - All arm bodies in a Match
///
/// NOT tail position:
/// - Condition of If
/// - Subject of Match
/// - Inside BinOp, UnOp, or any compound expression
/// - Block.stmts (only Block.expr can be tail)
fn all_self_calls_in_tail_pos(expr: &IrExpr, fn_name: &str, is_effect: bool) -> (bool, bool) {
    match &expr.kind {
        // Direct self-call in tail position
        IrExprKind::Call { target: CallTarget::Named { name }, .. } if name == fn_name => {
            (true, true)
        }

        // If: condition is NOT tail, both branches ARE tail
        IrExprKind::If { cond, then, else_ } => {
            let (cond_has, cond_all) = scan_non_tail(cond, fn_name);
            if cond_has && !cond_all {
                return (true, false);
            }
            let (then_has, then_all) = all_self_calls_in_tail_pos(then, fn_name, is_effect);
            let (else_has, else_all) = all_self_calls_in_tail_pos(else_, fn_name, is_effect);
            let has = cond_has || then_has || else_has;
            let all = (!cond_has || cond_all) && (!then_has || then_all) && (!else_has || else_all);
            (has, all)
        }

        // Match: subject is NOT tail, arm bodies ARE tail
        IrExprKind::Match { subject, arms } => {
            let (subj_has, subj_all) = scan_non_tail(subject, fn_name);
            if subj_has && !subj_all {
                return (true, false);
            }
            let (has, all) = arms.iter().fold((subj_has, !subj_has || subj_all), |(has, all), arm| {
                let (arm_has, arm_all) = all_self_calls_in_tail_pos(&arm.body, fn_name, is_effect);
                let (g_has, g_all) = arm.guard.as_ref().map_or((false, true), |g| scan_non_tail(g, fn_name));
                (has || arm_has || g_has, all && (!arm_has || arm_all) && (!g_has || g_all))
            });
            (has, all)
        }

        // Block: stmts are NOT tail, only the trailing expr is tail

        IrExprKind::Block { stmts, expr } => {
            let (has, all) = stmts.iter().fold((false, true), |(has, all), stmt| {
                let (s_has, s_all) = scan_non_tail_stmt(stmt, fn_name);
                (has || s_has, all && (!s_has || s_all))
            });
            let (has, all) = expr.as_ref().map_or((has, all), |tail| {
                let (t_has, t_all) = all_self_calls_in_tail_pos(tail, fn_name, is_effect);
                (has || t_has, all && (!t_has || t_all))
            });
            (has, all)
        }

        // #557: `expr!` / `expr?` wrapping a tail self-call. Since auto-? moved
        // into the frontend (cc70ebc4), an effect fn's tail self-call reaches
        // TCO already wrapped as `Try{Call self}` on every arm — without this
        // arm `is_tco_candidate` returned false and the loop conversion was
        // lost, so every implicitly-lifted recursive `effect fn` ran O(n) stack
        // and crashed on BOTH targets. A `?` on a tail self-call is still a tail
        // call: Ok continues the recursion, Err short-circuits (and in loop form
        // the only Err sources are base cases, so the `?` is subsumed).
        IrExprKind::Try { expr: inner } | IrExprKind::Unwrap { expr: inner } => {
            match &inner.kind {
                IrExprKind::Call { target: CallTarget::Named { name }, .. } if name == fn_name => (true, true),
                _ => scan_non_tail(inner, fn_name),
            }
        }

        // #557: on the WASM arm ResultPropagation runs BEFORE TCO, so an effect
        // fn's tail is already `Ok(<tail>)`. Treat the Ok wrapper as
        // tail-transparent for effect fns (it is the propagation wrapper, not a
        // user value) so `Ok(Try(Call self))` / `Ok(0)` are seen in tail
        // position. For non-effect fns Ok is a real construction and stays
        // opaque.
        IrExprKind::ResultOk { expr: inner } if is_effect => {
            all_self_calls_in_tail_pos(inner, fn_name, is_effect)
        }

        // Anything else: scan for non-tail self-calls.
        // Explicit-preserve: every remaining variant (including a Call that is
        // NOT a self-call, which falls through the guarded arm above) delegates
        // to scan_non_tail — same RHS the catch-all had, total-by-construction.
        IrExprKind::Call { .. }
        | IrExprKind::TailCall { .. } | IrExprKind::RuntimeCall { .. }
        | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::Fan { .. } | IrExprKind::ForIn { .. }
        | IrExprKind::While { .. } | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::List { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::EmptyMap | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::Tuple { .. }
        | IrExprKind::Range { .. } | IrExprKind::Member { .. }
        | IrExprKind::TupleIndex { .. } | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. } | IrExprKind::Lambda { .. }
        | IrExprKind::StringInterp { .. }
        | IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
        | IrExprKind::OptionSome { .. } | IrExprKind::OptionNone
        | IrExprKind::UnwrapOr { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. } | IrExprKind::Await { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. } | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. }
            => scan_non_tail(expr, fn_name),
    }
}

/// Check whether any expression in an iterator contains a self-call (non-tail).
/// Returns `(has_any, !has_any)` — the `all` component is simply the negation of `has`.
fn any_has_self_call<'a>(exprs: impl Iterator<Item = &'a IrExpr>, fn_name: &str) -> (bool, bool) {
    let has = exprs.fold(false, |has, e| has || scan_non_tail(e, fn_name).0);
    (has, !has)
}

/// Scan an expression that is NOT in tail position. Any self-call found here
/// means the function has a non-tail self-call.
fn scan_non_tail(expr: &IrExpr, fn_name: &str) -> (bool, bool) {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if name == fn_name => {
            // Self-call in non-tail position: disqualify
            // But also scan args for additional self-calls
            let has = args.iter().fold(true, |has, arg| has || scan_non_tail(arg, fn_name).0);
            (has, false)
        }
        IrExprKind::Call { target, args, .. } => {
            let target_has = match target {
                CallTarget::Computed { callee } => scan_non_tail(callee, fn_name).0,
                CallTarget::Method { object, .. } => scan_non_tail(object, fn_name).0,
                _ => false,
            };
            let has = args.iter().fold(target_has, |has, arg| has || scan_non_tail(arg, fn_name).0);
            (has, !has)
        }
        IrExprKind::BinOp { left, right, .. } => {
            let has = scan_non_tail(left, fn_name).0 || scan_non_tail(right, fn_name).0;
            (has, !has)
        }
        IrExprKind::UnOp { operand, .. } => {
            scan_non_tail(operand, fn_name)
        }
        IrExprKind::If { cond, then, else_ } => {
            let has = scan_non_tail(cond, fn_name).0
                || scan_non_tail(then, fn_name).0
                || scan_non_tail(else_, fn_name).0;
            (has, !has)
        }
        IrExprKind::Match { subject, arms } => {
            let has = arms.iter().fold(scan_non_tail(subject, fn_name).0, |has, arm| {
                let g_has = arm.guard.as_ref().map_or(false, |g| scan_non_tail(g, fn_name).0);
                has || scan_non_tail(&arm.body, fn_name).0 || g_has
            });
            (has, !has)
        }
        IrExprKind::Block { stmts, expr } => {
            let has = stmts.iter().fold(false, |has, stmt| has || scan_non_tail_stmt(stmt, fn_name).0);
            let has = expr.as_ref().map_or(has, |e| has || scan_non_tail(e, fn_name).0);
            (has, !has)
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            any_has_self_call(elements.iter(), fn_name)
        }
        IrExprKind::Record { fields, .. } => {
            any_has_self_call(fields.iter().map(|(_, v)| v), fn_name)
        }
        IrExprKind::Lambda { body, .. } => {
            // Lambdas are independent scopes; a self-call in a lambda
            // is not a direct self-recursive tail call
            let (b_has, _) = scan_non_tail(body, fn_name);
            (b_has, !b_has)
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            scan_non_tail(expr, fn_name)
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            let has = scan_non_tail(expr, fn_name).0 || scan_non_tail(fallback, fn_name).0;
            (has, !has)
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            scan_non_tail(object, fn_name)
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            let has = scan_non_tail(object, fn_name).0 || scan_non_tail(index, fn_name).0;
            (has, !has)
        }
        IrExprKind::SpreadRecord { base, fields } => {
            let has = fields.iter().fold(scan_non_tail(base, fn_name).0, |has, (_, v)| {
                has || scan_non_tail(v, fn_name).0
            });
            (has, !has)
        }
        IrExprKind::StringInterp { parts } => {
            let has = parts.iter().fold(false, |has, p| {
                if let IrStringPart::Expr { expr } = p { has || scan_non_tail(expr, fn_name).0 } else { has }
            });
            (has, !has)
        }
        IrExprKind::MapLiteral { entries } => {
            let has = entries.iter().fold(false, |has, (k, v)| {
                has || scan_non_tail(k, fn_name).0 || scan_non_tail(v, fn_name).0
            });
            (has, !has)
        }
        IrExprKind::Range { start, end, .. } => {
            let has = scan_non_tail(start, fn_name).0 || scan_non_tail(end, fn_name).0;
            (has, !has)
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            let has = body.iter().fold(scan_non_tail(iterable, fn_name).0, |has, stmt| {
                has || scan_non_tail_stmt(stmt, fn_name).0
            });
            (has, !has)
        }
        IrExprKind::While { cond, body } => {
            let has = body.iter().fold(scan_non_tail(cond, fn_name).0, |has, stmt| {
                has || scan_non_tail_stmt(stmt, fn_name).0
            });
            (has, !has)
        }
        IrExprKind::Fan { exprs } => {
            any_has_self_call(exprs.iter(), fn_name)
        }
        IrExprKind::RustMacro { args, .. } => {
            any_has_self_call(args.iter(), fn_name)
        }
        // Leaf nodes (and codegen-internal nodes that never carry a TCO-relevant
        // self-call): no self-calls. Explicit-preserve — same RHS the catch-all
        // had, total-by-construction so a new IrExprKind is a compile error here.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::OptionNone | IrExprKind::EmptyMap
        | IrExprKind::TailCall { .. } | IrExprKind::RuntimeCall { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => (false, true),
    }
}

fn scan_non_tail_stmt(stmt: &IrStmt, fn_name: &str) -> (bool, bool) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
            scan_non_tail(value, fn_name)
        }
        IrStmtKind::BindDestructure { value, .. } => {
            scan_non_tail(value, fn_name)
        }
        IrStmtKind::Expr { expr } => {
            scan_non_tail(expr, fn_name)
        }
        IrStmtKind::Guard { cond, else_ } => {
            let (c_has, _) = scan_non_tail(cond, fn_name);
            let (e_has, _) = scan_non_tail(else_, fn_name);
            let has = c_has || e_has;
            (has, !has)
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            let (i_has, _) = scan_non_tail(index, fn_name);
            let (v_has, _) = scan_non_tail(value, fn_name);
            let has = i_has || v_has;
            (has, !has)
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            let (k_has, _) = scan_non_tail(key, fn_name);
            let (v_has, _) = scan_non_tail(value, fn_name);
            let has = k_has || v_has;
            (has, !has)
        }
        IrStmtKind::FieldAssign { value, .. } => {
            scan_non_tail(value, fn_name)
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            let (a_has, _) = scan_non_tail(a, fn_name);
            let (b_has, _) = scan_non_tail(b, fn_name);
            let has = a_has || b_has;
            (has, !has)
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            scan_non_tail(end, fn_name)
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            scan_non_tail(len, fn_name)
        }
        IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => (false, true),
        IrStmtKind::Comment { .. } => (false, true),
    }
}

/// Rewrite a TCO-eligible function body from recursive form to a loop.
fn rewrite_to_loop(func: &mut IrFunction, var_table: &mut VarTable) -> HashSet<usize> {
    let fn_name = func.name.clone();
    // For effect fns returning Result[T, E], the TCO result variable should hold T
    // because the Rust codegen auto-unwraps Result via `?` operator.
    let ret_ty = if func.is_effect {
        match &func.ret_ty {
            Ty::Applied(TypeConstructorId::Result, args) if !args.is_empty() => args[0].clone(),
            _ => func.ret_ty.clone(),
        }
    } else {
        func.ret_ty.clone()
    };

    // Mark all param VarIds as mutable (they'll be reassigned in the loop).
    //
    // Historically all borrow annotations were reset to Own here, because a
    // param that persists across loop iterations needs an owned value. BUT
    // that causes massive clones in binary parsers: a self-recursive walker
    // over a 77MB Bytes buffer would clone the buffer on every iteration.
    //
    // For Bytes specifically, holding a `&Vec<u8>` across iterations is
    // safe — the reference targets something owned by the outermost caller,
    // and reassigning it inside the loop (to the same or a derived &Vec<u8>)
    // is a no-op lifetime-wise. Keep the borrow there; force ownership
    // everywhere else.
    let mut bytes_borrowed_params: HashSet<usize> = HashSet::new();
    let mut reverted_to_own: HashSet<usize> = HashSet::new();
    for (i, param) in func.params.iter_mut().enumerate() {
        var_table.entries[param.var.0 as usize].mutability = Mutability::Var;
        let keep_borrow = matches!(param.ty, Ty::Bytes)
            && !matches!(param.borrow, almide_ir::ParamBorrow::Own);
        if keep_borrow {
            bytes_borrowed_params.insert(i);
        } else {
            // If BorrowInsertion had previously marked this param as Borrow,
            // external call sites are now emitting &str / &Vec where we now
            // expect owned. Record so the call-site walker can strip wrappers.
            if !matches!(param.borrow, almide_ir::ParamBorrow::Own) {
                reverted_to_own.insert(i);
            }
            param.borrow = almide_ir::ParamBorrow::Own;
        }
    }
    TCO_BORROWED_PARAMS.with(|s| *s.borrow_mut() = bytes_borrowed_params.clone());

    // Allocate a result variable
    let result_var = var_table.alloc(
        "__tco_result".into(),
        ret_ty.clone(),
        Mutability::Var,
        None,
    );

    // Allocate temporaries for each param. For borrow-preserved params the
    // temp's type is `Ty::Unknown` so the walker emits `let __tco_tmp_data = data;`
    // (no explicit type annotation). That lets Rust infer `&Vec<u8>` from the
    // RHS, which matches the reassignment `data = __tco_tmp_data`.
    let temps: Vec<(VarId, Ty)> = func.params.iter().enumerate().map(|(i, p)| {
        let tmp_ty = if bytes_borrowed_params.contains(&i) {
            Ty::Unknown
        } else {
            p.ty.clone()
        };
        let tmp = var_table.alloc(
            format!("__tco_tmp_{}", p.name).into(),
            tmp_ty.clone(),
            Mutability::Let,
            None,
        );
        (tmp, tmp_ty)
    }).collect();

    // Collect param info for rewrite
    let params: Vec<(VarId, Ty)> = func.params.iter()
        .map(|p| (p.var, p.ty.clone()))
        .collect();

    // Close the tail-recursive heap-accumulator leak: when a self-tail-call
    // overwrites a heap param P with a FRESH allocation each iteration (a List /
    // Record / Map / String literal or a string concat — `tco_managed_params`),
    // the OLD value of P is dead the instant it is reassigned, and the new value
    // cannot alias it (a literal allocates a new block). Such a `dec_param` gets:
    //   - an entry `Inc` (the loop ADOPTS ownership of the caller's borrowed-in
    //     value so the first per-iteration `Dec` releases the loop's added ref,
    //     not the caller's still-live one — see `body_stmts` below),
    //   - a `Dec` before every loop reassignment (free the dead old value),
    //   - a `Dec` at every base case whose result is NON-heap (free the final
    //     value); a heap-typed base-case result might carry P out to the caller,
    //     so its Dec is suppressed (the caller frees it) — see `emit_base_case`.
    // Params reassigned via a bare carry (`f(P, …)`), a possibly-in-place-reusing
    // call (`f(list.drop(P,1))`), or any non-literal arg are left UNMANAGED: their
    // new value may alias the old, so a Dec could use-after-free. Conservative —
    // those keep the pre-existing (bounded-per-call) leak rather than risk a
    // double-free now that frees are live.
    let dec_params: Vec<VarId> = tco_managed_params(&func.body, &params, fn_name.as_str());
    if std::env::var("ALMIDE_TCO_DEBUG").is_ok() {
        let names = |vs: &[VarId]| vs.iter().map(|v| var_table.get(*v).name.as_str().to_string()).collect::<Vec<_>>();
        eprintln!("[tco] {} dec_params={:?}", fn_name.as_str(), names(&dec_params));
    }

    // Rewrite the body expression
    let old_body = std::mem::take(&mut func.body);
    let is_effect = func.is_effect;
    let rewritten = rewrite_tail_expr(old_body, &fn_name, &params, &temps, result_var, is_effect, &dec_params);

    // Build the default value for the result variable
    let default_val = default_for_type(&ret_ty);

    // Construct: { var __tco_result = default; while true { rewritten_body }; __tco_result }
    let bind_result = IrStmt {
        kind: IrStmtKind::Bind {
            var: result_var,
            mutability: Mutability::Var,
            ty: ret_ty.clone(),
            value: default_val,
        },
        span: None,
    };

    // The while body is a single Expr statement wrapping the rewritten body
    let while_body_stmt = IrStmt {
        kind: IrStmtKind::Expr { expr: rewritten },
        span: None,
    };

    // The while body is just the rewritten body. (A previous trailing
    // `RcDec __tco_tmp_*` here was both DEAD — every path through the rewritten
    // body ends in `continue` or `break`, so a statement after it is unreachable —
    // and WRONG: each temp is MOVED into its param (`param = __tco_tmp`), so
    // Dec-ing it would free the live loop value. The old-value Dec for heap params
    // is now emitted correctly at the reassignment / base-case sites; see
    // `dec_params` and `emit_tail_call_replacement` / `emit_base_case`.
    let while_body = vec![while_body_stmt];

    let while_expr = IrExpr {
        kind: IrExprKind::While {
            cond: Box::new(IrExpr {
                kind: IrExprKind::LitBool { value: true },
                ty: Ty::Bool,
                span: None, def_id: None,
            }),
            body: while_body,
        },
        ty: Ty::Unit,
        span: None, def_id: None,
    };

    let while_stmt = IrStmt {
        kind: IrStmtKind::Expr { expr: while_expr },
        span: None,
    };

    let tail_var = IrExpr {
        kind: IrExprKind::Var { id: result_var },
        ty: ret_ty.clone(),
        span: None, def_id: None,
    };

    // For effect fns, wrap the result in Ok() since the function returns Result
    let tail_expr = if func.is_effect {
        IrExpr {
            kind: IrExprKind::ResultOk { expr: Box::new(tail_var) },
            ty: func.ret_ty.clone(),
            span: None, def_id: None,
        }
    } else {
        tail_var
    };

    // Entry Inc for dec-managed heap params: converts the borrowed-in param value
    // to callee-owned, so the uniform `Dec`-before-reassign (and base-case `Dec`)
    // balances on the FIRST iteration too (it releases this added ref, leaving the
    // caller's own ref intact for the caller to Dec). Without it, iteration 1 would
    // free the caller's value early.
    let mut body_stmts = vec![bind_result];
    for p in &dec_params {
        body_stmts.push(IrStmt { kind: IrStmtKind::RcInc { var: *p }, span: None });
    }
    body_stmts.push(while_stmt);

    func.body = IrExpr {
        kind: IrExprKind::Block {
            stmts: body_stmts,
            expr: Some(Box::new(tail_expr)),
        },
        ty: func.ret_ty.clone(),
        span: func.body.span, def_id: None,
    };

    reverted_to_own
}

/// Heap-typed for TCO RC purposes (mirrors Perceus `is_heap_type`).
fn tco_is_heap(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. }
        | Ty::Unknown | Ty::Fn { .. })
}

/// A heap param P is "managed" (entry-Inc + per-reassign Dec + base-case Dec) iff,
/// in EVERY self-tail-call, the argument in P's position is a FRESH allocation:
/// a `List`/`Record`/`Map`/string literal or a string concat. A fresh allocation
/// is a brand-new heap block, so it can never alias P's OLD value — making
/// `Dec(old P)` before the reassignment provably free of use-after-free. (Any
/// heap SUBcomponent of P borrowed into the fresh value, e.g. `Acc{ tag: P.tag }`
/// — would be a move, but a field-read producing a heap value is Inc'd by Perceus
/// Rule-1 on aliasing, so the Dec only frees what is truly dead.) Bare carries,
/// in-place-reusing calls (`list.drop`/`map`/`filter` over a single-use source),
/// and any non-literal arg are conservatively UNMANAGED: their new value might BE
/// the old block, so a Dec could double-free now that frees are live.
///
/// WASM has no `Borrow`/`Clone` IR nodes (those passes are Rust-only), so the
/// fresh-allocation shape — not a borrow annotation — is the soundness signal.
fn tco_managed_params(body: &IrExpr, params: &[(VarId, Ty)], fn_name: &str) -> Vec<VarId> {
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
    // managed[i] starts true; a non-fresh self-call arg flips it false. We do NOT
    // pre-filter by `tco_is_heap(param.ty)`: at TCO time a record/variant param is
    // an unresolved `Ty::Named` (not yet `Ty::Record`), so a type test would miss
    // it. Instead we rely on the arg shape — `is_fresh_alloc` only matches
    // heap-allocating literals (List/Record/Map/String/concat), so "all args fresh"
    // already IMPLIES the param is heap. A non-heap param (e.g. `n` fed `n - 1`)
    // never has a fresh arg, so it is excluded automatically — and is thus never
    // handed a (corruption-prone) rc_dec on a scalar.
    let mut managed: Vec<bool> = vec![true; params.len()];
    struct V<'a> { fn_name: &'a str, managed: &'a mut Vec<bool>, saw_self_call: bool }
    impl IrVisitor for V<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind {
                if name.as_str() == self.fn_name {
                    self.saw_self_call = true;
                    for (i, arg) in args.iter().enumerate() {
                        if i < self.managed.len() && !is_fresh_alloc(arg) {
                            self.managed[i] = false;
                        }
                    }
                }
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) { walk_stmt(self, s); }
    }
    let mut v = V { fn_name, managed: &mut managed, saw_self_call: false };
    v.visit_expr(body);
    if !v.saw_self_call { return Vec::new(); }
    params.iter().zip(managed).filter_map(|((var, _), m)| m.then_some(*var)).collect()
}

/// A self-tail-call argument the LOOP can adopt ownership of. Literal heap
/// allocations are trivially owned. Since the Round-3 ownership discipline,
/// CALL-valued args are adoption-safe too: user fns return OWNED (the
/// return-alias dup, mechanism #6), and after this rewrite turns the arg
/// into a `bind temp = arg; assign param = temp` chain, an alias-shaped
/// temp receives the bind-level Inc at Perceus time — either way the loop
/// holds its own reference, so the per-iteration Dec is balanced. Scalar
/// results are excluded by type (a Dec on a scalar would corrupt), and
/// `Unknown` stays unmanaged (conservative).
fn is_fresh_alloc(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::List { .. } | IrExprKind::Record { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::StringInterp { .. }
        | IrExprKind::BinOp { op: BinOp::ConcatStr, .. } => true,
        IrExprKind::Call { .. } | IrExprKind::RuntimeCall { .. } => match &e.ty {
            Ty::Int | Ty::Float | Ty::Bool | Ty::Unit | Ty::Unknown => false,
            Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32
            | Ty::UInt64 | Ty::Float32 => false,
            _ => true,
        },
        IrExprKind::Block { expr: Some(tail), .. } => is_fresh_alloc(tail),
        _ => false,
    }
}

/// Rewrite an expression in tail position:
/// - Self-calls become: bind temps, assign params, continue
/// - If/Match: recurse into branches
/// - Block: recurse into trailing expr
/// - Anything else (base case): assign to result var, break
fn rewrite_tail_expr(
    expr: IrExpr,
    fn_name: &str,
    params: &[(VarId, Ty)],
    temps: &[(VarId, Ty)],
    result_var: VarId,
    is_effect: bool,
    dec_params: &[VarId],
) -> IrExpr {
    match expr.kind {
        // Self-recursive call in tail position -> reassign params and continue
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if name == fn_name => {
            emit_tail_call_replacement(args, params, temps, result_var, dec_params)
        }

        // #557: `(self-call)?` / `(self-call)!` — a frontend auto-? wrapping a
        // tail self-call loop-converts IDENTICALLY to the bare self-call (the
        // `?` is subsumed by the loop; an Err can only originate at a base
        // case, never the recursion).
        IrExprKind::Try { expr: inner } | IrExprKind::Unwrap { expr: inner }
            if matches!(&inner.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. } if name == fn_name) =>
        {
            match inner.kind {
                IrExprKind::Call { args, .. } => emit_tail_call_replacement(args, params, temps, result_var, dec_params),
                _ => unreachable!("guard guarantees a self-call"),
            }
        }

        // Effect fn: the Ok(..) propagation wrapper is tail-transparent —
        // RECURSE into it so `Ok(Try(Call self))` reaches the tail-call arm and
        // `Ok(0)` reaches the base-case arm (#557). Previously this eagerly
        // emitted the whole inner as a base case, so a wasm-arm
        // `Ok(Try(Call self))` was mis-emitted as a base value (no loop).
        IrExprKind::ResultOk { expr: inner } if is_effect => {
            rewrite_tail_expr(*inner, fn_name, params, temps, result_var, is_effect, dec_params)
        }

        // If: recurse into both branches
        IrExprKind::If { cond, then, else_ } => {
            let new_then = rewrite_tail_expr(*then, fn_name, params, temps, result_var, is_effect, dec_params);
            let new_else = rewrite_tail_expr(*else_, fn_name, params, temps, result_var, is_effect, dec_params);
            IrExpr {
                kind: IrExprKind::If {
                    cond,
                    then: Box::new(new_then),
                    else_: Box::new(new_else),
                },
                ty: Ty::Unit,
                span: expr.span, def_id: None,
            }
        }

        // Match: recurse into arm bodies
        IrExprKind::Match { subject, arms } => {
            let new_arms = arms.into_iter().map(|arm| {
                IrMatchArm {
                    pattern: arm.pattern,
                    guard: arm.guard,
                    body: rewrite_tail_expr(arm.body, fn_name, params, temps, result_var, is_effect, dec_params),
                }
            }).collect();
            IrExpr {
                kind: IrExprKind::Match { subject, arms: new_arms },
                ty: Ty::Unit,
                span: expr.span, def_id: None,
            }
        }

        // Block: recurse into trailing expr
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let new_tail = rewrite_tail_expr(*tail, fn_name, params, temps, result_var, is_effect, dec_params);
            IrExpr {
                kind: IrExprKind::Block {
                    stmts,
                    expr: Some(Box::new(new_tail)),
                },
                ty: Ty::Unit,
                span: expr.span, def_id: None,
            }
        }

        // Base case: assign result and break.
        // Explicit-preserve: every remaining variant — including a Call that is
        // NOT a self-call, a ResultOk in a non-effect fn, and a Block with no
        // trailing expr (all of which fall through the guarded/partial arms
        // above) — is a base case emitted via emit_base_case. Same RHS the
        // catch-all had, total-by-construction.
        //
        // These patterns bind nothing (`{ .. }` / unit variants), so `expr`
        // is not partially moved and remains usable whole — exactly as `_` was.
        IrExprKind::Call { .. } | IrExprKind::ResultOk { .. }
        | IrExprKind::Block { .. }
        | IrExprKind::TailCall { .. } | IrExprKind::RuntimeCall { .. }
        | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::Fan { .. } | IrExprKind::ForIn { .. }
        | IrExprKind::While { .. } | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::List { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::EmptyMap | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::Tuple { .. }
        | IrExprKind::Range { .. } | IrExprKind::Member { .. }
        | IrExprKind::TupleIndex { .. } | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. } | IrExprKind::Lambda { .. }
        | IrExprKind::StringInterp { .. }
        | IrExprKind::ResultErr { .. } | IrExprKind::OptionSome { .. }
        | IrExprKind::OptionNone | IrExprKind::Try { .. }
        | IrExprKind::Unwrap { .. } | IrExprKind::UnwrapOr { .. }
        | IrExprKind::ToOption { .. } | IrExprKind::OptionalChain { .. }
        | IrExprKind::Await { .. } | IrExprKind::Clone { .. }
        | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
        | IrExprKind::RustMacro { .. } | IrExprKind::ToVec { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {
            emit_base_case(expr, result_var, dec_params)
        }
    }
}

/// Emit the replacement for a tail self-call:
/// ```text
/// let __tco_tmp_0 = arg0_expr
/// let __tco_tmp_1 = arg1_expr
/// param0 = __tco_tmp_0
/// param1 = __tco_tmp_1
/// continue
/// ```
/// Strip a Borrow wrapper from an expression (if present).
/// TCO params become owned, so borrow annotations from BorrowInsertion must be removed.
fn strip_borrow(expr: IrExpr) -> IrExpr {
    match expr.kind {
        IrExprKind::Borrow { expr: inner, .. } => *inner,
        _ => expr,
    }
}

fn emit_tail_call_replacement(
    args: Vec<IrExpr>,
    params: &[(VarId, Ty)],
    temps: &[(VarId, Ty)],
    _result_var: VarId,
    dec_params: &[VarId],
) -> IrExpr {
    let mut stmts: Vec<IrStmt> = Vec::new();

    // F5 (#527): an IDENTITY CARRY — argument i is the bare Var of param i —
    // is a semantic no-op; emitting the temp anyway gave the temp bind a
    // Rule-1 alias-Inc with no Dec anywhere (the temp is Dec-exempt by name,
    // an unmanaged param has no per-iteration Dec): +1 rc per iteration, an
    // immortal param block and rc creep toward wrap on long loops. Skip both
    // the bind and the assign for those positions.
    let identity_carry: Vec<bool> = args.iter().enumerate().map(|(i, arg)| {
        matches!(&arg.kind, IrExprKind::Var { id } if *id == params[i].0)
    }).collect();

    // Bind temporaries to argument expressions.
    // Strip Borrow from arg unless this param position is kept-borrowed
    // (e.g. Bytes borrow preserved across iterations).
    for (i, arg) in args.into_iter().enumerate() {
        if identity_carry[i] { continue; }
        let (tmp_var, tmp_ty) = &temps[i];
        let keep = TCO_BORROWED_PARAMS.with(|s| s.borrow().contains(&i));
        let unwrapped = if keep { arg } else { strip_borrow(arg) };
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: *tmp_var,
                mutability: Mutability::Let,
                ty: tmp_ty.clone(),
                value: unwrapped,
            },
            span: None,
        });
    }

    // Free the OLD value of each dec-managed heap param BEFORE overwriting it.
    // The new values are already captured in the temps above (which only borrowed
    // the old params), so the old values are now dead and unaliased — Dec-ing them
    // is safe and is exactly the per-iteration free that closes the accumulator
    // leak. (Done after ALL temp binds so an arg that borrows several params still
    // reads their live old values first.)
    for p in dec_params {
        stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: *p }, span: None });
    }

    // Assign params from temporaries (identity carries skipped — the param
    // already holds its value).
    for (i, (param_var, _)) in params.iter().enumerate() {
        if identity_carry[i] { continue; }
        let (tmp_var, tmp_ty) = &temps[i];
        stmts.push(IrStmt {
            kind: IrStmtKind::Assign {
                var: *param_var,
                value: IrExpr {
                    kind: IrExprKind::Var { id: *tmp_var },
                    ty: tmp_ty.clone(),
                    span: None, def_id: None,
                },
            },
            span: None,
        });
    }

    // Continue the loop
    let continue_expr = IrExpr {
        kind: IrExprKind::Continue,
        ty: Ty::Unit,
        span: None, def_id: None,
    };

    IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(continue_expr)),
        },
        ty: Ty::Unit,
        span: None, def_id: None,
    }
}

/// Emit the base case: assign to result variable and break.
/// ```text
/// __tco_result = expr
/// break
/// ```
fn emit_base_case(expr: IrExpr, result_var: VarId, dec_params: &[VarId]) -> IrExpr {
    // A heap-typed base-case result may MOVE a managed param out to the caller
    // (e.g. `then s` returns the accumulator). Freeing the param here would then
    // be a use-after-free on the returned value, so suppress the exit Dec for a
    // heap result and let the caller own it. A non-heap result (Int/Bool/…) cannot
    // carry a param out, so the params are dead after the assign and freeing the
    // final loop value here balances the entry `Inc`.
    let result_is_heap = tco_is_heap(&expr.ty);

    let assign = IrStmt {
        kind: IrStmtKind::Assign {
            var: result_var,
            value: expr,
        },
        span: None,
    };

    let mut stmts = vec![assign];
    if !result_is_heap {
        for p in dec_params {
            stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: *p }, span: None });
        }
    }

    let break_expr = IrExpr {
        kind: IrExprKind::Break,
        ty: Ty::Unit,
        span: None, def_id: None,
    };

    IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(break_expr)),
        },
        ty: Ty::Unit,
        span: None, def_id: None,
    }
}

/// Produce a default value for a given type (used to initialize the result variable).
/// The value is never observed — every control path assigns before reading — but
/// Rust's type checker requires a valid expression of the correct type.
fn default_for_type(ty: &Ty) -> IrExpr {
    let kind = match ty {
        Ty::Int => IrExprKind::LitInt { value: 0 },
        Ty::Float => IrExprKind::LitFloat { value: 0.0 },
        Ty::Bool => IrExprKind::LitBool { value: false },
        Ty::String => IrExprKind::LitStr { value: String::new() },
        Ty::Unit => IrExprKind::Unit,
        Ty::Applied(TypeConstructorId::Result, args) => {
            let inner_ty = args.first().cloned().unwrap_or(Ty::Unit);
            let inner = default_for_type(&inner_ty);
            IrExprKind::ResultOk { expr: Box::new(inner) }
        }
        Ty::Applied(TypeConstructorId::Option, _) => {
            IrExprKind::OptionNone
        }
        Ty::Applied(TypeConstructorId::List, _) => {
            IrExprKind::List { elements: vec![] }
        }
        Ty::Applied(TypeConstructorId::Map, _) => {
            IrExprKind::MapLiteral { entries: vec![] }
        }
        Ty::Tuple(elems) => {
            IrExprKind::Tuple {
                elements: elems.iter().map(|t| default_for_type(t)).collect(),
            }
        }
        // Named types and other complex types: cannot synthesize a default value.
        // TCO should not be applied to functions returning these types.
        // Return Unit as unreachable placeholder (guarded by can_default_init check).
        _ => IrExprKind::Unit,
    };
    IrExpr {
        kind,
        ty: ty.clone(),
        span: None, def_id: None,
    }
}

/// Returns true if we can produce a valid default value for this type.
/// Types that fail this check should not be TCO'd (the result variable
/// cannot be initialized without unsafe code).
fn can_default_init(ty: &Ty) -> bool {
    match ty {
        Ty::Int | Ty::Float | Ty::Bool | Ty::String | Ty::Unit => true,
        Ty::Applied(TypeConstructorId::Result, args) => {
            args.first().map_or(true, |inner| can_default_init(inner))
        }
        Ty::Applied(TypeConstructorId::Option, _) => true,
        Ty::Applied(TypeConstructorId::List, _) => true,
        Ty::Applied(TypeConstructorId::Map, _) => true,
        Ty::Tuple(elems) => elems.iter().all(|t| can_default_init(t)),
        _ => false,
    }
}
