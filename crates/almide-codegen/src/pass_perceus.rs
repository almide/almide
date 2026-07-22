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
                // Rule 1 (unified): a heap local bound to a BORROWED ALIAS must
                // acquire its own reference, or its scope-end Dec under-counts and
                // double-frees the value the alias points into. Inc the BOUND var
                // AFTER the bind: it is then loop-body-local (balances the per-
                // iteration scope-end Dec) and works for aliases produced through
                // match/if/block tails, where no pre-existing source var exists to
                // Inc beforehand. `yields_borrowed_alias` subsumes the former
                // Var/Clone/Deref allow-list — Inc-after on the bound var is
                // equivalent to Inc-before on the source they alias.
                let alias_inc = is_heap_type(&ty) && yields_borrowed_alias(&expr);
                // Rule 5: RcInc for closure captures (captured vars exist BEFORE
                // the bind, so these Incs wrap around the VDecl).
                let capture_incs: Vec<VarId> = if let IrExprKind::ClosureCreate { captures, .. } = &expr.kind {
                    captures.iter().filter(|(_, ty)| is_heap_type(ty)).map(|(v, _)| *v).collect()
                } else { vec![] };

                // `let var = expr; rc_inc(var); <rest>` — the Inc lives in the
                // VDecl body so it stays at the bind's chain level (verifier-
                // counted) and runs once per loop iteration for in-loop binds.
                //
                // ORDERING HAZARD: when `expr` is a BLOCK, its trailing temp
                // Decs run while the block evaluates — BEFORE an after-VDecl
                // Inc. If the tail aliases a temp's interior (unwrap_or of a
                // parse temp), the temp's typed dec frees the payload first
                // and the late Inc RESURRECTS a freed block (json_gltf trap).
                // For a Block whose tail is a Var bound inside it, hoist the
                // Inc INTO the block, right after that bind — before any Dec.
                let mut expr = expr;
                let mut inner_inc_done = false;
                if alias_inc {
                    if let IrExprKind::Block { stmts, expr: Some(tail) } = &mut expr.kind {
                        if let IrExprKind::Var { id: tail_id } = &tail.kind {
                            let tail_id = *tail_id;
                            let bind_pos = stmts.iter().rposition(|st| matches!(
                                &st.kind, IrStmtKind::Bind { var: bv, .. } if *bv == tail_id
                            ));
                            if let Some(_pos) = bind_pos {
                                // The inner block was ALREADY processed by
                                // perceus_expr: its own VDecl arm applied
                                // Rule 1 to the tail bind (bind-adjacent,
                                // before any trailing temp Dec — satisfying
                                // the json_gltf ordering hazard this hoist
                                // was built for). Inserting a second Inc here
                                // DOUBLE-applied the rule: +1 leak per
                                // execution (verified — two rc_incs on the
                                // tail temp). All this arm must do is
                                // suppress the LATE outer Inc.
                                inner_inc_done = true;
                            }
                        }
                    }
                }
                let body = if alias_inc && !inner_inc_done {
                    FnBody::Inc { var, body: Box::new(result) }
                } else {
                    result
                };
                let mut node = FnBody::VDecl { var, ty, mutability, expr, body: Box::new(body) };
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
                //
                // Rule 1 applies to ASSIGN exactly as to VDecl: the var keeps
                // its scope-end Dec, so an ALIAS-shaped RHS must acquire its
                // own reference or that Dec under-counts the aliased value —
                // `var c = fresh; c = list.get_or(xs, 0, d)` double-freed the
                // shared element (rc==0 sentinel trap, verified repro). The
                // VDecl arm had this rule from the start; this arm was the
                // bypass.
                //
                // EXCEPTION — MOVE, not share: a bare-Var RHS whose source is
                // a scope-Dec-EXEMPT temp (`__tco_*`, `__br_*`,
                // `__perceus_*`: the same name classes the Ret/Nop dec
                // insertion skips) DONATES its reference — those temps never
                // get their own Dec, so the assign transfers ownership and an
                // Inc here double-counts (+1 per TCO loop iteration,
                // measured: deep churn 7 MB → 55 MB).
                let ty = var_table.get(var).ty.clone();
                let moved_from_exempt_temp = matches!(&expr.kind,
                    IrExprKind::Var { id } if {
                        let n = var_table.get(*id).name;
                        let n = n.as_str();
                        n.starts_with("__tco_") || n.starts_with("__br_")
                            || n.starts_with("__perceus_")
                    });
                let alias_inc = is_heap_type(&ty)
                    && yields_borrowed_alias(&expr)
                    && !moved_from_exempt_temp;
                let body = if alias_inc {
                    FnBody::Inc { var, body: Box::new(result) }
                } else {
                    result
                };
                FnBody::Assign { var, expr, body: Box::new(body) }
            }
            ChainHead::Expr { mut expr } => {
                perceus_expr(&mut expr, var_table);
                FnBody::Expr { expr, body: Box::new(result) }
            }
            ChainHead::Stmt { mut stmt } => {
                // A `Guard { cond, else_ }` statement reaches perceus NOWHERE
                // else: `block_to_fnbody` funnels every stmt kind that isn't
                // Bind/Assign/RcInc/RcDec/Expr into this pass-through arm, so
                // without recursing here its `cond` and its divergent `else_`
                // block get ZERO Rc processing — every heap temp inside the
                // else block leaks (repro: `guard c else { io.print("v${x}"); ok(()) }`,
                // the interpolation temp is never Dec'd). `perceus_expr` on the
                // else Block round-trips it through the Block arm, which inserts
                // the scope-end Decs before its (divergent) Ret, exactly as for
                // any other block.
                if let IrStmtKind::Guard { cond, else_ } = &mut stmt.kind {
                    perceus_expr(cond, var_table);
                    perceus_expr(else_, var_table);
                }
                FnBody::Stmt { stmt, body: Box::new(result) }
            }
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
            FnBody::VDecl { var, ty, expr, body, .. } => {
                // An `EnvLoad`-bound local BORROWS the closure environment's
                // captured value — the env owns it (Rule-5 Inc'd it at capture).
                // A scope-end Dec here would free a value the env still holds, so
                // the next call or the env teardown double-frees it. Exclude such
                // borrow locals from the scope-end Dec (they own no reference).
                if is_heap_type(ty) && !matches!(expr.kind, IrExprKind::EnvLoad { .. }) {
                    vars.push(*var);
                }
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

/// `FnBody::Ret` terminal case of `insert_decs_before_ret`, extracted
/// verbatim (cog>30 decomposition, pattern 2 — the terminal-node match's
/// only two arms, `Ret`/`Nop`, are each a self-contained "compute one
/// `FnBody` value" case with no state shared between them).
fn insert_decs_ret_terminal(mut expr: IrExpr, heap_vars: &[VarId], ret_vars: &HashSet<VarId>, var_table: &mut VarTable) -> FnBody {
    // F2 (#527): the RET EXPRESSION's interior gets the FULL rule
    // set. Until now the terminal was returned untouched and nothing
    // ever perceus_expr'd it — a tail-position Match/If/Block
    // subtree (the dominant fn shape `fn f() { stmts; match … }`)
    // received ZERO rc processing: no Rule-1 incs, no capture incs,
    // no temp decs. Leak-direction, but it disabled reclamation
    // across the most common code shape and masked the lift's
    // ordering hazard below.
    perceus_expr(&mut expr, var_table);
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
            insert_decs_ret_lift(expr, vars_to_dec, var_table)
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

/// Tail-lift branch of `insert_decs_ret_terminal`, extracted verbatim
/// (further split of the same decomposition): `let __ret = expr; Dec(vars);
/// Ret(__ret)`, used when a var being Dec'd is also referenced inside the
/// return expression (so the plain "Dec-then-Ret" order would free it
/// before use).
fn insert_decs_ret_lift(expr: IrExpr, vars_to_dec: Vec<VarId>, var_table: &mut VarTable) -> FnBody {
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
    // Rule 1 applies HERE too: this hand-built VDecl bypasses
    // the ChainHead::VDecl arm, so a ret expr that ALIASES one
    // of the locals being Dec'd below (e.g. `if filled then
    // base + "…" else base` with `Dec base` following) needs
    // its own reference or the function returns a freed
    // pointer. This under-count was masked for years by the
    // chain-temp over-incs the alias-gated hoist removed
    // (caught by the byte gate as a resurrection trap,
    // default_fields_test).
    //
    // SUPPRESSION (same rule as the VDecl arm's hoist): with
    // F2 processing the expr's interior, a Block whose tail
    // var was bound inside already received the inner
    // Rule-1 Inc — a second one here would double-apply.
    let inner_already_inc = matches!(&expr.kind,
        IrExprKind::Block { stmts, expr: Some(tail) }
            if matches!(&tail.kind, IrExprKind::Var { id }
                if stmts.iter().any(|st| matches!(&st.kind,
                    IrStmtKind::Bind { var: bv, .. } if bv == id))));
    if is_heap_type(&ret_ty) && yields_borrowed_alias(&expr) && !inner_already_inc {
        res = FnBody::Inc { var: ret_var, body: Box::new(res) };
    }
    FnBody::VDecl { var: ret_var, ty: ret_ty, mutability: Mutability::Let, expr, body: Box::new(res) }
}

fn insert_decs_before_ret(fb: FnBody, heap_vars: &[VarId], ret_vars: &HashSet<VarId>, var_table: &mut VarTable) -> FnBody {
    // Iterative over the chain (see [`ChainHead`]): the non-terminal nodes are
    // pass-throughs (the former recursion only rebuilt them around the processed
    // remainder), and the Dec-insertion logic runs at the terminal (`Ret`/`Nop`).
    let (heads, terminal) = split_chain(fb);
    let mut result = match terminal {
        FnBody::Ret { expr } => insert_decs_ret_terminal(expr, heap_vars, ret_vars, var_table),
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

include!("pass_perceus_p2.rs");
include!("pass_perceus_p3.rs");
