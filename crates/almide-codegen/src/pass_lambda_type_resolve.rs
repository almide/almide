//! Lambda Type Resolution pass (top-down).
//!
//! Resolves lambda parameter types from call-site context before closure
//! conversion. After this pass, every lambda parameter reachable from a
//! typed call site (list.map, list.filter, etc.) has a concrete type in
//! both its IR annotation and the VarTable.
//!
//! This is the "first half" of a two-pass design inspired by OCaml's
//! flambda: types are propagated top-down, then closure conversion runs
//! bottom-up on fully-typed IR.
//!
//! Postcondition: all Lambda param VarTable entries that are transitively
//! reachable from a typed list-callback call are `!is_unresolved_structural()`.

use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Postcondition, Target};

#[derive(Debug)]
pub struct LambdaTypeResolvePass;

impl NanoPass for LambdaTypeResolvePass {
    fn name(&self) -> &str { "LambdaTypeResolve" }

    fn targets(&self) -> Option<Vec<Target>> {
        // Both WASM and Rust targets. Historically Rust avoided the
        // pass because `@inline_rust` templates carried call-site
        // type info at expansion time. Once closure-bearing list fns
        // migrated to `@intrinsic` + `IrExprKind::RuntimeCall`, the
        // Rust walker no longer has the stdlib call signature to
        // propagate element types into lambda params; the lambda's
        // `c: String` stays `TypeVar` and `MatchSubjectPass` fails to
        // recognise the subject type.
        Some(vec![Target::Rust, Target::Wgsl])
    }

    fn postconditions(&self) -> Vec<Postcondition> {
        vec![Postcondition::Custom(check_lambda_params_resolved)]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let IrProgram { functions, top_lets, modules, var_table, .. } = &mut program;
        for func in functions.iter_mut() {
            resolve_expr(&mut func.body, var_table);
        }
        for tl in top_lets.iter_mut() {
            resolve_expr(&mut tl.value, var_table);
        }
        for module in modules.iter_mut() {
            for func in module.functions.iter_mut() {
                resolve_expr(&mut func.body, var_table);
            }
            for tl in module.top_lets.iter_mut() {
                resolve_expr(&mut tl.value, var_table);
            }
        }
        PassResult { program, changed: true }
    }
}

// ── Postcondition check ─────────────────────────────────────────────

fn check_lambda_params_resolved(program: &IrProgram) -> Vec<String> {
    let mut violations = Vec::new();
    struct Checker<'a> { vt: &'a VarTable, violations: &'a mut Vec<String> }
    impl<'a> IrVisitor for Checker<'a> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Lambda { params, .. } = &expr.kind {
                for (vid, pty) in params {
                    let vt_ty = &self.vt.get(*vid).ty;
                    if pty.is_unresolved_structural() && vt_ty.is_unresolved_structural() {
                        self.violations.push(format!(
                            "Lambda param {:?} still unresolved: ir={:?} vt={:?}",
                            vid, pty, vt_ty
                        ));
                    }
                }
            }
            walk_expr(self, expr);
        }
    }
    let mut c = Checker { vt: &program.var_table, violations: &mut violations };
    for func in &program.functions { c.visit_expr(&func.body); }
    // Note: module-level checks would need module.var_table; skip for now
    // as the pass runs per-module and violations surface at WASM emit time.
    violations
}

// ── Top-down expression walker ──────────────────────────────────────
//
// Key invariant: at each Call node, we resolve lambda param types FIRST,
// then recurse into children. This means outer lambdas' params are
// resolved before inner lambdas are visited.

// ── resolve_expr arm extraction (cog>100 decomposition, pattern 2) ──
//
// 1:1 text-moves of the two largest `resolve_expr` match arms. Each
// re-narrows `expr.kind` via `let-else` and mutates `expr`/`vt` exactly as
// the inline arm did — no behavior change.

fn resolve_expr_call(expr: &mut IrExpr, vt: &mut VarTable) {
    let IrExprKind::Call { target, args, .. } = &mut expr.kind else { unreachable!() };
    // 1. Resolve lambda params from call-site list element type
    resolve_call_lambdas(target, args, vt);
    // 2. Recurse into target
    match target {
        CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => {
            resolve_expr(object, vt);
        }
        _ => {}
    }
    // 3. Recurse into args (including lambda bodies)
    for a in args.iter_mut() {
        resolve_expr(a, vt);
    }
    // 4. Update Call's own return type from resolved args for a
    //    few stdlib list ops whose generic signature left
    //    TypeVars unsubstituted. Without this, a `let zipped =
    //    list.zip(filter, spectrum)` inside a closure keeps
    //    `List[Tuple[TypeVar, Float]]` and the fold callback
    //    that follows fails to resolve `pair: (Float, Float)`.
    if expr.ty.has_unresolved_deep() {
        if let Some(new_ty) = compute_stdlib_call_ret(target, args, vt) {
            expr.ty = new_ty;
        }
    }
}

fn resolve_expr_lambda(expr: &mut IrExpr, vt: &mut VarTable) {
    let IrExprKind::Lambda { params, .. } = &mut expr.kind else { unreachable!() };
    // Sync param types: VarTable ↔ IR annotation (concrete wins)
    sync_lambda_param_types(params, vt);
    // Update Ty::Fn wrapper to match resolved params
    refresh_lambda_fn_ty(expr, vt);
    // Recurse into body (params are now resolved for inner lambdas to see)
    if let IrExprKind::Lambda { body, .. } = &mut expr.kind {
        resolve_expr(body, vt);
    }
    // Bottom-up: infer still-Unknown params from body usage
    if let IrExprKind::Lambda { params, body, .. } = &mut expr.kind {
        infer_lambda_params_from_body(params, body, vt);
        refresh_lambda_fn_ty(expr, vt);
    }
}

/// Param-sync phase of `resolve_expr_lambda`, extracted verbatim (cog>30
/// decomposition, sequential-phase pattern). Syncs `VarTable` ↔ IR
/// annotation (concrete wins) — uses `.has_unresolved_deep()` to catch
/// `Applied(List, [TypeVar(A)])`.
fn sync_lambda_param_types(params: &mut [(VarId, Ty)], vt: &mut VarTable) {
    for (vid, pty) in params.iter_mut() {
        if (vid.0 as usize) < vt.len() {
            let vt_ty = vt.get(*vid).ty.clone();
            if pty.has_unresolved_deep() && !(vt_ty).has_unresolved_deep() {
                *pty = vt_ty;
            } else if !pty.has_unresolved_deep() && (vt_ty).has_unresolved_deep() {
                vt.entries[vid.0 as usize].ty = pty.clone();
            }
        }
    }
}

/// Bottom-up param-inference phase of `resolve_expr_lambda`, extracted
/// verbatim (cog>30 decomposition) — infer still-Unknown params from body
/// usage.
fn infer_lambda_params_from_body(params: &mut [(VarId, Ty)], body: &IrExpr, vt: &mut VarTable) {
    for (vid, pty) in params.iter_mut() {
        if pty.has_unresolved_deep() {
            if let Some(inferred) = super::pass_concretize_types::infer_var_type_from_body(body, *vid) {
                *pty = inferred.clone();
                vt.entries[vid.0 as usize].ty = inferred;
            }
        }
    }
}

/// `IrExprKind::RuntimeCall` case of `resolve_expr`, extracted verbatim
/// (cog>30 decomposition, pattern 2: uniform match arms, mirrors the
/// `lower_expr`/`infer_expr_inner` extraction shape).
fn resolve_expr_runtime_call(expr: &mut IrExpr, vt: &mut VarTable) {
    let IrExprKind::RuntimeCall { symbol, args } = &mut expr.kind else { unreachable!() };
    for a in args.iter_mut() {
        resolve_expr(a, vt);
    }
    if expr.ty.has_unresolved_deep() {
        let synthetic = CallTarget::Named { name: *symbol };
        if let Some(new_ty) = compute_stdlib_call_ret(&synthetic, args, vt) {
            expr.ty = new_ty;
        }
    }
}

/// `IrExprKind::Block` case of `resolve_expr`, extracted verbatim.
fn resolve_expr_block(expr: &mut IrExpr, vt: &mut VarTable) {
    let IrExprKind::Block { stmts, expr: tail } = &mut expr.kind else { unreachable!() };
    for s in stmts.iter_mut() { resolve_stmt(s, vt); }
    if let Some(e) = tail { resolve_expr(e, vt); }
}

/// `IrExprKind::Match` case of `resolve_expr`, extracted verbatim.
fn resolve_expr_match(expr: &mut IrExpr, vt: &mut VarTable) {
    let IrExprKind::Match { subject, arms } = &mut expr.kind else { unreachable!() };
    resolve_expr(subject, vt);
    for arm in arms.iter_mut() {
        if let Some(g) = &mut arm.guard { resolve_expr(g, vt); }
        resolve_expr(&mut arm.body, vt);
    }
}

/// `IrExprKind::ForIn` case of `resolve_expr`, extracted verbatim.
fn resolve_expr_for_in(expr: &mut IrExpr, vt: &mut VarTable) {
    let IrExprKind::ForIn { iterable, body, .. } = &mut expr.kind else { unreachable!() };
    resolve_expr(iterable, vt);
    for s in body.iter_mut() { resolve_stmt(s, vt); }
}

/// `IrExprKind::While` case of `resolve_expr`, extracted verbatim.
fn resolve_expr_while(expr: &mut IrExpr, vt: &mut VarTable) {
    let IrExprKind::While { cond, body } = &mut expr.kind else { unreachable!() };
    resolve_expr(cond, vt);
    for s in body.iter_mut() { resolve_stmt(s, vt); }
}

/// Resolve a `TupleIndex` node's result type from its object's (now
/// bottom-up-resolved) Tuple type. Returns `Some(new_ty)` if resolved (the
/// caller assigns it to `expr.ty` itself — this only reads `object` and
/// `current_ty`, no `&mut IrExpr` needed). Extracted from
/// `sync_resolved_expr_ty` (cog>30 decomposition, second round).
fn resolve_tuple_index_result_ty(object: &IrExpr, index: usize, current_ty: &Ty, vt: &VarTable) -> Option<Ty> {
    // Resolve from object's Tuple type (object.ty may have been updated above)
    let obj_ty = if let Ty::Tuple(_) = &object.ty {
        &object.ty
    } else if let IrExprKind::Var { id } = &object.kind {
        if (id.0 as usize) < vt.len() { &vt.get(*id).ty } else { &object.ty }
    } else {
        &object.ty
    };
    if let Ty::Tuple(elems) = obj_ty {
        if let Some(elem_ty) = elems.get(index) {
            if !elem_ty.is_unresolved_structural() && current_ty.is_unresolved_structural() {
                return Some(elem_ty.clone());
            }
        }
    }
    None
}

/// Post-visit: sync expr.ty from VarTable for Var nodes, and resolve
/// TupleIndex result type from the object's Tuple type / propagate BinOp
/// operand types. Extracted from `resolve_expr`'s trailing sync match
/// (cog>30 decomposition).
fn sync_resolved_expr_ty(expr: &mut IrExpr, vt: &VarTable) {
    match &expr.kind {
        IrExprKind::Var { id } => {
            if expr.ty.is_unresolved_structural() && (id.0 as usize) < vt.len() {
                let vt_ty = &vt.get(*id).ty;
                if !vt_ty.is_unresolved_structural() {
                    expr.ty = vt_ty.clone();
                }
            }
        }
        IrExprKind::TupleIndex { object, index } => {
            if let Some(new_ty) = resolve_tuple_index_result_ty(object, *index, &expr.ty, vt) {
                expr.ty = new_ty;
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            // If BinOp result is unresolved but operands are resolved, propagate
            if expr.ty.is_unresolved_structural() {
                if !left.ty.is_unresolved_structural() {
                    expr.ty = left.ty.clone();
                } else if !right.ty.is_unresolved_structural() {
                    expr.ty = right.ty.clone();
                }
            }
        }
        _ => {}
    }
}

/// The nodes whose resolution needs more than "visit the children": a call's
/// signature, a lambda's params, or a scope's bindings.
fn resolve_expr_scoped(expr: &mut IrExpr, vt: &mut VarTable) {
    match &expr.kind {
        IrExprKind::Call { .. } => resolve_expr_call(expr, vt),
        IrExprKind::RuntimeCall { .. } => resolve_expr_runtime_call(expr, vt),
        IrExprKind::Lambda { .. } => resolve_expr_lambda(expr, vt),
        IrExprKind::Block { .. } => resolve_expr_block(expr, vt),
        IrExprKind::Match { .. } => resolve_expr_match(expr, vt),
        IrExprKind::ForIn { .. } => resolve_expr_for_in(expr, vt),
        IrExprKind::While { .. } => resolve_expr_while(expr, vt),
        _ => unreachable!("resolve_expr_scoped called on a non-scoped kind"),
    }
}

/// Each element of a flat child sequence (`List` / `Tuple`).
fn resolve_each(xs: &mut [IrExpr], vt: &mut VarTable) {
    for e in xs.iter_mut() {
        resolve_expr(e, vt);
    }
}

/// Each name-tagged child (`Record` / `SpreadRecord` fields) — the name plays
/// no part in the walk.
fn resolve_fields(fields: &mut [(almide_base::intern::Sym, IrExpr)], vt: &mut VarTable) {
    for (_, e) in fields.iter_mut() {
        resolve_expr(e, vt);
    }
}

/// Each map entry's key, then its value.
fn resolve_map_entries(entries: &mut [(IrExpr, IrExpr)], vt: &mut VarTable) {
    for (k, v) in entries.iter_mut() {
        resolve_expr(k, vt);
        resolve_expr(v, vt);
    }
}

/// Each interpolated sub-expression of a `StringInterp`.
fn resolve_interp_parts(parts: &mut [IrStringPart], vt: &mut VarTable) {
    for p in parts.iter_mut() {
        if let IrStringPart::Expr { expr: e } = p {
            resolve_expr(e, vt);
        }
    }
}

fn resolve_expr(expr: &mut IrExpr, vt: &mut VarTable) {
    // The nodes that carry a lambda, a scope, or a call signature each have
    // their own resolver; the rest only need their children visited, grouped
    // here by child shape.
    if matches!(
        expr.kind,
        IrExprKind::Call { .. } | IrExprKind::RuntimeCall { .. } | IrExprKind::Lambda { .. }
            | IrExprKind::Block { .. } | IrExprKind::Match { .. }
            | IrExprKind::ForIn { .. } | IrExprKind::While { .. }
    ) {
        resolve_expr_scoped(expr, vt);
        return;
    }
    match &mut expr.kind {
        IrExprKind::If { cond, then, else_ } => {
            resolve_expr(cond, vt);
            resolve_expr(then, vt);
            resolve_expr(else_, vt);
        }
        // ── One child ──
        IrExprKind::UnOp { operand: e, .. }
        | IrExprKind::Member { object: e, .. } | IrExprKind::TupleIndex { object: e, .. }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e } => resolve_expr(e, vt),

        // ── Two children, left to right ──
        IrExprKind::BinOp { left: a, right: b, .. }
        | IrExprKind::IndexAccess { object: a, index: b }
        | IrExprKind::MapAccess { object: a, key: b }
        | IrExprKind::Range { start: a, end: b, .. } => {
            resolve_expr(a, vt);
            resolve_expr(b, vt);
        }

        // ── A flat sequence of children ──
        IrExprKind::List { elements: xs } | IrExprKind::Tuple { elements: xs } => {
            resolve_each(xs, vt)
        }

        // ── Name-tagged children ──
        IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => {
            resolve_fields(fields, vt)
        }

        // ── Shapes with their own traversal ──
        IrExprKind::MapLiteral { entries } => resolve_map_entries(entries, vt),
        IrExprKind::StringInterp { parts } => resolve_interp_parts(parts, vt),
        // Handled by `resolve_expr_scoped`; never routed here.
        IrExprKind::Call { .. } | IrExprKind::RuntimeCall { .. } | IrExprKind::Lambda { .. }
        | IrExprKind::Block { .. } | IrExprKind::Match { .. }
        | IrExprKind::ForIn { .. } | IrExprKind::While { .. } => {
            unreachable!("resolve_expr reached a scoped kind after the guard above")
        }

        // Leaf / non-type-bearing kinds: nothing to descend into for the
        // top-down type propagation. Listed explicitly so a new IrExprKind
        // is a compile error here, never a silently-dropped subtree.
        IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::Unit
        | IrExprKind::Var { .. }
        | IrExprKind::FnRef { .. }
        | IrExprKind::Fan { .. }
        | IrExprKind::Break
        | IrExprKind::Continue
        | IrExprKind::TailCall { .. }
        | IrExprKind::EmptyMap
        | IrExprKind::OptionNone
        | IrExprKind::Unwrap { .. }
        | IrExprKind::UnwrapOr { .. }
        | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. }
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
        | IrExprKind::Todo { .. } => {}
    }

    // Post-visit: sync expr.ty from VarTable for Var nodes,
    // and resolve TupleIndex result type from the object's Tuple type.
    sync_resolved_expr_ty(expr, vt);
}

fn resolve_stmt(stmt: &mut IrStmt, vt: &mut VarTable) {
    match &mut stmt.kind {
        IrStmtKind::Bind { var, ty, value, .. } => {
            resolve_expr(value, vt);
            // Propagate a resolved RHS type up into the Bind's declared
            // type AND the VarTable entry for the bound var. Without
            // this, a `let zipped = list.zip(xs, ys)` inside a closure
            // still carries `TypeVar` for zipped's type at the fold
            // call-site that follows, because LTR resolved zip's
            // result but never pushed the type forward through the
            // Bind boundary.
            if ty.has_unresolved_deep() && !value.ty.has_unresolved_deep() {
                *ty = value.ty.clone();
            }
            if (var.0 as usize) < vt.len() {
                let vt_ty = vt.get(*var).ty.clone();
                if vt_ty.has_unresolved_deep() && !value.ty.has_unresolved_deep() {
                    vt.entries[var.0 as usize].ty = value.ty.clone();
                }
            }
        }
        IrStmtKind::BindDestructure { value, .. } => resolve_expr(value, vt),
        IrStmtKind::Assign { value, .. } => resolve_expr(value, vt),
        IrStmtKind::IndexAssign { index, value, .. } => {
            resolve_expr(index, vt); resolve_expr(value, vt);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            resolve_expr(key, vt); resolve_expr(value, vt);
        }
        IrStmtKind::FieldAssign { value, .. } => resolve_expr(value, vt),
        IrStmtKind::Expr { expr } => resolve_expr(expr, vt),
        IrStmtKind::Guard { cond, else_ } => {
            resolve_expr(cond, vt); resolve_expr(else_, vt);
        }
        // Statements with no type to propagate into (or whose IrExpr
        // children — ListSwap/ListReverse/ListRotateLeft/ListCopySlice
        // operands — the original catch-all intentionally left untouched).
        // Listed explicitly so a new IrStmtKind is a compile error here.
        IrStmtKind::Comment { .. }
        | IrStmtKind::ListCopySlice { .. }
        | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. }
        | IrStmtKind::ListSwap { .. }
        | IrStmtKind::RcDec { .. }
        | IrStmtKind::RcInc { .. } => {}
    }
}

// ── Call-site lambda param resolution ───────────────────────────────
//
// For `list.map(xs, (x) => ...)`, resolve `x` from the element type of `xs`.
// Also handles list.zip, list.fold accumulator, option.{map,flat_map,filter},
// result.{map,flat_map,filter,map_err,or_else,unwrap_or_else}, etc.

/// List callback methods whose lambda's FIRST param is the element type.
/// Form: `method(xs, f)` where `f: (elem) -> ?`.
const LIST_ELEM_FIRST_METHODS: &[&str] = &[
    "map", "filter", "filter_map", "flat_map",
    "find", "any", "all", "each", "count", "partition",
    "sort_by", "group_by", "unique_by", "take_while", "drop_while",
    "min_by", "max_by", "chunk_by", "dedup_by",
];

/// Option callback methods whose lambda receives the inner type T.
/// Form: `method(o: Option[T], f: (T) -> ?)`.
const OPTION_INNER_METHODS: &[&str] = &[
    "map", "flat_map", "filter",
];

/// Result callback methods whose lambda receives the OK type A.
/// Form: `method(r: Result[A, E], f: (A) -> ?)`.
const RESULT_OK_METHODS: &[&str] = &[
    "map", "flat_map", "filter",
];

/// Result callback methods whose lambda receives the ERR type E.
/// Form: `method(r: Result[A, E], f: (E) -> ?)`.
const RESULT_ERR_METHODS: &[&str] = &[
    "map_err", "or_else", "unwrap_or_else",
];

/// List callback methods whose lambda's SECOND param is the element type.
/// Form: `method(xs, init, f)` where `f: (acc, elem) -> acc`.
const LIST_ELEM_SECOND_METHODS: &[&str] = &[
    "fold", "scan",
];

/// List callback methods where elem is BOTH params (reduce: (elem, elem) -> elem).
const LIST_ELEM_BOTH_METHODS: &[&str] = &["reduce"];

/// Which position(s) of an Option/Result/collection's type args a lambda's
/// param(s) should be resolved from.
enum ElemSource { ListElem, OptionInner, ResultOk, ResultErr }

include!("pass_lambda_type_lookup.rs");
