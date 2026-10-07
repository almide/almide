//! Branch-lift pass: lift a heap-typed `let`-bound `if`/`match` into a tail
//! helper function so the v1 trust-spine wasm renderer can lower it.
//!
//! ## The problem this solves
//!
//! The v1 MIR renderer (`almide-mir::render_wasm`) WALLS on a `let`/`var` whose
//! value is a heap-result `if`/`match`:
//!
//! ```almide
//! let result = match binary_search(sorted, t) {   // String result = HEAP
//!   some(idx) => "found at index " + int.to_string(idx)
//!   none      => "not found"
//! }
//! ```
//!
//! The flat ownership certificate cannot attribute one scope-end `Drop` to
//! exactly-one-of-two mutually-exclusive arm allocations (see
//! `almide-mir/src/lower/binds_p2.rs` — the `If`/`Match` heap-bind wall). A tail
//! heap-result `match` moves each arm's value OUT (`im` per arm), but a let-bound
//! value is held and dropped at scope end, which would release a moved-out object
//! — the checker rejects the resulting `im·im·d`.
//!
//! ## The fix (validated empirically)
//!
//! Rewrite the let-bound branch into a TAIL helper call. The branch becomes the
//! *body* of a fresh top-level function (a tail position, where the existing tail
//! handlers — `try_lower_variant_value_match` and friends — render it soundly,
//! each arm moving its value out as the function's return). The original `let`
//! then binds the helper's CALL RESULT, which is a proven shape (`binds.rs` /
//! `binds_p3.rs` — a heap call-result bound to a `let`):
//!
//! ```almide
//! fn __branch_lift_0(sorted: List[Int], t: Int) -> String = match binary_search(sorted, t) {
//!   some(idx) => "found at index " + int.to_string(idx)
//!   none      => "not found"
//! }
//! // …
//! let result = __branch_lift_0(sorted, t)
//! ```
//!
//! No new ownership-cert / Coq machinery is needed: the lift moves the construct
//! into a position the existing proven lowering already handles.
//!
//! ## Why this lives in `almide-optimize`
//!
//! The pass MUST run in the SHARED frontend pipeline so BOTH the v1 trust-spine
//! path (`parse → check → lower → optimize → mono → ir_link`) AND the standard
//! codegen path see the lifted form. `optimize::optimize_program` is exactly that
//! shared cut point (`render_program.rs` calls it). Running here (before mono /
//! ir_link) means the synthesized helper is monomorphized and linked like any
//! other user function.
//!
//! ## Scope (minimal blast radius)
//!
//! ONLY heap-typed `let`/`var`-bound `If`/`Match` that sit **inside a loop body**
//! (`for-in` / `while`) are lifted. This is the precise residual the MIR renderer
//! cannot already handle:
//!
//! - A top-level / block-level let-bound heap branch is handled by the existing,
//!   tested MIR tail-duplication desugar (`almide-mir`'s
//!   `desugar_let_bound_heap_branch` — it copies the continuation into each arm).
//! - That desugar's recursion (`desugar_nested_branch_arms`) descends into `if` /
//!   `match` arms and block tails, but **NOT into `ForIn` / `While` bodies**. So a
//!   let-bound heap branch nested in a `for`/`while` loop body (e.g.
//!   `examples/binary-search.almd`'s `for t in targets { let r = match …; … }`) is
//!   never reached and walls.
//!
//! Lifting ONLY the in-loop case means:
//! - the existing tail-duplication path (and its tests / corpus behavior) is left
//!   completely UNTOUCHED — the two paths never overlap; and
//! - the genuinely-walled residual (in-loop let-bound heap branches) now renders.
//!
//! Scalar binds, tail-position branches, and every other construct are likewise
//! left UNTOUCHED. The lift is a pure structural rewrite preserving observable
//! behavior (the branch becomes a tail fn body; the bind becomes its call result).

use std::collections::HashSet;
use almide_ir::free_vars::free_vars;
use almide_ir::substitute::substitute_var_in_expr;
use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use almide_ir::*;
use almide_base::intern::{sym, Sym};
use almide_ir::mut_args::{place_root, CallWrites, MutParamTable};
use almide_lang::types::{is_heap_ty, Ty};

/// Lift every heap-typed `let`/`var`-bound `if`/`match` value into a fresh tail
/// helper function, replacing the bind value with a call to that helper.
pub fn lift_heap_branch_binds(program: &mut IrProgram) {
    let mut counter: u32 = 0;
    let mut_params = MutParamTable::of(program);

    // Root program: function bodies + top-level let initializers all share the
    // program-wide `var_table`, so a helper synthesized from any of them resolves
    // against the same VarId namespace.
    {
        let IrProgram { functions, top_lets, var_table, .. } = &mut *program;
        let globals: HashSet<VarId> = top_lets.iter().map(|tl| tl.var).collect();
        let mut lifter = BranchLifter { vt: var_table, counter: &mut counter, new_funcs: Vec::new(), loop_depth: 0,
        dense_depth: 0, globals, mut_params: &mut_params, scope: None };
        for func in functions.iter_mut() {
            let before = lifter.new_funcs.len();
            lifter.visit_expr_mut(&mut func.body);
            // #3041: a helper lifted out of an effect fn's body keeps its
            // origin's declaration for the capability witness.
            if func.declares_effect() {
                IrFunction::mark_effect_origin(&mut lifter.new_funcs[before..]);
            }
        }
        for tl in top_lets.iter_mut() {
            lifter.visit_expr_mut(&mut tl.value);
        }
        let lifted = lifter.new_funcs;
        functions.extend(lifted);
    }

    // Imported modules: each carries its own `var_table`, so helpers lifted from a
    // module's functions must be placed in that module (their body's VarIds index
    // the module's table, not the program's).
    for module in program.modules.iter_mut() {
        let IrModule { name, functions, top_lets, var_table, .. } = &mut *module;
        let globals: HashSet<VarId> = top_lets.iter().map(|tl| tl.var).collect();
        let scope = Some(*name);
        let mut lifter = BranchLifter { vt: var_table, counter: &mut counter, new_funcs: Vec::new(), loop_depth: 0, dense_depth: 0, globals, mut_params: &mut_params, scope };
        for func in functions.iter_mut() {
            let before = lifter.new_funcs.len();
            lifter.visit_expr_mut(&mut func.body);
            // #3041: a helper lifted out of an effect fn's body keeps its
            // origin's declaration for the capability witness.
            if func.declares_effect() {
                IrFunction::mark_effect_origin(&mut lifter.new_funcs[before..]);
            }
        }
        for tl in top_lets.iter_mut() {
            lifter.visit_expr_mut(&mut tl.value);
        }
        let lifted = lifter.new_funcs;
        functions.extend(lifted);
    }
}

/// A mut-visitor that lifts heap-branch binds. It descends the whole IR via
/// `walk_stmt_mut` / `walk_expr_mut` (so nested binds in blocks, loop bodies, and
/// branch arms are all reached), and at each `Bind` statement whose value is a
/// heap-typed `if`/`match`, replaces that value with a call to a synthesized tail
/// helper. The visitor walks children FIRST (bottom-up), so a branch arm that itself
/// contains a liftable bind is rewritten before the outer bind is lifted — the outer
/// helper body then already contains the inner helper call.
///
/// `loop_depth` tracks `for-in` / `while` nesting. Lifting fires for: any heap
/// `if`/`match` inside a loop body (the region the MIR tail-duplication desugar
/// cannot reach), PLUS an out-of-loop heap VARIANT `match` (Some/None/Ok/Err/…) —
/// the desugar covers an out-of-loop `if` but not a `match`, and a variant match's
/// subject materializes once (so MIR call count == IR call count). An out-of-loop
/// LITERAL-pattern `match` is left to the desugar: its `subject == lit` chain
/// duplicates the subject's calls, which `count_ir_calls` can't predict (a `mir > ir`
/// caps-backing breach). See `visit_stmt_mut` and the module docs.
struct BranchLifter<'a> {
    vt: &'a mut VarTable,
    counter: &'a mut u32,
    new_funcs: Vec<IrFunction>,
    loop_depth: u32,
    /// Module-level top-let ids (immutable AND mutable): NEVER captured as helper
    /// params — they resolve as globals inside the helper too, and capturing a
    /// MUTABLE one would shadow it with a snapshot param (its assigns then write
    /// the param — a lost global write).
    globals: HashSet<VarId>,
    /// >0 while inside a Block whose heap let-bound `if`/`match` COUNT exceeds the MIR
    /// tail-duplication desugar's bounded-duplication gate (rest > 3 declines there —
    /// the value_deep_eq 5-chain ceiling). In that region an out-of-loop heap `if` is
    /// lifted too: each bind becomes ONE helper call (chain-length immune, no 2^n
    /// duplication), the sound shape the try-lowered helper renders.
    dense_depth: u32,
    /// Every fn's `mut` parameter positions (see [`MutParamTable`]): an
    /// argument passed there is WRITTEN by the call (#2907), exactly like an
    /// `Assign` to it.
    mut_params: &'a MutParamTable,
    /// The module whose fns this lifter walks (`None` = the root program): a
    /// bare `Named` call resolves in this scope first.
    scope: Option<Sym>,
}

/// The vars a call in `e` passes at a `mut` parameter position (as the
/// argument itself or as the record a field argument is read from): the call
/// writes them back, so outlining it into a helper that takes them by value
/// would lose the write.
fn collect_mut_arg_vars(e: &IrExpr, mut_params: &MutParamTable, scope: Option<Sym>, out: &mut HashSet<u32>) {
    struct V<'m, 'o> {
        mut_params: &'m MutParamTable,
        scope: Option<Sym>,
        out: &'o mut HashSet<u32>,
    }
    impl visit::IrVisitor for V<'_, '_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target, args, .. } = &e.kind {
                // Only an identified callee counts here, as before the shared
                // table (LICM, which can only lose by guessing, treats an
                // unidentified one as writing every place argument).
                if let CallWrites::Positions(idxs) = self.mut_params.writes(target, self.scope) {
                    // The written place may be a field path of any depth
                    // (`bump(o.mid.inner)`): its ROOT var is written. Reading
                    // one `Member` level only lifted that call into a helper
                    // taking `o` by value — a native rustc error and a lost
                    // write on wasm.
                    for id in idxs.iter().filter_map(|&i| args.get(i).and_then(place_root)) {
                        self.out.insert(id.0);
                    }
                }
            }
            visit::walk_expr(self, e);
        }
    }
    visit::IrVisitor::visit_expr(&mut V { mut_params, scope, out }, e);
}

impl<'a> IrMutVisitor for BranchLifter<'a> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // A `for-in` / `while` body is the region the MIR tail-duplication desugar
        // cannot reach; mark it so binds within get lifted. Descend with the depth
        // raised, then restore it (so a SIBLING construct after the loop is not
        // mistakenly treated as in-loop).
        let is_loop = matches!(expr.kind, IrExprKind::ForIn { .. } | IrExprKind::While { .. });
        if is_loop {
            self.loop_depth += 1;
        }
        // A DENSE straight-line chain: >3 heap let-bound branches in ONE block is past
        // the MIR desugar's bounded-duplication gate — mark the region so the If arm
        // below lifts (see `dense_depth`).
        let is_dense = match &expr.kind {
            IrExprKind::Block { stmts, .. } => {
                stmts
                    .iter()
                    .filter(|s| stmt_holds_heap_if(s))
                    .count()
                    > 3
            }
            _ => false,
        };
        if is_dense {
            self.dense_depth += 1;
        }
        walk_expr_mut(self, expr);
        if is_dense {
            self.dense_depth -= 1;
        }
        if is_loop {
            self.loop_depth -= 1;
        }
        // In a DENSE region, lift a heap-result `if`/simple-pattern `match` EXPRESSION in
        // place (the call-arg position the MIR ANF lift would otherwise turn into the >3
        // let-bound chain the bounded-duplication gate refuses — e.g. `println(match X {
        // some(v) => …, none => … })` chained 5+ times, alias_combinator_rc/
        // codec_decode_errors). One helper call per branch — chain-length immune.
        // Bottom-up (children walked above), so nested branches lift inside-out. `Match` is
        // gated to the SAME simple-pattern subset the stmt-level Bind arm below uses (Some/
        // None/Ok/Err/Bind/Wildcard): a literal-pattern match desugars to an `if subject ==
        // lit` chain that DUPLICATES the subject's calls (an unpredictable `mir > ir` count),
        // and a custom-variant/tuple/list/record-pattern match can still wall in the tail
        // handler — lifting either just relocates the wall into a dead helper.
        let heap_branch_kind = match &expr.kind {
            IrExprKind::If { .. } => true,
            IrExprKind::Match { arms, .. } => arms.iter().all(|a| {
                matches!(
                    a.pattern,
                    IrPattern::Some { .. }
                        | IrPattern::None
                        | IrPattern::Ok { .. }
                        | IrPattern::Err { .. }
                        | IrPattern::Bind { .. }
                        | IrPattern::Wildcard
                )
            }),
            _ => false,
        };
        if self.dense_depth > 0
            && heap_branch_kind
            && is_heap_ty(&expr.ty)
            && !holds_error_op(expr)
        {
            let ty = expr.ty.clone();
            self.lift_bind_value(ty, expr);
        }
    }

    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        // Bottom-up: lift inside the value's sub-expressions first.
        walk_stmt_mut(self, stmt);

        if let IrStmtKind::Bind { ty, value, .. } = &mut stmt.kind {
            if !is_heap_ty(ty) {
                return;
            }
            let fire = match &value.kind {
                // Inside a loop body the desugar cannot reach, lift any heap `if`/`match`.
                // Out of a loop, the MIR tail-duplication desugar already covers an `if`, so an
                // `if` is left to it. It does NOT cover a `match` — lift exactly an OPTION/RESULT
                // match (`some/none/ok/err`, plus binder/wildcard catch-alls): its subject is
                // materialized ONCE and the tail handler (`try_lower_variant_value_match` /
                // `try_lower_result_match`) renders it for both scalar AND heap payloads (verified).
                //   • A LITERAL-pattern match (`"a" => …`, `0 => …`) lowers to an `if subject == lit`
                //     chain that DUPLICATES the subject's calls — a count `count_ir_calls` can't
                //     predict, tripping the `mir > ir` caps-backing wall.
                //   • A custom-variant / tuple / list / record-pattern match can still wall in the
                //     tail handler, so lifting it just relocates the wall into a dead helper.
                // Both are left to the existing desugar (or a later widening of this gate).
                IrExprKind::Match { arms, .. } => {
                    self.loop_depth > 0
                        || arms.iter().all(|a| {
                            matches!(
                                a.pattern,
                                IrPattern::Some { .. }
                                    | IrPattern::None
                                    | IrPattern::Ok { .. }
                                    | IrPattern::Err { .. }
                                    | IrPattern::Bind { .. }
                                    | IrPattern::Wildcard
                            )
                        })
                }
                IrExprKind::If { .. } => self.loop_depth > 0 || self.dense_depth > 0,
                _ => false,
            };
            if fire && !holds_error_op(value) {
                self.lift_bind_value(ty.clone(), value);
            }
        }
    }
}

/// Does this branch carry an error-PROPAGATING `!`/`?` (`IrExprKind::Unwrap`/`Try`) that
/// belongs to the ENCLOSING function? The helper this pass synthesizes is a plain
/// `fn … -> <payload>` — non-effect, payload-returning — so a propagation lifted into it
/// has NO channel out: the native backend emitted `?` in a non-`Result` fn (rustc
/// E0277) and the wasm side always-wrapped the helper's body in `ok(..)` while the call
/// site bound it as the bare payload, printing the ok-discriminant (`match json.parse("hi")
/// { ok(p) => p, err(_) => json.parse("[1,2]")! }` → `true`). DECLINE the lift; the branch
/// keeps its in-place route, where the `!` still sits in the fn that owns it. A `Lambda`
/// body is NOT descended — a `!` there propagates to the lambda's own result, which the
/// lift carries along unchanged.
///
/// #3451: the same holds for every other exit that targets the enclosing fn or loop. A
/// `guard … else err(..)` lowers to an early `return Err(..)` (rustc E0308 in the
/// payload-returning helper), and a `break`/`continue` — bare or as a guard's `else` —
/// would leave the helper with no loop to target (IR verify: "outside of loop"). A
/// `Guard` statement or a `Break`/`Continue` anywhere in the branch declines the lift too.
fn holds_error_op(e: &IrExpr) -> bool {
    struct V {
        found: bool,
    }
    impl visit::IrVisitor for V {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found || matches!(e.kind, IrExprKind::Lambda { .. }) {
                return;
            }
            if matches!(
                e.kind,
                IrExprKind::Unwrap { .. }
                    | IrExprKind::Try { .. }
                    | IrExprKind::Break
                    | IrExprKind::Continue
            ) {
                self.found = true;
                return;
            }
            visit::walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if matches!(s.kind, IrStmtKind::Guard { .. }) {
                self.found = true;
                return;
            }
            visit::walk_stmt(self, s);
        }
    }
    let mut v = V { found: false };
    visit::IrVisitor::visit_expr(&mut v, e);
    v.found
}

impl<'a> BranchLifter<'a> {
    /// Replace the heap-branch `value` in place with a call to a freshly
    /// synthesized tail helper `fn __branch_lift_N(p…) -> ty = <original branch>`.
    fn lift_bind_value(&mut self, ty: Ty, value: &mut IrExpr) {
        // #2062: helper parameters are values, not the enclosing storage places.
        // Outlining a write would update a snapshot and silently lose the write.
        // Local bindings may still mutate, and globals keep their own storage.
        // Include write-only targets and specialized collection mutations too.
        let mut assigned = HashSet::new();
        almide_ir::collect_assigned_vars(value, &mut assigned);
        collect_mut_arg_vars(value, self.mut_params, self.scope, &mut assigned);
        if !assigned.is_empty() {
            let locals = almide_ir::free_vars::bound_vars(value);
            if assigned.iter().any(|id| {
                let var = VarId(*id);
                !locals.contains(&var) && !self.globals.contains(&var)
            }) {
                return;
            }
        }

        // 1. The branch is evaluated in the enclosing scope BEFORE the bind takes
        //    effect, so its free variables are exactly the enclosing locals it
        //    references (params, prior `let`s, loop binders). The bound var itself
        //    cannot appear (it is not yet defined). `bound = ∅`: nothing is already
        //    in scope that we want to exclude from the capture set.
        let bound: HashSet<VarId> = HashSet::new();
        // Deterministically sorted by VarId; module-level top-lets excluded (they
        // resolve as globals in the helper — capturing a mutable one would shadow it).
        let params: Vec<VarId> = free_vars(value, &bound)
            .into_iter()
            .filter(|v| !self.globals.contains(v))
            .collect();

        // 2. Synthesize the helper name + take the branch expr out as the body.
        let id = *self.counter;
        *self.counter = id + 1;
        // NOT `__`-prefixed: this is a real user-fn DEFINITION both backends emit, but the
        // codegen builtin-lowering pass rewrites EVERY `__`-prefixed Named CALL to a runtime
        // intrinsic (`almide_rt_<name>`) — which mismatches this definition on the native Rust
        // path (cannot-find-fn `almide_rt___branch_lift_0`). A plain name keeps it a user fn
        // everywhere; the v1 MIR renderer treats it as a let-bound call result (a proven shape).
        let func_name = sym(&format!("branch_lift_synth_{}", id));
        let body = std::mem::replace(value, IrExpr::default());
        let body_span = body.span;

        // 3. Build the helper's parameters from the captured free vars, each a
        //    FRESH VarId with the captured var's name and type, and rename the
        //    body onto them. The helper used to keep the enclosing fn's ids as
        //    its params, so one VarId was bound in two functions — and every
        //    VarId-keyed ownership annotation (`borrowed_loop_vars`,
        //    `shared_mut_vars`, `tco_owned_params`, the clone pass's last-use
        //    counts) then had to be second-guessed with "fn-local truth" gates
        //    wherever the helper was rendered (#1130, #1143, #1232, #2194).
        //    A VarId is bound in exactly one function (`verify_ir` checks it),
        //    so an annotation about it means the same thing everywhere.
        let mut body = body;
        let func_params: Vec<IrParam> = params
            .iter()
            .map(|&vid| {
                let info = self.vt.get(vid).clone();
                let fresh = self.vt.alloc(info.name, info.ty.clone(), Mutability::Let, info.span);
                let read = IrExpr { kind: IrExprKind::Var { id: fresh }, ty: info.ty.clone(), span: info.span, def_id: None };
                body = substitute_var_in_expr(&body, vid, &read);
                IrParam {
                    var: fresh,
                    ty: info.ty,
                    name: info.name,
                    borrow: ParamBorrow::Own,
                    is_mut: false,
                    open_record: None,
                    default: None,
                    attrs: vec![],
                }
            })
            .collect();

        self.new_funcs.push(IrFunction {
            name: func_name,
            params: func_params,
            ret_ty: ty.clone(),
            body,
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
            mutated_params: vec![], // fresh-fn: lifted helper owns fresh non-mut params
            module_origin: None,
        });

        // 4. Replace the bind value with a call to the helper, passing each captured
        //    var as an argument (in the same deterministic param order).
        let args: Vec<IrExpr> = params
            .iter()
            .map(|&vid| IrExpr {
                kind: IrExprKind::Var { id: vid },
                ty: self.vt.get(vid).ty.clone(),
                span: body_span,
                def_id: None,
            })
            .collect();

        *value = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: func_name },
                args,
                type_args: vec![],
            },
            ty,
            span: body_span,
            def_id: None,
        };
    }
}

// The heap classification is imported from `almide_lang::types` (#926) — the
// dependency-cycle reason this file used to carry a verbatim copy is gone now
// that the definition lives beside `Ty` itself, upstream of both this crate and
// `almide-mir` (whose `lower::is_heap_ty` re-exports the same fn).
/// Does the statement contain a HEAP-result `if`/`match` anywhere (bind value, call
/// argument, operand)? The dense-chain scan counts these BEFORE the MIR-side ANF lift
/// rewrites call-arg branches into let binds.
fn stmt_holds_heap_if(stmt: &IrStmt) -> bool {
    use almide_ir::visit::{walk_stmt, IrVisitor};
    struct V(bool);
    impl IrVisitor for V {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::If { .. } | IrExprKind::Match { .. })
                && is_heap_ty(&e.ty)
            {
                self.0 = true;
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut v = V(false);
    walk_stmt(&mut v, stmt);
    v.0
}


#[cfg(test)]
mod tests {
    use super::*;

    fn lit_str(v: &str) -> IrExpr {
        IrExpr { kind: IrExprKind::LitStr { value: v.into() }, ty: Ty::String, span: None, def_id: None }
    }
    fn lit_int(v: i64) -> IrExpr {
        IrExpr { kind: IrExprKind::LitInt { value: v }, ty: Ty::Int, span: None, def_id: None }
    }
    fn var(id: u32, ty: Ty) -> IrExpr {
        IrExpr { kind: IrExprKind::Var { id: VarId(id) }, ty, span: None, def_id: None }
    }
    /// `if <cond_var> then <then> else <else_>` typed `ty`.
    fn iff(cond_var: u32, then: IrExpr, else_: IrExpr, ty: Ty) -> IrExpr {
        IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(var(cond_var, Ty::Bool)),
                then: Box::new(then),
                else_: Box::new(else_),
            },
            ty,
            span: None,
            def_id: None,
        }
    }
    fn bind(var_id: u32, ty: Ty, value: IrExpr) -> IrStmt {
        IrStmt {
            kind: IrStmtKind::Bind { var: VarId(var_id), mutability: Mutability::Let, ty, value },
            span: None,
        }
    }
    fn block(stmts: Vec<IrStmt>) -> IrExpr {
        IrExpr { kind: IrExprKind::Block { stmts, expr: None }, ty: Ty::Unit, span: None, def_id: None }
    }
    /// `for <var0> in <iter> { <body> }` (iter is an empty list; only structure matters).
    fn for_in(loop_var: u32, body: Vec<IrStmt>) -> IrExpr {
        IrExpr {
            kind: IrExprKind::ForIn {
                var: VarId(loop_var),
                var_tuple: None,
                iterable: Box::new(IrExpr {
                    kind: IrExprKind::List { elements: vec![] },
                    ty: Ty::Unit,
                    span: None,
                    def_id: None,
                }),
                body,
            },
            ty: Ty::Unit,
            span: None,
            def_id: None,
        }
    }

    /// Build a minimal program: one `main` whose body is `body`, with a var_table
    /// sized to cover every VarId referenced (all typed via `vt_tys`).
    fn program_with_main(body: IrExpr, vt_tys: &[Ty]) -> IrProgram {
        let mut var_table = VarTable::new();
        for (i, ty) in vt_tys.iter().enumerate() {
            var_table.alloc(sym(&format!("v{i}")), ty.clone(), Mutability::Let, None);
        }
        let main = IrFunction {
            name: sym("main"),
            params: vec![],
            ret_ty: Ty::Unit,
            body,
            is_effect: true,
                is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![], // fresh-fn: lifted helper owns fresh non-mut params
            module_origin: None,
        };
        IrProgram { functions: vec![main], var_table, ..Default::default() }
    }

    fn main_bind_value_kind(p: &IrProgram) -> &IrExprKind {
        // main's body: ForIn whose body[0] is the bind we care about.
        let main = p.functions.iter().find(|f| f.name == sym("main")).unwrap();
        let IrExprKind::ForIn { body, .. } = &main.body.kind else { panic!("expected ForIn") };
        let IrStmtKind::Bind { value, .. } = &body[0].kind else { panic!("expected Bind") };
        &value.kind
    }

    #[test]
    fn lifts_in_loop_heap_branch_to_helper_call() {
        // main: for v0 in [] { let v2: String = if v1 then "a" else "b" }
        //   v0 = loop var, v1 = a Bool free var, v2 = the bound String.
        let branch = iff(1, lit_str("a"), lit_str("b"), Ty::String);
        let body = for_in(0, vec![bind(2, Ty::String, branch)]);
        let mut prog = program_with_main(body, &[Ty::Unit, Ty::Bool, Ty::String]);

        lift_heap_branch_binds(&mut prog);

        // The bind value is now a call to the synthesized helper.
        match main_bind_value_kind(&prog) {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                assert_eq!(name.as_str(), "branch_lift_synth_0", "deterministic helper name");
                // Free var of the branch = the Bool cond v1 (the String literals are not vars).
                assert_eq!(args.len(), 1, "captures exactly the one free var");
                assert!(matches!(args[0].kind, IrExprKind::Var { id: VarId(1) }));
            }
            other => panic!("in-loop heap branch must be lifted to a Call, got {other:?}"),
        }
        // A `__branch_lift_0` fn was synthesized, Private, returning String, body = the if.
        let helper = prog
            .functions
            .iter()
            .find(|f| f.name == sym("branch_lift_synth_0"))
            .expect("helper synthesized");
        assert_eq!(helper.visibility, IrVisibility::Private);
        assert_eq!(helper.ret_ty, Ty::String);
        assert_eq!(helper.params.len(), 1);
        // #3041: lifted out of an `effect fn` (main), the helper keeps that
        // declaration for the capability witness — as a marker, not the ABI flag.
        assert!(!helper.is_effect && helper.is_effect_origin() && helper.declares_effect());
        // The param is a FRESH id (the table had 3 vars), named and typed like
        // the captured `v1`, and the body reads it — the enclosing fn's `v1`
        // is bound in one function only.
        let fresh = helper.params[0].var;
        assert_eq!(fresh, VarId(3));
        assert_eq!(prog.var_table.get(fresh).ty, Ty::Bool);
        let IrExprKind::If { cond, .. } = &helper.body.kind else { panic!("helper body is the branch") };
        assert!(matches!(cond.kind, IrExprKind::Var { id } if id == fresh), "the body reads the fresh param");
    }

    /// #3451: a branch holding an exit that targets the enclosing fn or loop — a
    /// `guard` (its `else err(..)` is an early `return Err`), a bare `break` /
    /// `continue` — must stay inline: the payload-returning helper has no channel
    /// for it (rustc E0308 / IR verify "outside of loop").
    #[test]
    fn declines_a_branch_holding_a_guard_break_or_continue() {
        let exit = |kind: IrExprKind| IrExpr { kind, ty: Ty::Unit, span: None, def_id: None };
        let guarded = |else_: IrExpr| IrStmt {
            kind: IrStmtKind::Guard { cond: var(1, Ty::Bool), else_ },
            span: None,
        };
        let shapes = [
            vec![guarded(exit(IrExprKind::Continue))],
            vec![IrStmt { kind: IrStmtKind::Expr { expr: exit(IrExprKind::Break) }, span: None }],
            vec![IrStmt { kind: IrStmtKind::Expr { expr: exit(IrExprKind::Continue) }, span: None }],
        ];
        for stmts in shapes {
            let then = IrExpr {
                kind: IrExprKind::Block { stmts, expr: Some(Box::new(lit_str("a"))) },
                ty: Ty::String,
                span: None,
                def_id: None,
            };
            let branch = iff(1, then, lit_str("b"), Ty::String);
            let body = for_in(0, vec![bind(2, Ty::String, branch)]);
            let mut prog = program_with_main(body, &[Ty::Unit, Ty::Bool, Ty::String]);
            lift_heap_branch_binds(&mut prog);
            assert!(matches!(main_bind_value_kind(&prog), IrExprKind::If { .. }), "exit branch stays inline");
            assert_eq!(prog.functions.len(), 1, "no helper synthesized");
        }
    }

    #[test]
    fn leaves_out_of_loop_heap_branch_untouched() {
        // main: { let v1: String = if v0 then "a" else "b" }  (NO enclosing loop)
        // The existing MIR tail-duplication desugar owns this case — do not lift.
        let branch = iff(0, lit_str("a"), lit_str("b"), Ty::String);
        let body = block(vec![bind(1, Ty::String, branch)]);
        let mut prog = program_with_main(body, &[Ty::Bool, Ty::String]);

        lift_heap_branch_binds(&mut prog);

        let main = prog.functions.iter().find(|f| f.name == sym("main")).unwrap();
        let IrExprKind::Block { stmts, .. } = &main.body.kind else { panic!() };
        let IrStmtKind::Bind { value, .. } = &stmts[0].kind else { panic!() };
        assert!(matches!(value.kind, IrExprKind::If { .. }), "out-of-loop branch stays inline");
        assert_eq!(prog.functions.len(), 1, "no helper synthesized");
    }

    #[test]
    fn leaves_scalar_in_loop_branch_untouched() {
        // main: for v0 in [] { let v2: Int = if v1 then 1 else 2 }
        // Int is scalar — the renderer handles a scalar let-bound if inline.
        let branch = iff(1, lit_int(1), lit_int(2), Ty::Int);
        let body = for_in(0, vec![bind(2, Ty::Int, branch)]);
        let mut prog = program_with_main(body, &[Ty::Unit, Ty::Bool, Ty::Int]);

        lift_heap_branch_binds(&mut prog);

        assert!(matches!(main_bind_value_kind(&prog), IrExprKind::If { .. }), "scalar branch stays inline");
        assert_eq!(prog.functions.len(), 1, "no helper synthesized");
    }

    /// #2931: `for v0 in [] { let v2: String = if v1 then { list.push(v3, "x"); "a" } else "b" }`
    /// with NO `list` module in the program — the wasm leg's IR, which does not
    /// lower a bridge-surface module. `list.push`'s `mut` marker comes from its
    /// bundled declaration, so the write to the enclosing `v3` blocks the lift
    /// exactly as it does on native, where the module is present.
    #[test]
    fn declines_a_branch_writing_an_outer_var_through_a_bundled_mut_param() {
        let push = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym("list"), func: sym("push"), def_id: None },
                args: vec![var(3, Ty::list(Ty::String)), lit_str("x")],
                type_args: vec![],
            },
            ty: Ty::Unit,
            span: None,
            def_id: None,
        };
        let then = IrExpr {
            kind: IrExprKind::Block { stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: push }, span: None }], expr: Some(Box::new(lit_str("a"))) },
            ty: Ty::String,
            span: None,
            def_id: None,
        };
        let branch = iff(1, then, lit_str("b"), Ty::String);
        let body = for_in(0, vec![bind(2, Ty::String, branch)]);
        let mut prog = program_with_main(body, &[Ty::Unit, Ty::Bool, Ty::String, Ty::list(Ty::String)]);

        lift_heap_branch_binds(&mut prog);

        assert!(matches!(main_bind_value_kind(&prog), IrExprKind::If { .. }), "the writing branch stays inline");
        assert_eq!(prog.functions.len(), 1, "no helper synthesized");
    }

    fn stub_fn(name: &str, mutated_params: Vec<usize>) -> IrFunction {
        IrFunction {
            name: sym(name),
            params: vec![],
            ret_ty: Ty::Unit,
            body: IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
            is_effect: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params,
            module_origin: None,
        }
    }

    fn lowered_module(name: &str, fns: Vec<IrFunction>) -> IrModule {
        IrModule {
            name: sym(name),
            versioned_name: None,
            type_decls: vec![],
            functions: fns,
            top_lets: vec![],
            var_table: VarTable::new(),
            exports: vec![],
            imports: vec![],
        }
    }

    /// `for v0 in [] { let v2: String = if v1 then { <call>; "a" } else "b" }`,
    /// v3 the outer var the call is handed.
    fn loop_with_writing_branch(call: IrExpr) -> IrProgram {
        let then = IrExpr {
            kind: IrExprKind::Block { stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: call }, span: None }], expr: Some(Box::new(lit_str("a"))) },
            ty: Ty::String,
            span: None,
            def_id: None,
        };
        let branch = iff(1, then, lit_str("b"), Ty::String);
        let body = for_in(0, vec![bind(2, Ty::String, branch)]);
        program_with_main(body, &[Ty::Unit, Ty::Bool, Ty::String, Ty::Unknown])
    }

    fn call(target: CallTarget, args: Vec<IrExpr>) -> IrExpr {
        IrExpr { kind: IrExprKind::Call { target, args, type_args: vec![] }, ty: Ty::Unit, span: None, def_id: None }
    }

    fn lifted(prog: &IrProgram) -> bool {
        !matches!(main_bind_value_kind(prog), IrExprKind::If { .. })
    }

    /// #2948: a root fn `insert(tag, mut m)` beside the LOWERED `map` module
    /// (the native leg's IR, where `map.insert(mut m, ..)` is present). The
    /// user's call writes its SECOND argument; reading the stdlib fn's
    /// position 0 instead let the branch be outlined and the write was lost on
    /// native only.
    #[test]
    fn a_user_fn_named_like_a_stdlib_mutator_keeps_its_own_mut_positions() {
        let c = call(CallTarget::Named { name: sym("insert") }, vec![lit_str("a"), var(3, Ty::Unknown)]);
        let mut prog = loop_with_writing_branch(c);
        prog.functions.push(stub_fn("insert", vec![1]));
        prog.modules.push(lowered_module("map", vec![stub_fn("insert", vec![0])]));

        lift_heap_branch_binds(&mut prog);

        assert!(!lifted(&prog), "the branch writing v3 through the user's `mut m` stays inline");
    }

    /// The #2931 / #2948 GATE, enumerated from the stdlib declarations rather
    /// than a hand list: for EVERY bundled fn that writes a param, a branch
    /// handing it an outer var is declined identically whether the module was
    /// lowered into the IR (native) or not (wasm) — and whatever
    /// `mutated_params` the lowered copy happens to carry (here: a wrong one), since
    /// a bundled fn's answer is read from its declaration on both legs.
    #[test]
    fn every_stdlib_mut_fn_declines_the_lift_whether_or_not_its_module_is_lowered() {
        let fns = almide_ir::mut_args::stdlib_mut_fns();
        assert!(!fns.is_empty());
        let mut disagree = Vec::new();
        for (module, func, idxs) in &fns {
            let arity = idxs.iter().max().copied().unwrap_or(0) + 1;
            let args: Vec<IrExpr> = (0..arity)
                .map(|i| if i == idxs[0] { var(3, Ty::Unknown) } else { lit_int(0) })
                .collect();
            let target = CallTarget::Module { module: sym(module), func: sym(func), def_id: None };

            let mut without = loop_with_writing_branch(call(target.clone(), args.clone()));
            lift_heap_branch_binds(&mut without);

            let mut with = loop_with_writing_branch(call(target, args));
            // A lowered copy whose table points past every argument: if the
            // lookup consulted the lowered module, the write would be missed.
            with.modules.push(lowered_module(module, vec![stub_fn(func, vec![arity])]));
            lift_heap_branch_binds(&mut with);

            if lifted(&without) || lifted(&with) {
                disagree.push(format!("{module}.{func}: lifted without={} with={}", lifted(&without), lifted(&with)));
            }
        }
        assert!(disagree.is_empty(), "a write through a stdlib mut param was outlined:\n{}", disagree.join("\n"));
    }
}
