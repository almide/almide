//! PerceusPass: Insert RcInc/RcDec IR nodes for automatic memory management.
//!
//! Uses Lean 4 FnBody representation internally:
//!   Block IR → FnBody chain → insert Inc/Dec → FnBody chain → Block IR
//!
//! FnBody (continuation-based):
//!   VDecl(var, ty, expr, body)  — let var: ty = expr; then body
//!   Assign(var, expr, body)     — var = expr; then body
//!   Inc(var, body)              — rc_inc(var); then body
//!   Dec(var, body)              — rc_dec(var); then body
//!   Expr(expr, body)            — eval expr (discard); then body
//!   Ret(expr)                   — return expr
//!   Nop                         — end of chain (Unit return)
//!
//! Target: WASM only (Rust handles ownership natively).

use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use std::collections::{HashMap, HashSet};
use super::pass::{NanoPass, PassResult, Target};

// ── Lean 4 FnBody (internal representation for Perceus) ──

/// Continuation-based IR node (Lean 4 style).
/// Each node chains to the next via `body`. No "tail expression" concept.
#[derive(Debug, Clone)]
enum FnBody {
    /// let var: ty = expr; then body
    VDecl { var: VarId, ty: Ty, mutability: Mutability, expr: IrExpr, body: Box<FnBody> },
    /// var = expr; then body
    Assign { var: VarId, expr: IrExpr, body: Box<FnBody> },
    /// rc_inc(var); then body
    Inc { var: VarId, body: Box<FnBody> },
    /// rc_dec(var); then body
    Dec { var: VarId, body: Box<FnBody> },
    /// eval expr (discard result); then body
    Expr { expr: IrExpr, body: Box<FnBody> },
    /// Pass-through: original statement preserved as-is
    Stmt { stmt: IrStmt, body: Box<FnBody> },
    /// return expr
    Ret { expr: IrExpr },
    /// end of chain (Unit return)
    Nop,
}

/// Convert Block IR to FnBody chain.
fn block_to_fnbody(stmts: Vec<IrStmt>, tail: Option<Box<IrExpr>>) -> FnBody {
    let ret = match tail {
        Some(e) => FnBody::Ret { expr: *e },
        None => FnBody::Nop,
    };
    // Build chain in reverse: last stmt wraps ret, second-to-last wraps that, etc.
    stmts.into_iter().rev().fold(ret, |body, stmt| {
        match stmt.kind {
            IrStmtKind::Bind { var, ty, value, mutability } =>
                FnBody::VDecl { var, ty, mutability, expr: value, body: Box::new(body) },
            IrStmtKind::Assign { var, value } =>
                FnBody::Assign { var, expr: value, body: Box::new(body) },
            IrStmtKind::RcInc { var } =>
                FnBody::Inc { var, body: Box::new(body) },
            IrStmtKind::RcDec { var } =>
                FnBody::Dec { var, body: Box::new(body) },
            IrStmtKind::Expr { expr } =>
                FnBody::Expr { expr, body: Box::new(body) },
            _ => FnBody::Stmt { stmt, body: Box::new(body) },
        }
    })
}

/// Convert FnBody chain back to Block IR (stmts + tail).
fn fnbody_to_block(mut fb: FnBody) -> (Vec<IrStmt>, Option<Box<IrExpr>>) {
    let mut stmts = Vec::new();
    loop {
        match fb {
            FnBody::VDecl { var, ty, mutability, expr, body } => {
                stmts.push(IrStmt {
                    kind: IrStmtKind::Bind { var, ty, mutability, value: expr },
                    span: None,
                });
                fb = *body;
            }
            FnBody::Assign { var, expr, body } => {
                stmts.push(IrStmt { kind: IrStmtKind::Assign { var, value: expr }, span: None });
                fb = *body;
            }
            FnBody::Inc { var, body } => {
                stmts.push(IrStmt { kind: IrStmtKind::RcInc { var }, span: None });
                fb = *body;
            }
            FnBody::Dec { var, body } => {
                stmts.push(IrStmt { kind: IrStmtKind::RcDec { var }, span: None });
                fb = *body;
            }
            FnBody::Expr { expr, body } => {
                stmts.push(IrStmt { kind: IrStmtKind::Expr { expr }, span: None });
                fb = *body;
            }
            FnBody::Stmt { stmt, body } => {
                stmts.push(stmt);
                fb = *body;
            }
            FnBody::Ret { expr } => {
                return (stmts, Some(Box::new(expr)));
            }
            FnBody::Nop => {
                return (stmts, None);
            }
        }
    }
}

/// Recursively apply Perceus to expressions (handles nested blocks).
fn perceus_expr(expr: &mut IrExpr, var_table: &mut VarTable) {
    PerceusDriver { var_table }.visit_expr_mut(expr);
}

/// Drives Rc insertion through the exhaustive IrMutVisitor walk. The Block/While/
/// ForIn arms run the FnBody round-trip (the actual Rc insertion) and are kept
/// verbatim; the default delegates to walk_expr_mut so a Block nested inside any
/// previously-dropped kind (call args, IterChain/RcWrap payloads) is reached too.
/// This only ADDS balanced (Lean-certified) Rc processing — never removes it — so
/// it has no silent-leak direction; new placements surface as traps / native==wasm.
struct PerceusDriver<'a> {
    var_table: &'a mut VarTable,
}

impl IrMutVisitor for PerceusDriver<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        match &mut expr.kind {
            IrExprKind::Block { stmts, expr: tail } => {
                let old_stmts = std::mem::take(stmts);
                let old_tail = tail.take();
                let fb = block_to_fnbody(old_stmts, old_tail);
                let fb = perceus_fnbody(fb, self.var_table);
                let fb = insert_ret_decs(fb, self.var_table);
                let (new_stmts, new_tail) = fnbody_to_block(fb);
                *stmts = new_stmts;
                *tail = new_tail;
            }
            IrExprKind::While { cond, body } => {
                self.visit_expr_mut(cond);
                // Convert while body stmts to FnBody chain (Nop terminus)
                let old_body = std::mem::take(body);
                let fb = block_to_fnbody(old_body, None); // None = Nop
                let fb = perceus_fnbody(fb, self.var_table);
                let fb = insert_ret_decs(fb, self.var_table); // Dec before Nop for heap locals
                let (new_body, _) = fnbody_to_block(fb);
                *body = new_body;
            }
            IrExprKind::ForIn { iterable, body, .. } => {
                self.visit_expr_mut(iterable);
                let old_body = std::mem::take(body);
                let fb = block_to_fnbody(old_body, None);
                let fb = perceus_fnbody(fb, self.var_table);
                let fb = insert_ret_decs(fb, self.var_table);
                let (new_body, _) = fnbody_to_block(fb);
                *body = new_body;
            }
            _ => walk_expr_mut(self, expr),
        }
    }
}

// ── Exit-tail-block flattening (pre-Perceus) ────────────────────────────────
//
// TCO rewrites a self-tail-call that is the TAIL of a block into a nested block
// ending in `continue`/`break`:  `Block{ A, tail: Block{ B, tail: continue } }`.
// That nesting breaks heap-local cleanup two ways once frees are live:
//   1. Perceus places a heap local's `RcDec` at the OUTER block end, after the
//      nested `continue` — unreachable dead code, so the local leaks every
//      iteration.
//   2. The flat per-scope verifier (`perceus_verified::count_decs`) only counts
//      a Dec at the SAME nesting level as the `Bind`, so a Dec moved into the
//      nested exit block reads as "no RcDec" (false leak) and refuses the build.
// Flattening the nested exit block UP — `Block{ A ++ B, tail: continue }` —
// makes `continue` the block's own tail. Then `insert_decs_before_ret` naturally
// emits the heap-local Dec as a same-level statement BEFORE the exit: reachable
// (correct execution) AND counted by the verifier. It also dissolves the
// `__perceus_ret = { … continue }` tail-lift bug: with `continue` as the tail,
// `collect_ret_vars` sees no escaping value, so no lift is attempted.
//
// Semantically transparent: a block's value IS its tail, so inlining a
// tail-positioned block preserves meaning. Applied before any Dec insertion.

/// Flatten loop-exit nesting so a `continue`/`break` becomes its enclosing
/// block's own TAIL. Two shapes, applied post-order (innermost first):
///   (a) Trailing discarded-exit statement — `Block{ …, Expr(X) ; tail: None }`
///       where `X` always exits — is the no-value form TCO emits when the
///       self-call is a discarded tail. Promote `X` to the block's tail.
///   (b) Tail block ending in an exit — `Block{ A ; tail: Block{ B ; tail: exit }}`
///       — hoist `B` up and make `exit` the outer tail.
/// After this, `insert_decs_before_ret` sees `continue`/`break` as the terminal
/// and emits each heap-local Dec as a same-level statement BEFORE it: reachable
/// (no leak) and at the Bind's level (counted by the flat verifier).
fn flatten_exit_tail_blocks(body: &mut IrExpr) {
    struct Flattener;
    impl IrMutVisitor for Flattener {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e); // children first (innermost nesting collapses first)
            if let IrExprKind::Block { stmts, expr } = &mut e.kind {
                // (a) Promote a trailing always-exit `Expr` statement to the tail.
                #[allow(clippy::collapsible_if)]
                if expr.is_none() {
                    if let Some(IrStmtKind::Expr { expr: last }) = stmts.last().map(|s| &s.kind) {
                        if expr_always_exits(last) {
                            if let Some(IrStmt { kind: IrStmtKind::Expr { expr: x }, .. }) = stmts.pop() {
                                *expr = Some(Box::new(x));
                            }
                        }
                    }
                }
                // (b) Absorb a tail block that ends in a bare exit.
                if let Some(tail) = expr {
                    while tail_block_ends_in_exit(tail) {
                        if let IrExprKind::Block { stmts: inner, expr: inner_tail } = &mut tail.kind {
                            let mut hoisted = std::mem::take(inner);
                            stmts.append(&mut hoisted);
                            *tail = inner_tail.take().expect("checked by tail_block_ends_in_exit");
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }
    Flattener.visit_expr_mut(body);
}

/// True iff `t` is a `Block` whose own tail is a bare `continue`/`break`.
fn tail_block_ends_in_exit(t: &IrExpr) -> bool {
    matches!(&t.kind, IrExprKind::Block { expr: Some(inner), .. }
        if matches!(inner.kind, IrExprKind::Continue | IrExprKind::Break))
}

/// Unconditionally exits the loop iteration (continue/break), possibly through a
/// trailing block/if/match — but NOT through a nested loop.
fn expr_always_exits(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Continue | IrExprKind::Break => true,
        IrExprKind::Block { stmts, expr } =>
            expr.as_deref().is_some_and(expr_always_exits)
                || stmts.iter().any(|s| matches!(&s.kind,
                    IrStmtKind::Expr { expr } if expr_always_exits(expr))),
        IrExprKind::If { then, else_, .. } => expr_always_exits(then) && expr_always_exits(else_),
        IrExprKind::Match { arms, .. } =>
            !arms.is_empty() && arms.iter().all(|a| expr_always_exits(&a.body)),
        _ => false,
    }
}

/// A `FnBody` chain node with its continuation (`body`) detached. Linearizing
/// the chain into a `Vec<ChainHead>` + terminal lets the per-node passes below
/// fold over a function body ITERATIVELY instead of recursing once per
/// statement. A wide `fn` body (thousands of sibling statements) is an N-deep
/// `FnBody` chain (`block_to_fnbody`); a recursive walk over it grows the native
/// stack with the program's statement count and overflows it (e.g. Windows'
/// 1 MiB main-thread stack). Heads carry only their own data; the terminal
/// (`Ret`/`Nop`) and the per-node logic are applied while re-linking each head
/// to the already-processed remainder — tail-to-head, identical to the former
/// recursion's post-order.
enum ChainHead {
    VDecl { var: VarId, ty: Ty, mutability: Mutability, expr: IrExpr },
    Assign { var: VarId, expr: IrExpr },
    Inc { var: VarId },
    Dec { var: VarId },
    Expr { expr: IrExpr },
    Stmt { stmt: IrStmt },
}

/// Split a `FnBody` chain into its forward-ordered heads and its terminal
/// (`Ret`/`Nop`). Iterative — O(1) native stack regardless of chain length.
fn split_chain(mut fb: FnBody) -> (Vec<ChainHead>, FnBody) {
    let mut heads = Vec::new();
    loop {
        match fb {
            FnBody::VDecl { var, ty, mutability, expr, body } => {
                heads.push(ChainHead::VDecl { var, ty, mutability, expr });
                fb = *body;
            }
            FnBody::Assign { var, expr, body } => {
                heads.push(ChainHead::Assign { var, expr });
                fb = *body;
            }
            FnBody::Inc { var, body } => { heads.push(ChainHead::Inc { var }); fb = *body; }
            FnBody::Dec { var, body } => { heads.push(ChainHead::Dec { var }); fb = *body; }
            FnBody::Expr { expr, body } => { heads.push(ChainHead::Expr { expr }); fb = *body; }
            FnBody::Stmt { stmt, body } => { heads.push(ChainHead::Stmt { stmt }); fb = *body; }
            term @ (FnBody::Ret { .. } | FnBody::Nop) => return (heads, term),
        }
    }
}

/// Insert Perceus Inc/Dec into a FnBody chain. Iterative over the chain so a
/// function body with N statements costs O(1) native stack, not O(N) (see
/// [`ChainHead`]). Folding the heads tail-to-head reproduces the former
/// post-order recursion exactly: each node's `expr` is `perceus_expr`-processed
/// (and `var_table` temps allocated) after the remainder, in the same order.
fn perceus_fnbody(fb: FnBody, var_table: &mut VarTable) -> FnBody {
    let (heads, terminal) = split_chain(fb);
    // Ret/Nop are returned unchanged here (Dec insertion happens in
    // insert_ret_decs), matching the former Ret/Nop arms.
    let mut result = terminal;
    for head in heads.into_iter().rev() {
        result = match head {
            ChainHead::VDecl { var, ty, mutability, mut expr } => {
                // Recurse into the expression (handles nested blocks)
                perceus_expr(&mut expr, var_table);
                // Rule 1: RcInc for heap alias
                let needs_inc = is_heap_type(&ty) && matches!(&expr.kind,
                    IrExprKind::Var { .. } | IrExprKind::Clone { .. } | IrExprKind::Deref { .. });
                let inc_var = if needs_inc {
                    match &expr.kind {
                        IrExprKind::Var { id } => Some(*id),
                        IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e } =>
                            if let IrExprKind::Var { id } = &e.kind { Some(*id) } else { None },
                        _ => None,
                    }
                } else { None };
                // Rule 5: RcInc for closure captures
                let capture_incs: Vec<VarId> = if let IrExprKind::ClosureCreate { captures, .. } = &expr.kind {
                    captures.iter().filter(|(_, ty)| is_heap_type(ty)).map(|(v, _)| *v).collect()
                } else { vec![] };

                let mut node = FnBody::VDecl { var, ty, mutability, expr, body: Box::new(result) };
                // Wrap with Inc nodes
                if let Some(id) = inc_var {
                    node = FnBody::Inc { var: id, body: Box::new(node) };
                }
                for cap in capture_incs.into_iter().rev() {
                    node = FnBody::Inc { var: cap, body: Box::new(node) };
                }
                node
            }
            ChainHead::Assign { var, mut expr } => {
                perceus_expr(&mut expr, var_table);
                // Mutable assign: do NOT Dec old value here.
                // The WASM emitter handles mutable vars with local.set — the old
                // pointer is overwritten but NOT freed mid-scope. The scope-exit
                // Dec handles the final value. Intermediate old values leak by
                // design in the current model (same as Koka's approach for var).
                // TODO: proper old-value recovery requires COW or arena allocation.
                FnBody::Assign { var, expr, body: Box::new(result) }
            }
            ChainHead::Expr { mut expr } => {
                perceus_expr(&mut expr, var_table);
                FnBody::Expr { expr, body: Box::new(result) }
            }
            ChainHead::Stmt { stmt } => FnBody::Stmt { stmt, body: Box::new(result) },
            ChainHead::Inc { var } => FnBody::Inc { var, body: Box::new(result) },
            ChainHead::Dec { var } => FnBody::Dec { var, body: Box::new(result) },
        };
    }
    result
}

/// Collect all heap VDecl vars from the chain, insert Dec before Ret.
fn insert_ret_decs(fb: FnBody, var_table: &mut VarTable) -> FnBody {
    let mut heap_vars: Vec<VarId> = Vec::new();
    collect_heap_vdecls(&fb, &mut heap_vars);
    // Find which vars are used in the Ret expression (don't dec those)
    let ret_vars = collect_ret_vars(&fb);
    insert_decs_before_ret(fb, &heap_vars, &ret_vars, var_table)
}

fn collect_heap_vdecls(fb: &FnBody, vars: &mut Vec<VarId>) {
    // Iterative chain walk — O(1) native stack for an N-statement body.
    let mut cur = fb;
    loop {
        match cur {
            FnBody::VDecl { var, ty, body, .. } => {
                if is_heap_type(ty) { vars.push(*var); }
                cur = body;
            }
            FnBody::Assign { body, .. } | FnBody::Inc { body, .. }
            | FnBody::Dec { body, .. } | FnBody::Expr { body, .. }
            | FnBody::Stmt { body, .. } => cur = body,
            FnBody::Ret { .. } | FnBody::Nop => return,
        }
    }
}

fn collect_ret_vars(fb: &FnBody) -> HashSet<VarId> {
    // The result depends only on the terminal `Ret` expr; walk to it iteratively.
    let mut cur = fb;
    loop {
        match cur {
            FnBody::Ret { expr } => {
                let mut vars = HashSet::new();
                collect_var_refs_expr(expr, &mut vars);
                return vars;
            }
            FnBody::VDecl { body, .. } | FnBody::Assign { body, .. }
            | FnBody::Inc { body, .. } | FnBody::Dec { body, .. }
            | FnBody::Expr { body, .. } | FnBody::Stmt { body, .. } => cur = body,
            FnBody::Nop => return HashSet::new(),
        }
    }
}

fn insert_decs_before_ret(fb: FnBody, heap_vars: &[VarId], ret_vars: &HashSet<VarId>, var_table: &mut VarTable) -> FnBody {
    // Iterative over the chain (see [`ChainHead`]): the non-terminal nodes are
    // pass-throughs (the former recursion only rebuilt them around the processed
    // remainder), and the Dec-insertion logic runs at the terminal (`Ret`/`Nop`).
    let (heads, terminal) = split_chain(fb);
    let mut result = match terminal {
        FnBody::Ret { expr } => {
            // Variables used inside the return expression (but not AS the return value)
            // need tail lift: let __ret = expr; Dec(vars); Ret(__ret)
            let vars_to_dec: Vec<VarId> = heap_vars.iter()
                .filter(|v| !ret_vars.contains(v) || {
                    // If var is in ret_vars AND the ret is NOT just Var(v), it's used inside
                    !matches!(&expr.kind, IrExprKind::Var { id } if *id == **v)
                })
                .filter(|v| {
                    let info = var_table.get(**v);
                    let name = info.name.as_str();
                    // Skip TCO/branch/perceus temporaries (their own RC management)
                    !name.starts_with("__tco_") && !name.starts_with("__br_")
                    && !name.starts_with("__perceus_old")
                })
                .copied()
                .collect();

            if vars_to_dec.is_empty() {
                FnBody::Ret { expr }
            } else {
                // Check if any var_to_dec is used inside the ret expr
                let needs_lift = vars_to_dec.iter().any(|v| ret_vars.contains(v));
                if needs_lift && !matches!(&expr.kind, IrExprKind::Var { .. }) {
                    // Tail lift: let __ret = expr; Dec(vars); Ret(__ret)
                    let ret_ty = expr.ty.clone();
                    let ret_var = var_table.alloc(
                        almide_base::intern::sym("__perceus_ret"),
                        ret_ty.clone(),
                        Mutability::Let,
                        None,
                    );
                    let mut res = FnBody::Ret {
                        expr: IrExpr { kind: IrExprKind::Var { id: ret_var }, ty: ret_ty.clone(), span: None, def_id: None }
                    };
                    for var in vars_to_dec.iter().rev() {
                        res = FnBody::Dec { var: *var, body: Box::new(res) };
                    }
                    FnBody::VDecl { var: ret_var, ty: ret_ty, mutability: Mutability::Let, expr, body: Box::new(res) }
                } else {
                    // No lift needed — just insert Decs before Ret
                    let mut res = FnBody::Ret { expr };
                    for var in vars_to_dec.iter().rev() {
                        res = FnBody::Dec { var: *var, body: Box::new(res) };
                    }
                    res
                }
            }
        }
        FnBody::Nop => {
            // While/for body: insert Dec for heap vars bound in this body.
            let mut res = FnBody::Nop;
            for var in heap_vars.iter().rev() {
                let info = var_table.get(*var);
                let name = info.name.as_str();
                if !name.starts_with("__tco_") && !name.starts_with("__br_")
                    && !name.starts_with("__perceus_old") {
                    res = FnBody::Dec { var: *var, body: Box::new(res) };
                }
            }
            res
        }
        // split_chain only ever yields Ret or Nop as the terminal.
        other => other,
    };
    for head in heads.into_iter().rev() {
        result = match head {
            ChainHead::VDecl { var, ty, mutability, expr } =>
                FnBody::VDecl { var, ty, mutability, expr, body: Box::new(result) },
            ChainHead::Assign { var, expr } =>
                FnBody::Assign { var, expr, body: Box::new(result) },
            ChainHead::Inc { var } => FnBody::Inc { var, body: Box::new(result) },
            ChainHead::Dec { var } => FnBody::Dec { var, body: Box::new(result) },
            ChainHead::Expr { expr } => FnBody::Expr { expr, body: Box::new(result) },
            ChainHead::Stmt { stmt } => FnBody::Stmt { stmt, body: Box::new(result) },
        };
    }
    result
}

#[derive(Debug)]
pub struct PerceusPass;

impl NanoPass for PerceusPass {
    fn name(&self) -> &str { "Perceus" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        // Need split borrow: functions mutably, var_table mutably
        let var_table = &mut program.var_table;
        let functions = &mut program.functions;
        for func in functions.iter_mut() {
            if insert_rc_ops(func, var_table) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if insert_rc_ops(func, &mut program.var_table) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

/// Perceus RC elimination: remove redundant Inc/Dec pairs.
///
/// Theorem (Inc-Dec Cancellation):
///   If RcInc(x) was inserted for Bind(b, Var(x)) and b is immutable
///   with use_count ≤ 1, then RcInc(x) and RcDec(b) cancel.
///
/// Proof: RcInc adds +1 to RC(x). RcDec(b) subtracts -1 from RC(x)
///   (b aliases x). Since b is single-use and immutable, no other
///   reference changes occur during b's lifetime. The pair is identity. □
#[derive(Debug)]
pub struct PerceusOptPass;

impl NanoPass for PerceusOptPass {
    fn name(&self) -> &str { "PerceusOpt" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            if eliminate_redundant_rc(func, &program.var_table) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if eliminate_redundant_rc(func, &program.var_table) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

/// Eliminate redundant RcInc/RcDec pairs in a function (all nested blocks).
fn eliminate_redundant_rc(func: &mut IrFunction, var_table: &VarTable) -> bool {
    eliminate_in_expr(&mut func.body, var_table)
}

fn eliminate_in_expr(expr: &mut IrExpr, var_table: &VarTable) -> bool {
    let mut v = EliminateRc { var_table, changed: false };
    v.visit_expr_mut(expr);
    v.changed
}

/// Removes redundant RcInc/RcDec pairs from every block, riding the exhaustive
/// IrMutVisitor walk so nested blocks inside any node kind (loop bodies, call
/// args, …) are scanned too. Eliminating a redundant pair is always valid, so the
/// now-total recursion can only find MORE pairs.
struct EliminateRc<'a> {
    var_table: &'a VarTable,
    changed: bool,
}

impl IrMutVisitor for EliminateRc<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        if let IrExprKind::Block { stmts, .. } = &mut expr.kind {
            if eliminate_in_block(stmts, self.var_table) { self.changed = true; }
        }
        walk_expr_mut(self, expr); // recurse all children (incl. previously-dropped kinds)
    }
}

/// Scan a block for eliminable Inc/Dec pairs.
///
/// Pattern: RcInc(x) immediately before Bind(b, Var(x))
///          + RcDec(b) later in the block
///   where b is immutable and use_count(b) ≤ 1
///
/// → Remove both RcInc(x) and RcDec(b).
fn eliminate_in_block(stmts: &mut Vec<IrStmt>, var_table: &VarTable) -> bool {
    let mut to_remove: HashSet<usize> = HashSet::new();

    // Pass 1: find RcInc(x) + Bind(b, Var(x)) pairs where b is single-use immutable
    let mut inc_targets: HashMap<usize, (VarId, VarId)> = HashMap::new(); // stmt_idx → (x, b)
    let mut i = 0;
    while i + 1 < stmts.len() {
        if let IrStmtKind::RcInc { var: x } = &stmts[i].kind {
            if let IrStmtKind::Bind { var: b, value, .. } = &stmts[i + 1].kind {
                let is_alias = match &value.kind {
                    IrExprKind::Var { id } => *id == *x,
                    IrExprKind::Clone { expr } => matches!(&expr.kind, IrExprKind::Var { id } if *id == *x),
                    IrExprKind::Deref { expr } => matches!(&expr.kind, IrExprKind::Var { id } if *id == *x),
                    _ => false,
                };
                if is_alias {
                    let info = var_table.get(*b);
                    let is_immutable = !matches!(info.mutability, Mutability::Var);
                    if is_immutable {
                        inc_targets.insert(i, (*x, *b));
                    }
                }
            }
        }
        i += 1;
    }

    // Pass 2: compute last-use index for each variable in this block
    let mut last_use_idx: HashMap<VarId, usize> = HashMap::new();
    for (j, stmt) in stmts.iter().enumerate() {
        let mut refs = HashSet::new();
        collect_var_refs_stmt(stmt, &mut refs);
        for var in refs {
            last_use_idx.insert(var, j);
        }
    }

    // Pass 3: for each eliminable pair, verify lifetime and find RcDec(b)
    for (&inc_idx, &(x, b)) in &inc_targets {
        // Lifetime check: last_use(x) >= last_use(b)
        // If b outlives x, we can't eliminate (x would be freed while b is live)
        let x_last = last_use_idx.get(&x).copied().unwrap_or(0);
        let b_last = last_use_idx.get(&b).copied().unwrap_or(0);
        if x_last < b_last { continue; } // b outlives x → unsafe to eliminate

        for (j, stmt) in stmts.iter().enumerate() {
            if j <= inc_idx { continue; }
            if let IrStmtKind::RcDec { var } = &stmt.kind {
                if *var == b {
                    to_remove.insert(inc_idx);  // remove RcInc(x)
                    to_remove.insert(j);         // remove RcDec(b)
                    break;
                }
            }
        }
    }

    if to_remove.is_empty() { return false; }

    // Apply removals (reverse order to preserve indices)
    let mut indices: Vec<usize> = to_remove.into_iter().collect();
    indices.sort_unstable_by(|a, b| b.cmp(a));
    for idx in indices {
        stmts.remove(idx);
    }

    true
}

/// Perceus verification: check that RC operations are correctly balanced.
///
/// Invariant: For each heap-typed variable v,
///   inc_count(v) + 1 (initial alloc) ≥ dec_count(v)
///   AND dec_count(v) ≥ 1 (every alloc has at least one free path)
///
/// Violations are reported as compiler warnings (not errors — the program
/// still runs, it just may leak or double-free).
#[derive(Debug)]
pub struct PerceusVerifyPass;

impl NanoPass for PerceusVerifyPass {
    fn name(&self) -> &str { "PerceusVerify" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, program: IrProgram, _target: Target) -> PassResult {
        for func in &program.functions {
            if func.is_test { continue; }
            // Lean-certified verify: uses perceus_verified::is_heap_type,
            // count_decs, count_incs (mirroring Lean 4 proven definitions)
            verify_function(func, &program.var_table);
        }
        PassResult { program, changed: false }
    }
}

/// Lean-certified verification. THE ACTUAL VERIFY uses
/// perceus_verified::verify_expr (mirrors Lean 4 proofs).
fn verify_function(func: &IrFunction, var_table: &VarTable) {
    perceus_verify_function(func, var_table);
}

/// Run Lean 4-certified Perceus RC verification on a single function.
/// Returns the number of violations found.
pub fn perceus_verify_function(func: &IrFunction, var_table: &VarTable) -> usize {
    let mut returned_vars: HashSet<VarId> = HashSet::new();
    collect_all_tail_vars(&func.body, &mut returned_vars);
    let mut moved_out_vars: HashSet<VarId> = HashSet::new();
    collect_moved_out_vars(&func.body, &mut moved_out_vars);
    let mut env_load_vars_set: HashSet<VarId> = HashSet::new();
    scan_env_loads(&func.body, &mut env_load_vars_set);

    let issues = super::perceus_verified::verify_expr(
        &func.body, var_table, &returned_vars, &moved_out_vars, &env_load_vars_set,
    );
    for (var, msg) in &issues {
        let name = var_table.get(*var).name.as_str();
        eprintln!("[perceus-belt] {}: `{}` (VarId {}) in `{}`",
            msg, name, var.0, func.name.as_str());
    }

    verify_branch_balance(&func.body, &HashSet::new(), &env_load_vars_set, var_table, func.name.as_str());
    issues.len()
}

fn scan_env_loads(expr: &IrExpr, vars: &mut HashSet<VarId>) {
    if let IrExprKind::Block { stmts, expr: tail } = &expr.kind {
        for stmt in stmts {
            if let IrStmtKind::Bind { var, value, ty, .. } = &stmt.kind {
                if is_heap_type(ty) && matches!(&value.kind, IrExprKind::EnvLoad { .. }) {
                    vars.insert(*var);
                }
            }
        }
        if let Some(t) = tail { scan_env_loads(t, vars); }
    }
    if let IrExprKind::If { then, else_, .. } = &expr.kind {
        scan_env_loads(then, vars); scan_env_loads(else_, vars);
    }
    if let IrExprKind::Match { arms, .. } = &expr.kind {
        for arm in arms { scan_env_loads(&arm.body, vars); }
    }
}

fn is_heap_type(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. } | Ty::Unknown | Ty::Fn { .. })
}

/// Insert RC operations into a function body using FnBody conversion.
/// Block IR → FnBody chain → Perceus rules → FnBody → Block IR.
fn insert_rc_ops(func: &mut IrFunction, var_table: &mut VarTable) -> bool {
    if func.is_test { return false; }

    // Flatten TCO's nested `…continue`/`…break` tail-blocks so heap-local Decs
    // land as same-level statements before the exit (reachable AND verifier-
    // visible). See `flatten_exit_tail_blocks`. Must precede Dec insertion.
    flatten_exit_tail_blocks(&mut func.body);

    // Apply Perceus recursively to the entire function body
    perceus_expr(&mut func.body, var_table);
    // Note: function parameters use borrow semantics — the CALLER owns the
    // value and Dec's it at scope exit. The callee does NOT Dec parameters.
    // This avoids double-free (caller Dec + callee Dec on same pointer).

    true
}

/// Collect every var that is moved out of its defining block as a bare-`Var`
/// block tail. The block's value *is* that var, so ownership transfers to
/// whatever consumes the block (an enclosing Bind, a return, a call arg); that
/// consumer carries the Dec. Such a var therefore must not be required to have a
/// Dec inside its own block — without this set the leak rule would false-positive
/// on every ANF-lifted tail temporary (e.g. `__perceus_ret`, `__anf_*`).
///
/// Exhaustive `IrVisitor` walk — total by construction, so a newly added node
/// kind that nests blocks cannot silently drop a moved-out tail (unlike the
/// hand-rolled, tail-context `collect_all_tail_vars`, which deliberately does not
/// descend into discarded statement positions).
fn collect_moved_out_vars(expr: &IrExpr, vars: &mut HashSet<VarId>) {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct MovedOutCollector<'a> { vars: &'a mut HashSet<VarId> }
    impl IrVisitor for MovedOutCollector<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Block { expr: Some(tail), .. } = &expr.kind {
                if let IrExprKind::Var { id } = &tail.kind {
                    self.vars.insert(*id);
                }
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            almide_ir::visit::walk_stmt(self, stmt);
        }
    }
    MovedOutCollector { vars }.visit_expr(expr);
}

/// Control-flow-aware verification: check branches independently.
/// For each if/else or match, verify that heap vars bound in an outer scope
/// are not leaked on any single branch.
fn verify_branch_balance(
    expr: &IrExpr,
    outer_heap_vars: &HashSet<VarId>,
    env_load_vars: &HashSet<VarId>,
    var_table: &VarTable,
    fn_name: &str,
) {
    BranchBalance { heap_vars: outer_heap_vars.clone(), env_load_vars, var_table, fn_name }
        .visit_expr(expr);
}

/// Diagnostic (post-insertion, no Rc decision): reports a heap var Dec'd on one
/// branch but not another. Rides the exhaustive IrVisitor walk — the special
/// Block (extend heap_vars) and If/Match (branch-consistency) arms are kept; the
/// default delegates to walk_expr so no node kind drops its subtree.
struct BranchBalance<'a> {
    heap_vars: HashSet<VarId>,
    env_load_vars: &'a HashSet<VarId>,
    var_table: &'a VarTable,
    fn_name: &'a str,
}

impl IrVisitor for BranchBalance<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Block { stmts, .. } => {
                let saved = self.heap_vars.clone();
                for stmt in stmts {
                    if let IrStmtKind::Bind { var, ty, .. } = &stmt.kind {
                        if is_heap_type(ty) && !self.env_load_vars.contains(var) {
                            self.heap_vars.insert(*var);
                        }
                    }
                }
                walk_expr(self, expr); // recurse stmts + tail with the extended heap_vars
                self.heap_vars = saved;
            }
            IrExprKind::If { cond, then, else_ } => {
                self.visit_expr(cond);
                // Both branches should Dec each outer heap var referenced in BOTH.
                let then_decs = collect_decs_in_expr(then);
                let else_decs = collect_decs_in_expr(else_);
                let mut then_refs = HashSet::new();
                let mut else_refs = HashSet::new();
                collect_var_refs_expr(then, &mut then_refs);
                collect_var_refs_expr(else_, &mut else_refs);
                for var in &self.heap_vars {
                    let in_both = then_refs.contains(var) && else_refs.contains(var);
                    if !in_both { continue; }
                    let in_then = then_decs.contains(var);
                    let in_else = else_decs.contains(var);
                    if in_then != in_else {
                        let name = self.var_table.get(*var).name.as_str();
                        eprintln!(
                            "[perceus-belt] BRANCH-LEAK: `{}` (VarId {}) in `{}` — Dec'd in {} but not {}",
                            name, var.0, self.fn_name,
                            if in_then { "then" } else { "else" },
                            if in_then { "else" } else { "then" },
                        );
                    }
                }
                self.visit_expr(then);
                self.visit_expr(else_);
            }
            IrExprKind::Match { subject, arms } => {
                self.visit_expr(subject);
                if arms.len() > 1 {
                    let arm_decs: Vec<HashSet<VarId>> = arms.iter()
                        .map(|arm| collect_decs_in_expr(&arm.body))
                        .collect();
                    let arm_refs: Vec<HashSet<VarId>> = arms.iter()
                        .map(|arm| { let mut r = HashSet::new(); collect_var_refs_expr(&arm.body, &mut r); r })
                        .collect();
                    for var in &self.heap_vars {
                        let ref_count = arm_refs.iter().filter(|r| r.contains(var)).count();
                        if ref_count < 2 { continue; }
                        let first_has = arm_decs[0].contains(var);
                        for (i, decs) in arm_decs.iter().enumerate().skip(1) {
                            if decs.contains(var) != first_has {
                                let name = self.var_table.get(*var).name.as_str();
                                eprintln!(
                                    "[perceus-belt] BRANCH-LEAK: `{}` (VarId {}) in `{}` — Dec'd in arm 0 but not arm {}",
                                    name, var.0, self.fn_name, i,
                                );
                            }
                        }
                    }
                }
                for arm in arms { self.visit_expr(&arm.body); }
            }
            _ => walk_expr(self, expr),
        }
    }
}

/// Collect all VarIds that are Dec'd within an expression.
fn collect_decs_in_expr(expr: &IrExpr) -> HashSet<VarId> {
    let mut decs = HashSet::new();
    scan_decs(expr, &mut decs);
    decs
}

fn scan_decs(expr: &IrExpr, decs: &mut HashSet<VarId>) {
    DecCollector { decs }.visit_expr(expr);
}

/// Collects every RcDec'd var (diagnostic input to verify_branch_balance), riding
/// the exhaustive IrVisitor walk so no node kind drops its subtree.
struct DecCollector<'a> {
    decs: &'a mut HashSet<VarId>,
}

impl IrVisitor for DecCollector<'_> {
    fn visit_stmt(&mut self, stmt: &IrStmt) {
        if let IrStmtKind::RcDec { var } = &stmt.kind { self.decs.insert(*var); }
        walk_stmt(self, stmt);
    }
}

/// Recursively collect all variables used in tail expressions at any nesting level.
fn collect_all_tail_vars(expr: &IrExpr, vars: &mut HashSet<VarId>) {
    // Tail-CONTEXT analysis: only positions that flow to a return are walked. A
    // plain exhaustive walk would over-collect (treat consumed Call args etc. as
    // returned) and so suppress leak diagnostics, so the non-tail node kinds are
    // listed as explicit no-ops, not recursed — total by construction, semantics
    // unchanged.
    match &expr.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            // Variables in this block's tail
            collect_var_refs_expr(tail, vars);
            // Recurse into statements' values
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } => collect_all_tail_vars(value, vars),
                    IrStmtKind::Assign { value, .. } => collect_all_tail_vars(value, vars),
                    IrStmtKind::Expr { expr } => collect_all_tail_vars(expr, vars),
                    IrStmtKind::BindDestructure { .. } | IrStmtKind::FieldAssign { .. }
                    | IrStmtKind::Guard { .. } | IrStmtKind::IndexAssign { .. }
                    | IrStmtKind::MapInsert { .. } | IrStmtKind::ListSwap { .. }
                    | IrStmtKind::ListReverse { .. } | IrStmtKind::ListRotateLeft { .. }
                    | IrStmtKind::ListCopySlice { .. } | IrStmtKind::RcInc { .. }
                    | IrStmtKind::RcDec { .. } | IrStmtKind::Comment { .. } => {}
                }
            }
            collect_all_tail_vars(tail, vars);
        }
        IrExprKind::Block { stmts, expr: None } => {
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } => collect_all_tail_vars(value, vars),
                    IrStmtKind::Assign { .. } | IrStmtKind::Expr { .. }
                    | IrStmtKind::BindDestructure { .. } | IrStmtKind::FieldAssign { .. }
                    | IrStmtKind::Guard { .. } | IrStmtKind::IndexAssign { .. }
                    | IrStmtKind::MapInsert { .. } | IrStmtKind::ListSwap { .. }
                    | IrStmtKind::ListReverse { .. } | IrStmtKind::ListRotateLeft { .. }
                    | IrStmtKind::ListCopySlice { .. } | IrStmtKind::RcInc { .. }
                    | IrStmtKind::RcDec { .. } | IrStmtKind::Comment { .. } => {}
                }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_all_tail_vars(cond, vars);
            collect_all_tail_vars(then, vars);
            collect_all_tail_vars(else_, vars);
        }
        IrExprKind::Match { subject, arms } => {
            collect_all_tail_vars(subject, vars);
            for arm in arms { collect_all_tail_vars(&arm.body, vars); }
        }
        IrExprKind::Lambda { body, .. } => collect_all_tail_vars(body, vars),
        // No return/tail position in any other node kind — listed so a new kind
        // forces a tail-or-not decision instead of silently joining this no-op set.
        IrExprKind::Await { .. } | IrExprKind::BinOp { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::Break | IrExprKind::Call { .. }
        | IrExprKind::Clone { .. } | IrExprKind::ClosureCreate { .. } | IrExprKind::Continue
        | IrExprKind::Deref { .. } | IrExprKind::EmptyMap | IrExprKind::EnvLoad { .. }
        | IrExprKind::Fan { .. } | IrExprKind::FnRef { .. } | IrExprKind::ForIn { .. }
        | IrExprKind::Hole | IrExprKind::IndexAccess { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::IterChain { .. } | IrExprKind::List { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::LitFloat { .. } | IrExprKind::LitInt { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::MapAccess { .. } | IrExprKind::MapLiteral { .. } | IrExprKind::Member { .. }
        | IrExprKind::OptionNone | IrExprKind::OptionSome { .. } | IrExprKind::OptionalChain { .. }
        | IrExprKind::Range { .. } | IrExprKind::RcWrap { .. } | IrExprKind::Record { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::ResultErr { .. } | IrExprKind::ResultOk { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::RustMacro { .. } | IrExprKind::SpreadRecord { .. }
        | IrExprKind::StringInterp { .. } | IrExprKind::TailCall { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::Todo { .. } | IrExprKind::Try { .. }
        | IrExprKind::Tuple { .. } | IrExprKind::TupleIndex { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::Unit | IrExprKind::Unwrap { .. } | IrExprKind::UnwrapOr { .. }
        | IrExprKind::Var { .. } | IrExprKind::While { .. } => {}
    }
}

fn collect_var_refs_expr(expr: &IrExpr, refs: &mut HashSet<VarId>) {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct VarCollector<'a> { refs: &'a mut HashSet<VarId> }
    impl IrVisitor for VarCollector<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Var { id } = &expr.kind { self.refs.insert(*id); }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            almide_ir::visit::walk_stmt(self, stmt);
        }
    }
    VarCollector { refs }.visit_expr(expr);
}

fn collect_var_refs_stmt(stmt: &IrStmt, refs: &mut HashSet<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => collect_var_refs_expr(value, refs),
        IrStmtKind::Assign { var, value } => { refs.insert(*var); collect_var_refs_expr(value, refs); }
        IrStmtKind::Expr { expr } => collect_var_refs_expr(expr, refs),
        IrStmtKind::Guard { cond, else_ } => { collect_var_refs_expr(cond, refs); collect_var_refs_expr(else_, refs); }
        IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => { refs.insert(*var); }
        // Explicit no-op, NOT a recurse: this ref set drives last-use → RcDec
        // placement, so its semantics must not change. Listing every remaining
        // kind makes a new IrStmtKind a compile error here, never a silent drop.
        IrStmtKind::BindDestructure { .. }
        | IrStmtKind::IndexAssign { .. } | IrStmtKind::MapInsert { .. }
        | IrStmtKind::FieldAssign { .. } | IrStmtKind::ListSwap { .. }
        | IrStmtKind::ListReverse { .. } | IrStmtKind::ListRotateLeft { .. }
        | IrStmtKind::ListCopySlice { .. } | IrStmtKind::Comment { .. } => {}
    }
}
