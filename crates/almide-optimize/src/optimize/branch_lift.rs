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
use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use almide_ir::*;
use almide_base::intern::sym;
use almide_lang::types::Ty;

/// Lift every heap-typed `let`/`var`-bound `if`/`match` value into a fresh tail
/// helper function, replacing the bind value with a call to that helper.
pub fn lift_heap_branch_binds(program: &mut IrProgram) {
    let mut counter: u32 = 0;

    // Root program: function bodies + top-level let initializers all share the
    // program-wide `var_table`, so a helper synthesized from any of them resolves
    // against the same VarId namespace.
    {
        let IrProgram { functions, top_lets, var_table, .. } = &mut *program;
        let mut lifter = BranchLifter { vt: var_table, counter: &mut counter, new_funcs: Vec::new(), loop_depth: 0 };
        for func in functions.iter_mut() {
            lifter.visit_expr_mut(&mut func.body);
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
        let IrModule { functions, top_lets, var_table, .. } = &mut *module;
        let mut lifter = BranchLifter { vt: var_table, counter: &mut counter, new_funcs: Vec::new(), loop_depth: 0 };
        for func in functions.iter_mut() {
            lifter.visit_expr_mut(&mut func.body);
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
        walk_expr_mut(self, expr);
        if is_loop {
            self.loop_depth -= 1;
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
                IrExprKind::If { .. } => self.loop_depth > 0,
                _ => false,
            };
            if fire {
                self.lift_bind_value(ty.clone(), value);
            }
        }
    }
}

impl<'a> BranchLifter<'a> {
    /// Replace the heap-branch `value` in place with a call to a freshly
    /// synthesized tail helper `fn __branch_lift_N(p…) -> ty = <original branch>`.
    fn lift_bind_value(&mut self, ty: Ty, value: &mut IrExpr) {
        // 1. The branch is evaluated in the enclosing scope BEFORE the bind takes
        //    effect, so its free variables are exactly the enclosing locals it
        //    references (params, prior `let`s, loop binders). The bound var itself
        //    cannot appear (it is not yet defined). `bound = ∅`: nothing is already
        //    in scope that we want to exclude from the capture set.
        let bound: HashSet<VarId> = HashSet::new();
        let params: Vec<VarId> = free_vars(value, &bound); // deterministically sorted by VarId

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

        // 3. Build the helper's parameters from the captured free vars, KEEPING
        //    their VarIds so the body resolves against the shared var_table. Types
        //    come from the enclosing function's var_table (the checker's truth).
        let func_params: Vec<IrParam> = params
            .iter()
            .map(|&vid| {
                let info = self.vt.get(vid);
                IrParam {
                    var: vid,
                    ty: info.ty.clone(),
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
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Private,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![],
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

/// Heap-managed types (need refcount; lowered as `Ptr`/`Boxed`) vs `Copy` scalars.
///
/// This MIRRORS `almide_mir::lower::is_heap_ty` exactly. It is duplicated here
/// (not imported) because `almide-mir` depends on `almide-optimize`, so importing
/// it would create a dependency cycle. Keep the two definitions in sync: a scalar
/// that this predicate misclassifies as heap would lift a bind the renderer
/// already handles inline (harmless but wasteful); a heap type misclassified as
/// scalar would leave the original wall in place.
fn is_heap_ty(ty: &Ty) -> bool {
    !matches!(
        ty,
        Ty::Int
            | Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
            | Ty::UInt64
            | Ty::Float
            | Ty::Float32
            | Ty::Float64
            | Ty::Bool
            | Ty::Unit
            | Ty::Never
            | Ty::RawPtr
            | Ty::ConstParam { .. }
            | Ty::ConstValue { .. }
    )
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
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![],
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
        assert_eq!(helper.params[0].var, VarId(1));
        assert!(matches!(helper.body.kind, IrExprKind::If { .. }), "helper body is the verbatim branch");
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
}
