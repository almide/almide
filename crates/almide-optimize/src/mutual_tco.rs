//! MUTUAL tail-call elimination on the SHARED IR (#1043) — one rewrite, every
//! consumer: the v0 Rust codegen, the v1 trust-spine native render, the v1
//! wasm render and almide-interp all consume `monomorphize`'s output, so a
//! tail-call SCC collapsed HERE is loop-shaped for all four by construction.
//! (The previous spelling lived in almide-codegen's TailCallOpt pass, which
//! only the v0 leg ran — the DEFAULT native path kept plain calls and its
//! termination kept hanging off LLVM's opt-level, the exact dependence #1043
//! objects to.)
//!
//! Each tail-call SCC of size >= 2 collapses into ONE dispatcher:
//!
//!   fn __mutual_tco_<n>_<first>(tag: Int, <member-0 slots>, <member-1 slots>, …) -> R {
//!     var __mt_running = true
//!     var __mt_result: R = <zero>
//!     while __mt_running {
//!       if tag == 0 { <body-0> } else { <body-1> }
//!     }
//!     __mt_result
//!   }
//!
//! where a member body's tail call `g(args)` becomes `{ temps = args; g's
//! slots = temps; tag = g }` (the arm then simply ends — the loop re-checks
//! and dispatches), and a base-case tail becomes `{ __mt_result = v;
//! __mt_running = false }`. NO `break`/`continue` and no `while true`: the
//! running-flag state machine is exactly the scalar-loop subset the v1 MIR
//! `while` lowering executes (Int/Bool cond, SetLocal-carried scalar state),
//! so the trust spine needs no new op, no new cert letter and no
//! verify_ownership change to run it.
//!
//! Every member becomes a THIN WRAPPER `f(a, b) = __mutual_tco_…(k, …, a, b,
//! …)` passing zero-literals in the other members' slots. The wrappers
//! preserve every external surface — non-tail call sites, function values,
//! exports — so any shape this pass does NOT rewrite (a call under `?`, a
//! call in non-tail position) still calls the wrapper and stays semantically
//! correct; it merely keeps its stack frame. Safe-by-default.
//!
//! SCOPE (v1): ALL-SCALAR groups — every member's params and the
//! (necessarily shared) return type are Int/Float/Bool. Scalars are Copy: no
//! borrow preservation, no per-iteration RcDec, no clone/move census. Heap-
//! typed groups keep today's shape (v0: plain calls at the pinned opt-level;
//! v1 wasm: per-fn `return_call`, C-178).

use std::collections::{HashMap, HashSet};

use almide_ir::{
    substitute, BinOp, CallTarget, IrExpr, IrExprKind, IrFunction, IrMatchArm, IrParam,
    IrProgram, IrStmt, IrStmtKind, IrVisibility, Mutability, ParamBorrow, VarId, VarTable,
};
use almide_lang::types::Ty;

/// Scalar gate: params AND the return type must be one of these.
fn scalar(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Bool)
}

fn candidate(f: &IrFunction) -> bool {
    !f.is_effect
        && !f.is_test
        && f.generics.is_none()
        && f.extern_attrs.is_empty()
        && f.export_attrs.is_empty()
        && f.attrs.is_empty()
        // Module-qualified names (stdlib splices, linked module fns) render
        // through a prefix scheme this pass does not participate in.
        && !f.name.as_str().contains('.')
        && scalar(&f.ret_ty)
        && f.params.iter().all(|p| scalar(&p.ty))
}

/// Collect fn names called in TAIL position (body root; If arms; Match arm
/// bodies; a Block's trailing expr). Everything else is by definition not a
/// tail call and is left for the wrappers to serve.
fn collect_tail_callees(expr: &IrExpr, out: &mut HashSet<almide_base::intern::Sym>) {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, .. } => {
            out.insert(*name);
        }
        IrExprKind::If { then, else_, .. } => {
            collect_tail_callees(then, out);
            collect_tail_callees(else_, out);
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms {
                collect_tail_callees(&arm.body, out);
            }
        }
        IrExprKind::Block { expr: Some(tail), .. } => {
            collect_tail_callees(tail, out);
        }
        // Explicit-preserve (traversal-totality): every remaining variant is
        // BY DEFINITION not a tail position — a call under any of these keeps
        // its stack frame and (after the rewrite) targets the member's
        // wrapper, which is the conservative-correct default for a variant
        // added later.
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
        | IrExprKind::Clone { .. }
        | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
        | IrExprKind::RustMacro { .. } | IrExprKind::ToVec { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
}

/// Tarjan strongly-connected components over the tail-call edges, iterative
/// (an explicit stack — the input is user code, so no recursion budget is
/// assumed). Only size >= 2 groups are rewritten; size-1 self-loops belong to
/// the per-fn TCO the codegen legs already run.
fn sccs(n: usize, edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut t = Tarjan::new(n);
    for root in 0..n {
        if !t.node[root].visited {
            t.walk(root, edges);
        }
    }
    t.out
}

#[derive(Clone, Copy)]
struct NodeState {
    index: u32,
    lowlink: u32,
    on_stack: bool,
    visited: bool,
}

/// Tarjan's running state: per-node marks, the DFS counter, the component
/// stack, and the components closed so far.
struct Tarjan {
    node: Vec<NodeState>,
    counter: u32,
    stack: Vec<usize>,
    out: Vec<Vec<usize>>,
}

impl Tarjan {
    fn new(n: usize) -> Self {
        Tarjan {
            node: vec![NodeState { index: 0, lowlink: 0, on_stack: false, visited: false }; n],
            counter: 0,
            stack: Vec::new(),
            out: Vec::new(),
        }
    }

    /// One DFS tree, driven by an EXPLICIT stack of `(node, edge cursor)` —
    /// the input is user code, so no recursion budget is assumed.
    fn walk(&mut self, root: usize, edges: &[Vec<usize>]) {
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some(&mut (v, ref mut cursor)) = work.last_mut() {
            if *cursor == 0 {
                self.enter(v);
            }
            let Some(&w) = edges[v].get(*cursor) else {
                work.pop();
                self.close(v, work.last().map(|&(parent, _)| parent));
                continue;
            };
            *cursor += 1;
            if !self.node[w].visited {
                work.push((w, 0));
            } else if self.node[w].on_stack {
                self.node[v].lowlink = self.node[v].lowlink.min(self.node[w].index);
            }
        }
    }

    /// First visit of `v`: stamp its index/lowlink and push it on the component stack.
    fn enter(&mut self, v: usize) {
        self.node[v].visited = true;
        self.node[v].index = self.counter;
        self.node[v].lowlink = self.counter;
        self.counter += 1;
        self.node[v].on_stack = true;
        self.stack.push(v);
    }

    /// `v`'s edges are exhausted: propagate its lowlink to its DFS parent, and
    /// if `v` is a component root, pop that component off the stack.
    fn close(&mut self, v: usize, parent: Option<usize>) {
        if let Some(p) = parent {
            self.node[p].lowlink = self.node[p].lowlink.min(self.node[v].lowlink);
        }
        if self.node[v].lowlink != self.node[v].index {
            return;
        }
        let mut scc = Vec::new();
        loop {
            let w = self.stack.pop().expect("tarjan stack underflow");
            self.node[w].on_stack = false;
            scc.push(w);
            if w == v {
                break;
            }
        }
        self.out.push(scc);
    }
}

fn scalar_zero(ty: &Ty) -> IrExpr {
    let kind = match ty {
        Ty::Int => IrExprKind::LitInt { value: 0 },
        Ty::Float => IrExprKind::LitFloat { value: 0.0 },
        Ty::Bool => IrExprKind::LitBool { value: false },
        _ => unreachable!("the candidate gate admits only scalar types"),
    };
    IrExpr { kind, ty: ty.clone(), span: None, def_id: None }
}

fn var_expr(id: VarId, ty: Ty) -> IrExpr {
    IrExpr { kind: IrExprKind::Var { id }, ty, span: None, def_id: None }
}

fn lit_int(v: i64) -> IrExpr {
    IrExpr { kind: IrExprKind::LitInt { value: v }, ty: Ty::Int, span: None, def_id: None }
}

fn lit_bool(v: bool) -> IrExpr {
    IrExpr { kind: IrExprKind::LitBool { value: v }, ty: Ty::Bool, span: None, def_id: None }
}

fn assign(var: VarId, value: IrExpr) -> IrStmt {
    IrStmt { kind: IrStmtKind::Assign { var, value }, span: None }
}

/// Entry point — called from `monomorphize` (both its exits), so every
/// pipeline that monomorphizes gets the rewrite.
pub fn run_mutual_tco(program: &mut IrProgram) {
    let IrProgram { functions, var_table, .. } = program;

    let mut idx_of: HashMap<almide_base::intern::Sym, usize> = HashMap::new();
    for (i, f) in functions.iter().enumerate() {
        if candidate(f) {
            idx_of.insert(f.name, i);
        }
    }
    if idx_of.len() < 2 {
        return;
    }

    let members: Vec<usize> = {
        let mut v: Vec<usize> = idx_of.values().copied().collect();
        v.sort_unstable();
        v
    };
    let dense_of: HashMap<usize, usize> =
        members.iter().enumerate().map(|(d, &i)| (i, d)).collect();
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); members.len()];
    for (&fi, &d) in &dense_of {
        let mut callees = HashSet::new();
        collect_tail_callees(&functions[fi].body, &mut callees);
        for c in callees {
            if let Some(&ci) = idx_of.get(&c) {
                edges[d].push(dense_of[&ci]);
            }
        }
        edges[d].sort_unstable();
    }

    let mut scc_no = 0usize;
    for scc in sccs(members.len(), &edges) {
        if scc.len() < 2 {
            continue;
        }
        let fis: Vec<usize> = scc.iter().map(|&d| members[d]).collect();
        // A tail call returns the callee's result unchanged, so intra-SCC ret
        // types agree by construction — but verify rather than trust.
        let ret_ty = functions[fis[0]].ret_ty.clone();
        if !fis.iter().all(|&i| functions[i].ret_ty == ret_ty) {
            continue;
        }
        rewrite_scc(functions, var_table, &fis, ret_ty, scc_no);
        scc_no += 1;
    }
}

/// The dispatcher's control vars plus the per-member parameter slots and jump
/// temps — everything the SCC rewrite threads through its phases.
struct SccFrame {
    /// Which member's arm runs on the next loop turn.
    tag: VarId,
    /// The `while` guard; a base case clears it.
    running: VarId,
    /// The value the dispatcher finally returns.
    result: VarId,
    /// Member k's dispatcher-param slots, reassigned on every jump.
    slots: Vec<Vec<(VarId, Ty)>>,
    /// Member k's jump temps, bound before the slot assigns so an arg that
    /// reads several current slots sees their pre-jump values — simultaneous
    /// assignment.
    temps: Vec<Vec<(VarId, Ty)>>,
}

impl SccFrame {
    /// Allocate every var the rewrite needs. Slot/temp names carry the MEMBER
    /// INDEX: `is_even(n)` and `is_odd(n)` must NOT both render a param `n`
    /// (the E0415 duplicate-binder class).
    fn alloc(
        var_table: &mut VarTable,
        functions: &[IrFunction],
        fis: &[usize],
        ret_ty: &Ty,
        scc_no: usize,
    ) -> Self {
        let mut var = |name: String, ty: Ty, m: Mutability| {
            var_table.alloc(almide_base::intern::sym(&name), ty, m, None)
        };
        let tag = var(format!("__mt_tag_{scc_no}"), Ty::Int, Mutability::Var);
        let running = var(format!("__mt_running_{scc_no}"), Ty::Bool, Mutability::Var);
        let result = var(format!("__mt_result_{scc_no}"), ret_ty.clone(), Mutability::Var);
        let mut slots: Vec<Vec<(VarId, Ty)>> = Vec::new();
        let mut temps: Vec<Vec<(VarId, Ty)>> = Vec::new();
        for (k, &fi) in fis.iter().enumerate() {
            let mut srow = Vec::new();
            let mut trow = Vec::new();
            for p in &functions[fi].params {
                let pname = var_table.get(p.var).name;
                let s = var_table.alloc(
                    almide_base::intern::sym(&format!("__mt{scc_no}_{k}_{}", pname.as_str())),
                    p.ty.clone(),
                    Mutability::Var,
                    None,
                );
                let t = var_table.alloc(
                    almide_base::intern::sym(&format!("__mt{scc_no}_tmp_{k}_{}", pname.as_str())),
                    p.ty.clone(),
                    Mutability::Let,
                    None,
                );
                srow.push((s, p.ty.clone()));
                trow.push((t, p.ty.clone()));
            }
            slots.push(srow);
            temps.push(trow);
        }
        SccFrame { tag, running, result, slots, temps }
    }

    /// The dispatcher's signature: the tag, then every member's slots in order.
    fn dispatcher_params(&self, var_table: &VarTable) -> Vec<IrParam> {
        let own = |var: VarId, ty: Ty| IrParam {
            var,
            ty,
            name: var_table.get(var).name,
            borrow: ParamBorrow::Own,
            is_mut: false,
            open_record: None,
            default: None,
            attrs: vec![],
        };
        let mut params = vec![own(self.tag, Ty::Int)];
        for row in &self.slots {
            for (sv, sty) in row {
                params.push(own(*sv, sty.clone()));
            }
        }
        params
    }
}

fn rewrite_scc(
    functions: &mut Vec<IrFunction>,
    var_table: &mut VarTable,
    fis: &[usize],
    ret_ty: Ty,
    scc_no: usize,
) {
    let member_idx: HashMap<almide_base::intern::Sym, usize> =
        fis.iter().enumerate().map(|(k, &i)| (functions[i].name, k)).collect();
    let frame = SccFrame::alloc(var_table, functions, fis, &ret_ty, scc_no);
    let arm_bodies = member_arm_bodies(functions, fis, &member_idx, &frame);
    let chain = tag_dispatch_chain(arm_bodies, frame.tag);
    let disp_name = almide_base::intern::sym(&format!(
        "__mutual_tco_{scc_no}_{}",
        functions[fis[0]].name.as_str()
    ));
    let dispatcher = IrFunction {
        name: disp_name,
        params: frame.dispatcher_params(var_table),
        ret_ty: ret_ty.clone(),
        body: dispatcher_body(chain, &frame, &ret_ty),
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Private,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], // fresh-fn: merged dispatcher over frame slots; members keep their own flags
        module_origin: None,
    };
    retarget_members_to_dispatcher(functions, fis, &frame, disp_name, &ret_ty);
    functions.push(dispatcher);
}

/// Each member's body with its params substituted by its own slots, then its
/// tail positions rewritten to jumps / base cases.
fn member_arm_bodies(
    functions: &[IrFunction],
    fis: &[usize],
    member_idx: &HashMap<almide_base::intern::Sym, usize>,
    frame: &SccFrame,
) -> Vec<IrExpr> {
    fis.iter()
        .enumerate()
        .map(|(k, &fi)| {
            let mut body = functions[fi].body.clone();
            for (pi, p) in functions[fi].params.iter().enumerate() {
                let repl = var_expr(frame.slots[k][pi].0, p.ty.clone());
                body = substitute::substitute_var_in_expr(&body, p.var, &repl);
            }
            rewrite_tail(body, member_idx, frame)
        })
        .collect()
}

/// `if tag == 0 { arm0 } else if tag == 1 { arm1 } else { armN }` — the last
/// member rides the final else, so the chain is total without a trap arm.
fn tag_dispatch_chain(mut arm_bodies: Vec<IrExpr>, tag_var: VarId) -> IrExpr {
    let mut chain = arm_bodies.pop().expect("scc has >= 2 members");
    for (k, arm) in arm_bodies.into_iter().enumerate().rev() {
        let cond = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::Eq,
                left: Box::new(var_expr(tag_var, Ty::Int)),
                right: Box::new(lit_int(k as i64)),
            },
            ty: Ty::Bool,
            span: None,
            def_id: None,
        };
        chain = IrExpr {
            kind: IrExprKind::If { cond: Box::new(cond), then: Box::new(arm), else_: Box::new(chain) },
            ty: Ty::Unit,
            span: None,
            def_id: None,
        };
    }
    chain
}

/// `{ var running = true; var result = 0; while running { <chain> }; result }`.
fn dispatcher_body(chain: IrExpr, frame: &SccFrame, ret_ty: &Ty) -> IrExpr {
    let while_expr = IrExpr {
        kind: IrExprKind::While {
            cond: Box::new(var_expr(frame.running, Ty::Bool)),
            body: vec![IrStmt { kind: IrStmtKind::Expr { expr: chain }, span: None }],
        },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    };
    let bind = |var: VarId, ty: Ty, value: IrExpr| IrStmt {
        kind: IrStmtKind::Bind { var, mutability: Mutability::Var, ty, value },
        span: None,
    };
    IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![
                bind(frame.running, Ty::Bool, lit_bool(true)),
                bind(frame.result, ret_ty.clone(), scalar_zero(ret_ty)),
                IrStmt { kind: IrStmtKind::Expr { expr: while_expr }, span: None },
            ],
            expr: Some(Box::new(var_expr(frame.result, ret_ty.clone()))),
        },
        ty: ret_ty.clone(),
        span: None,
        def_id: None,
    }
}

/// Members become thin wrappers: own args in own slots, zero-literals in the
/// others (never read before the entry arm assigns or jumps away).
fn retarget_members_to_dispatcher(
    functions: &mut [IrFunction],
    fis: &[usize],
    frame: &SccFrame,
    disp_name: almide_base::intern::Sym,
    ret_ty: &Ty,
) {
    for (k, &fi) in fis.iter().enumerate() {
        let mut args: Vec<IrExpr> = vec![lit_int(k as i64)];
        for (m, row) in frame.slots.iter().enumerate() {
            for (i, (_sv, sty)) in row.iter().enumerate() {
                if m == k {
                    let p = &functions[fi].params[i];
                    args.push(var_expr(p.var, p.ty.clone()));
                } else {
                    args.push(scalar_zero(sty));
                }
            }
        }
        functions[fi].body = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: disp_name },
                args,
                type_args: vec![],
            },
            ty: ret_ty.clone(),
            span: None,
            def_id: None,
        };
    }
}

/// Rewrite a member body's TAIL positions: an intra-SCC call becomes the
/// slot-assign jump, everything else becomes the result-and-halt base case.
/// Any shape not named here is a base case, and a base case containing a
/// member call still calls that member's WRAPPER — correct by construction,
/// merely un-optimized — so the catch-all is safe against future IrExprKind
/// growth (it moves the whole expr into the output, dropping nothing).
fn rewrite_tail(
    expr: IrExpr,
    member_idx: &HashMap<almide_base::intern::Sym, usize>,
    frame: &SccFrame,
) -> IrExpr {
    match expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            if member_idx.contains_key(&name) =>
        {
            emit_jump(member_idx[&name], args, frame)
        }
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond,
                then: Box::new(rewrite_tail(*then, member_idx, frame)),
                else_: Box::new(rewrite_tail(*else_, member_idx, frame)),
            },
            ty: Ty::Unit,
            span: expr.span,
            def_id: None,
        },
        IrExprKind::Match { subject, arms } => IrExpr {
            kind: IrExprKind::Match {
                subject,
                arms: arms
                    .into_iter()
                    .map(|arm| IrMatchArm {
                        pattern: arm.pattern,
                        guard: arm.guard,
                        body: rewrite_tail(arm.body, member_idx, frame),
                    })
                    .collect(),
            },
            ty: Ty::Unit,
            span: expr.span,
            def_id: None,
        },
        IrExprKind::Block { stmts, expr: Some(tail) } => IrExpr {
            kind: IrExprKind::Block {
                stmts,
                expr: Some(Box::new(rewrite_tail(*tail, member_idx, frame))),
            },
            ty: Ty::Unit,
            span: expr.span,
            def_id: None,
        },
        kind => {
            let base = IrExpr { kind, ty: expr.ty, span: expr.span, def_id: expr.def_id };
            IrExpr {
                kind: IrExprKind::Block {
                    stmts: vec![
                        assign(frame.result, base),
                        assign(frame.running, lit_bool(false)),
                    ],
                    expr: None,
                },
                ty: Ty::Unit,
                span: None,
                def_id: None,
            }
        }
    }
}

/// `f → g(args)` in tail position: bind g's temps from the args (all reads
/// see pre-jump slot values), move the temps into g's slots, set the tag.
/// The arm then ends; the running loop re-checks and dispatches on the new
/// tag — no `continue` needed, which is what keeps the dispatcher inside the
/// scalar-loop subset every backend already executes.
fn emit_jump(target: usize, args: Vec<IrExpr>, frame: &SccFrame) -> IrExpr {
    let mut stmts: Vec<IrStmt> = Vec::new();
    for (i, arg) in args.into_iter().enumerate() {
        let (tv, tty) = &frame.temps[target][i];
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: *tv,
                mutability: Mutability::Let,
                ty: tty.clone(),
                value: arg,
            },
            span: None,
        });
    }
    for (i, (sv, sty)) in frame.slots[target].iter().enumerate() {
        let (tv, _) = &frame.temps[target][i];
        stmts.push(assign(*sv, var_expr(*tv, sty.clone())));
    }
    stmts.push(assign(frame.tag, lit_int(target as i64)));
    IrExpr {
        kind: IrExprKind::Block { stmts, expr: None },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    }
}
