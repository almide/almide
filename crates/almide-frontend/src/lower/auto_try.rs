// ── Auto-? insertion (desugaring) ─────────────────────────────────────
//
// In effect fn bodies, the checker types user effect fn calls as
// Result[T, String] and auto_unwrap strips Result in let/var/match.
// This pass bridges the IR gap: Call.ty = Result[T, String] while
// Bind.ty = T. It wraps Result-typed calls in Try nodes, producing
// the T that bindings expect.
//
// Moved from codegen (pass_result_propagation.rs Phase 3) to lowering
// because this is desugaring, not code generation.

use std::collections::{HashMap, HashSet};
use almide_ir::*;
use almide_ir::visit::IrVisitor;
use almide_base::intern::sym;
use almide_base::intern::Sym;
use crate::types::{Ty, TypeConstructorId};

/// Statement-level context for target-directed Try coercion: the wrap/strip
/// decision for a binding or assignment depends on the TARGET's type (a
/// Result-typed target keeps the Result; anything else takes the `?`), so
/// the stmt walker needs the var table and the record field declarations.
struct TryCtx<'a> {
    var_table: &'a mut VarTable,
    record_fields: &'a HashMap<Sym, Vec<(Sym, Ty)>>,
    /// Vars bound with an EXPLICIT `Result[..]` annotation (collected during
    /// lowering). Only these binds keep the Result; an un-annotated bind of
    /// a `-> Result[..]`-declared effect fn has the same Bind.ty but takes
    /// the auto-`?` (#485).
    annotated_result_vars: &'a HashSet<VarId>,
    /// #558: qualified fn keys (`module.func`) whose FIRST param is
    /// Result/Option — that arg keeps its Result (no auto-?).
    first_arg_unwraps: &'a HashSet<Sym>,
    /// Usage-based skip set for THIS fn body: vars consumed as a Result
    /// (`match { ok/err }`, `== ok/err`, `??`). Collected once per body and
    /// consulted at EVERY Bind depth — VarIds are unique, so one flat set is
    /// sound. The old per-top-level-Block application missed a `let r = ...;
    /// match r { ok/err }` inside a match arm (porta mcp handle_tools_call).
    skip_unwrap: HashSet<u32>,
    /// ⊆ `skip_unwrap`: consumers that need the FULL Result unconditionally.
    force_skip: HashSet<u32>,
}

/// Insert auto-? (Try nodes) in all effect fn bodies of the program.
pub fn insert_auto_try(program: &mut IrProgram, annotated_result_vars: &HashSet<VarId>, first_arg_unwraps: &HashSet<Sym>) {
    // Record decls (root + modules) so FieldAssign can resolve a Named
    // target type to its field types. Decl names are canonical (qualified
    // `mod.Type` for user-module types), matching Ty::Named on var types.
    let mut record_fields: HashMap<Sym, Vec<(Sym, Ty)>> = HashMap::new();
    for td in program.type_decls.iter().chain(program.modules.iter().flat_map(|m| m.type_decls.iter())) {
        if let IrTypeDeclKind::Record { fields } = &td.kind {
            record_fields.insert(td.name, fields.iter().map(|f| (f.name, f.ty.clone())).collect());
        }
    }
    let IrProgram { functions, modules, var_table, .. } = program;
    for func in functions.iter_mut() {
        if func.is_effect && !func.is_test {
            let returns_result = func.ret_ty.is_result();
            let (skip_unwrap, force_skip) = collect_result_match_vars(&func.body);
            let mut ctx = TryCtx { var_table, record_fields: &record_fields, annotated_result_vars, first_arg_unwraps, skip_unwrap, force_skip };
            let ret_ty = func.ret_ty.clone();
            func.body = insert_try_body(std::mem::take(&mut func.body), returns_result, &ret_ty, &mut ctx);
        }
    }
    for module in modules.iter_mut() {
        let IrModule { functions, var_table, .. } = module;
        for func in functions.iter_mut() {
            if func.is_effect {
                let returns_result = func.ret_ty.is_result();
                let (skip_unwrap, force_skip) = collect_result_match_vars(&func.body);
                let mut ctx = TryCtx { var_table, record_fields: &record_fields, annotated_result_vars, first_arg_unwraps, skip_unwrap, force_skip };
                let ret_ty = func.ret_ty.clone();
                func.body = insert_try_body(std::mem::take(&mut func.body), returns_result, &ret_ty, &mut ctx);
            }
        }
    }
}

fn match_has_result_arms(arms: &[IrMatchArm]) -> bool {
    arms.iter().any(|arm| matches!(&arm.pattern, IrPattern::Ok { .. } | IrPattern::Err { .. }))
}

fn collect_result_match_vars(body: &IrExpr) -> (HashSet<u32>, HashSet<u32>) {
    let mut scan = ResultConsumerScan { vars: HashSet::new(), force: HashSet::new() };
    scan.visit_expr(body);
    (scan.vars, scan.force)
}

/// `vars` = every var that must stay a Result (the skip set). `force` ⊆ `vars` = the
/// subset consumed by a Result-`match { ok/err }` or `== ok/err`, which need the FULL
/// Result UNCONDITIONALLY (the consumer reads both arms). A `??`-only var goes to `vars`
/// but NOT `force`, so the #629 effect-`Result[Option,_]`-strip rule still applies to it.
///
/// Traversal is the exhaustive `IrVisitor` walk — a hand-rolled recursion here
/// missed value-carrying wrappers, so a consumer nested in one escaped the skip
/// set and its binding got auto-?'d out from under the match (porta mcp:
/// `ok(match parsed { ok/err })` sat behind a `ResultOk` → E0308 on native).
struct ResultConsumerScan {
    vars: HashSet<u32>,
    force: HashSet<u32>,
}

impl almide_ir::visit::IrVisitor for ResultConsumerScan {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Match { subject, arms } => {
                if match_has_result_arms(arms) {
                    if let IrExprKind::Var { id } = &subject.kind {
                        self.vars.insert(id.0);
                        self.force.insert(id.0);
                    }
                }
            }
            // `r ?? d` keeps `r` a Result (its OK type is the value of `??`). NOT a
            // `force` var: the #629 effect-Result[Option,_] strip rule still applies.
            IrExprKind::UnwrapOr { expr: inner, .. } => {
                if let IrExprKind::Var { id } = &inner.kind { self.vars.insert(id.0); }
            }
            // `r == ok(v)` / `r == err(e)` (and `!=`) read the full Result → `force`.
            IrExprKind::BinOp { op: BinOp::Eq | BinOp::Neq, left, right } => {
                let is_res = |e: &IrExpr| matches!(&e.kind,
                    IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. });
                if is_res(right) { if let IrExprKind::Var { id } = &left.kind { self.vars.insert(id.0); self.force.insert(id.0); } }
                if is_res(left) { if let IrExprKind::Var { id } = &right.kind { self.vars.insert(id.0); self.force.insert(id.0); } }
            }
            _ => {}
        }
        almide_ir::visit::walk_expr(self, expr);
    }
}

fn insert_try_body(expr: IrExpr, fn_returns_result: bool, ret_ty: &Ty, ctx: &mut TryCtx) -> IrExpr {
    if fn_returns_result {
        match expr.kind {
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let stmts = stmts.into_iter()
                    .map(|s| insert_try_stmt(s, ctx))
                    .collect();
                let tail = insert_try(*tail, false, ctx);
                let tail = strip_tail_try(tail);
                return IrExpr {
                    kind: IrExprKind::Block { stmts, expr: Some(Box::new(tail)) },
                    ty: expr.ty, span: expr.span, def_id: None,
                };
            }
            _ => {
                let result = insert_try(expr, false, ctx);
                return strip_tail_try(result);
            }
        }
    }
    // A NON-Result-declared effect fn (`fn_returns_result = false`) may still write an
    // EXPLICIT `ok(x)` sugar wrapper at its own tail (`fetch(p) -> List[String] = ok(["a",
    // "b"])`, `effect_if_branch_unwrap_test`'s `handler`'s `else { ok(["empty"]) }` arm) —
    // the checker types `ok(x): Result[T,_]` by its normal construction rule regardless of
    // the ENCLOSING function's declared return, so this Result wrapper survives `insert_try`
    // untouched (unlike the auto-INSERTED `Try` node `strip_tail_try` handles above, there is
    // no `Try` here to strip). But the function's WASM signature is built from its DECLARED
    // (non-Result) return type — a compiled body that still returns a REAL Result wrapper
    // object type-checks at the ABI level (both are opaque pointers) but points at the WRONG
    // block shape; any caller reading it as the declared raw type reads garbage (the v1 MIR
    // trust-spine DIAGNOSIS: `fetch`'s caller printed `0` instead of the real list). Collapse
    // it here, unconditionally, at every tail position `strip_tail_try` itself reaches
    // (Block/If/Match) — the SAME recursive shape, stripping `ResultOk` instead of `Try`.
    let result = insert_try(expr, false, ctx);
    // ONLY safe when the body never constructs an `err(...)` of its own anywhere (`fetch`'s
    // shape) — a fn whose body CAN take an Err branch (`validate`'s `if n>0 then ok(n) else
    // err("negative")`, this file's own no-regress guard: "must still type (and run) as a
    // Result") genuinely needs its callers to be able to read a real Result tag, so its
    // WASM repr must stay Result-shaped; stripping the `ok(n)` arm there would leave it
    // type-mismatched against the untouched `err(...)` sibling arm (a real regression this
    // was caught by — `validate`'s tail walled after an earlier, unconditional version of
    // this strip). `body_never_constructs_err` is a simple LOCAL scan (no transitive `!`-
    // propagation-through-callees analysis needed, unlike almide-mir's `compute_can_err` —
    // this only needs to know whether THIS body's own construction sites ever emit `err`).
    if body_never_constructs_err(&result) {
        strip_tail_result_ok_sugar(result, ret_ty)
    } else {
        result
    }
}

/// Does `body` construct an `err(...)` (`ResultErr`) ANYWHERE in its own AST? Does NOT follow
/// calls (a callee's own errors are its own concern) — purely a local scan, gating
/// `strip_tail_result_ok_sugar`'s safety (see its call site).
fn body_never_constructs_err(body: &IrExpr) -> bool {
    struct HasErr(bool);
    impl almide_ir::visit::IrVisitor for HasErr {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(&e.kind, IrExprKind::ResultErr { .. }) {
                self.0 = true;
            }
            if !self.0 {
                almide_ir::visit::walk_expr(self, e);
            }
        }
    }
    let mut scan = HasErr(false);
    scan.visit_expr(body);
    !scan.0
}

/// Collapse a redundant tail-position explicit `ok(x)` to `x`, for a NON-Result-declared
/// effect fn whose body writes it anyway (see `insert_try_body`'s call site for the full
/// rationale). Mirrors `strip_tail_try`'s exact recursive shape (Block-tail / both `If` arms /
/// every `Match` arm) so a wrapper nested inside a branch is reached too — `handler`'s
/// `if c then { match … } else { ok([...]) }` needs the `If`-arm case, not just the bare-tail
/// one `fetch`'s repro exercises. UNCONDITIONAL (no `inner.ty.is_result()` guard like
/// `strip_tail_try` — an EXPLICIT `ok(x)` node itself, at this position, is ALWAYS the
/// redundant sugar, never a real value the caller needs to keep, since the enclosing fn's own
/// declared type has no Result to begin with). Every level's `.ty` is FORCED to `ret_ty` (the
/// function's OWN declared type, not each subtree's own checker-assigned `Result[T,_]` type)
/// so a stripped `If`/`Match` doesn't disagree with its now-raw-typed children. `ResultErr` is
/// deliberately NOT stripped: a non-Result-declared fn's tail cannot type-check to `err(..)`
/// (no error slot in its declared/compiled ABI) — the checker rejects that shape before
/// lowering ever sees it, so no case for it is needed here.
fn strip_tail_result_ok_sugar(expr: IrExpr, ret_ty: &Ty) -> IrExpr {
    match expr.kind {
        IrExprKind::ResultOk { expr: inner } => {
            let mut inner = *inner;
            inner.ty = ret_ty.clone();
            inner
        }
        IrExprKind::Match { subject, arms } => {
            let arms = arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern, guard: arm.guard,
                body: strip_tail_result_ok_sugar(arm.body, ret_ty),
            }).collect();
            IrExpr { kind: IrExprKind::Match { subject, arms }, ty: ret_ty.clone(), span: expr.span, def_id: None }
        }
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond,
                then: Box::new(strip_tail_result_ok_sugar(*then, ret_ty)),
                else_: Box::new(strip_tail_result_ok_sugar(*else_, ret_ty)),
            },
            ty: ret_ty.clone(), span: expr.span, def_id: None,
        },
        IrExprKind::Block { stmts, expr: Some(tail) } => IrExpr {
            kind: IrExprKind::Block { stmts, expr: Some(Box::new(strip_tail_result_ok_sugar(*tail, ret_ty))) },
            ty: ret_ty.clone(), span: expr.span, def_id: None,
        },
        _ => expr,
    }
}

/// True when the value is an effect-lifted `Result[OK, _]` whose OK type is
/// itself a `Result` — i.e. a binding whose user-facing value really is a
/// Result that a `??` / `== ok` / Result-`match` consumer must keep. An
/// `Option`-fronted (or scalar) OK type means the consumer operates on the
/// auto-?'d inner value, so the skip must not apply. #629
fn value_ok_is_result(value: &IrExpr) -> bool {
    match &value.ty {
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].is_result(),
        _ => false,
    }
}

/// Strip one inserted top-level Try, restoring the Result-typed value.
/// Used wherever the TARGET keeps the Result (skip-set binding, declared
/// Result annotation, Result-typed assign target).
fn strip_top_try(expr: IrExpr) -> IrExpr {
    match expr.kind {
        IrExprKind::Try { expr: inner } if inner.ty.is_result() => *inner,
        _ => expr,
    }
}

/// Target-directed coercion: a Result-typed target keeps the Result (strip
/// the auto-inserted `?`); any other target keeps the Try that `insert_try`
/// added. `None` target type (unresolvable field) leaves the value as-is,
/// which means the common non-Result target behaves correctly.
fn coerce_to_target(value: IrExpr, target_is_result: bool) -> IrExpr {
    if target_is_result { strip_top_try(value) } else { value }
}

fn strip_tail_try(expr: IrExpr) -> IrExpr {
    match expr.kind {
        IrExprKind::Try { expr: inner } if inner.ty.is_result() => *inner,
        IrExprKind::Match { subject, arms } => {
            let arms = arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern, guard: arm.guard,
                body: strip_tail_try(arm.body),
            }).collect();
            IrExpr { kind: IrExprKind::Match { subject, arms }, ty: expr.ty, span: expr.span, def_id: None }
        }
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond,
                then: Box::new(strip_tail_try(*then)),
                else_: Box::new(strip_tail_try(*else_)),
            },
            ty: expr.ty, span: expr.span, def_id: None,
        },
        IrExprKind::Block { stmts, expr: Some(tail) } => IrExpr {
            kind: IrExprKind::Block { stmts, expr: Some(Box::new(strip_tail_try(*tail))) },
            ty: expr.ty, span: expr.span, def_id: None,
        },
        _ => expr,
    }
}

fn insert_try(expr: IrExpr, in_match_subject: bool, ctx: &mut TryCtx) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    let should_wrap = !in_match_subject && is_result_call(&expr);

    let kind = match expr.kind {
        IrExprKind::Block { stmts, expr: e } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| insert_try_stmt(s, ctx)).collect(),
            expr: e.map(|e| Box::new(insert_try(*e, false, ctx))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(insert_try(*cond, false, ctx)),
            then: Box::new(insert_try(*then, false, ctx)),
            else_: Box::new(insert_try(*else_, false, ctx)),
        },
        IrExprKind::Match { subject, arms } => {
            let arms_match_result = arms.iter().any(|a|
                matches!(&a.pattern, IrPattern::Ok { .. } | IrPattern::Err { .. }));
            IrExprKind::Match {
                subject: Box::new(insert_try(*subject, arms_match_result, ctx)),
                arms: arms.into_iter().map(|arm| IrMatchArm {
                    pattern: arm.pattern,
                    guard: arm.guard.map(|g| insert_try(g, false, ctx)),
                    body: insert_try(arm.body, false, ctx),
                }).collect(),
            }
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(insert_try(*left, false, ctx)),
            right: Box::new(insert_try(*right, false, ctx)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op,
            operand: Box::new(insert_try(*operand, false, ctx)),
        },
        IrExprKind::Call { target, args, type_args } => {
            // `result.*` / `option.*` functions consume their FIRST argument AS a
            // Result/Option (UFCS: `r.unwrap_or(d)` == `result.unwrap_or(r, d)`).
            // That arg must NOT be auto-?'d, exactly like the `??` (UnwrapOr) and
            // match-subject exceptions — otherwise `result.unwrap_or(int.parse(s), d)`
            // becomes `result.unwrap_or((int.parse(s))?, d)`, unwrapping the Result
            // the callee needs (E0308 at build; passes check/test). The `true` flag
            // suppresses only the top-level wrap of that arg, like a match subject.
            // #558: skip the auto-? on arg 0 when the callee's FIRST param is
            // Result/Option (derived from signatures), generalizing the old
            // hardcoded result/option module list — error.context/message,
            // testing.assert_ok and any user fn taking a Result first are now
            // covered. result/option stay as a fallback for intrinsic keys.
            let skip_first = match &target {
                CallTarget::Module { module, func, .. } => {
                    module.as_str() == "result" || module.as_str() == "option"
                        || ctx.first_arg_unwraps.contains(&sym(&format!("{}.{}", module.as_str(), func.as_str())))
                }
                // A USER fn whose first param is Result/Option (e.g.
                // `fn take(r: Result[..], ..)`) — its sig key is the bare name.
                CallTarget::Named { name } => ctx.first_arg_unwraps.contains(name),
                _ => false,
            };
            IrExprKind::Call {
                target,
                args: args.into_iter().enumerate()
                    .map(|(i, a)| insert_try(a, skip_first && i == 0, ctx))
                    .collect(),
                type_args,
            }
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body, lambda_id,
        },
        // #555: construction positions are TARGET-DIRECTED like the statement
        // arms — a Result-typed element/field must KEEP its Result (strip the
        // auto-`?`), not unwrap it. The unconditional wrap here made
        // `[step(), step()]: List[Result[..]]` and `Holder { r: step() }` lift
        // an effect call to Result and then auto-unwrap it, so native built
        // invalid Rust (E0308) while wasm ran and silently corrupted the value.
        IrExprKind::List { elements } => {
            let elem_is_result = match &ty {
                Ty::Applied(c, args) if *c == TypeConstructorId::List && args.len() == 1 => args[0].is_result(),
                _ => false,
            };
            IrExprKind::List {
                elements: elements.into_iter()
                    .map(|e| coerce_to_target(insert_try(e, false, ctx), elem_is_result))
                    .collect(),
            }
        }
        IrExprKind::Record { name, fields } => {
            let field_tys: HashMap<Sym, bool> = match &ty {
                Ty::Record { fields: fs } | Ty::OpenRecord { fields: fs } =>
                    fs.iter().map(|(n, t)| (*n, t.is_result())).collect(),
                Ty::Named(tn, _) => ctx.record_fields.get(tn)
                    .map(|fs| fs.iter().map(|(n, t)| (*n, t.is_result())).collect())
                    .unwrap_or_default(),
                _ => name.as_ref().and_then(|n| ctx.record_fields.get(n))
                    .map(|fs| fs.iter().map(|(n, t)| (*n, t.is_result())).collect())
                    .unwrap_or_default(),
            };
            IrExprKind::Record {
                name,
                fields: fields.into_iter()
                    .map(|(k, v)| {
                        let tgt = field_tys.get(&k).copied().unwrap_or(false);
                        (k, coerce_to_target(insert_try(v, false, ctx), tgt))
                    })
                    .collect(),
            }
        }
        IrExprKind::OptionSome { expr: inner } => IrExprKind::OptionSome {
            expr: Box::new(insert_try(*inner, false, ctx)),
        },
        IrExprKind::ResultOk { expr: inner } => IrExprKind::ResultOk {
            expr: Box::new(insert_try(*inner, false, ctx)),
        },
        IrExprKind::ResultErr { expr: inner } => IrExprKind::ResultErr {
            expr: Box::new(insert_try(*inner, false, ctx)),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(insert_try(*object, false, ctx)),
            field,
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(insert_try(*expr, false, ctx)),
            field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(insert_try(*iterable, false, ctx)),
            body: body.into_iter().map(|s| insert_try_stmt(s, ctx)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(insert_try(*cond, false, ctx)),
            body: body.into_iter().map(|s| insert_try_stmt(s, ctx)).collect(),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: insert_try(expr, false, ctx) },
                other => other,
            }).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| insert_try(e, false, ctx)).collect(),
        },
        IrExprKind::Tuple { elements } => {
            // #555: per-position target-directed coercion (Result-typed tuple
            // slot keeps its Result).
            let elem_results: Vec<bool> = match &ty {
                Ty::Tuple(tys) => tys.iter().map(|t| t.is_result()).collect(),
                _ => Vec::new(),
            };
            IrExprKind::Tuple {
                elements: elements.into_iter().enumerate()
                    .map(|(i, e)| coerce_to_target(insert_try(e, false, ctx), elem_results.get(i).copied().unwrap_or(false)))
                    .collect(),
            }
        }
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(insert_try(*base, false, ctx)),
            fields: fields.into_iter().map(|(k, v)| (k, insert_try(v, false, ctx))).collect(),
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(insert_try(*object, false, ctx)),
            index: Box::new(insert_try(*index, false, ctx)),
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(insert_try(*object, false, ctx)),
            index,
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone {
            expr: Box::new(insert_try(*expr, false, ctx)),
        },
        IrExprKind::Deref { expr } => IrExprKind::Deref {
            expr: Box::new(insert_try(*expr, false, ctx)),
        },
        IrExprKind::MapLiteral { entries } => {
            // #555: a Result-typed map VALUE keeps its Result.
            let val_is_result = match &ty {
                Ty::Applied(c, args) if *c == TypeConstructorId::Map && args.len() == 2 => args[1].is_result(),
                _ => false,
            };
            IrExprKind::MapLiteral {
                entries: entries.into_iter()
                    .map(|(k, v)| (insert_try(k, false, ctx), coerce_to_target(insert_try(v, false, ctx), val_is_result)))
                    .collect(),
            }
        }
        IrExprKind::Unwrap { expr: inner } => IrExprKind::Unwrap {
            expr: Box::new(insert_try(*inner, true, ctx)),
        },
        IrExprKind::Try { expr: inner } => IrExprKind::Try {
            expr: Box::new(insert_try(*inner, true, ctx)),
        },
        IrExprKind::ToOption { expr: inner } => IrExprKind::ToOption {
            expr: Box::new(insert_try(*inner, true, ctx)),
        },
        IrExprKind::UnwrapOr { expr: inner, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(insert_try(*inner, true, ctx)),
            fallback: Box::new(insert_try(*fallback, false, ctx)),
        },
        other => other,
    };

    // #717: an `if`/`match`/tail-`block` YIELDS its branch type. The checker
    // lifts an effect-fn-call branch to `Result[T, String]`, so the whole node
    // inherits that Result type — but `insert_try` above just auto-?'d those
    // branches down to `T`. Recompute the node type from the (now-unwrapped)
    // branches so a `let pick = if c then eff() else pure()` binding gets `T`,
    // not the stale `Result[T]` (which then emitted `Result<T>` = `?`-branches,
    // an E0308 type mismatch). Pure / genuinely-Result branches are unchanged:
    // their branch types already equal the original node type.
    let node_ty = match &kind {
        IrExprKind::If { then, else_, .. } if then.ty == else_.ty => then.ty.clone(),
        IrExprKind::Match { arms, .. }
            if !arms.is_empty() && arms.iter().all(|a| a.body.ty == arms[0].body.ty)
            => arms[0].body.ty.clone(),
        IrExprKind::Block { expr: Some(tail), .. } => tail.ty.clone(),
        _ => ty.clone(),
    };
    let mut result = IrExpr { kind, ty: node_ty, span, def_id: None };

    if should_wrap {
        let inner_ty = match &ty {
            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
            _ => ty,
        };
        result = IrExpr {
            kind: IrExprKind::Try { expr: Box::new(result) },
            ty: inner_ty,
            span, def_id: None,
        };
    }

    result
}

fn insert_try_stmt(stmt: IrStmt, ctx: &mut TryCtx) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => {
            // A binding consumed by `??` / `== ok(v)` / `match { ok/err }` is kept a
            // Result so that usage type-checks — BUT only when the binding's value
            // is genuinely Result-fronted at that consumer. An effect fn that
            // returns `Option[T]` is lifted to `Result[Option[T], String]`; binding
            // it and consuming with `??` is an OPTION-fallback, so the auto-? MUST
            // strip the effect `Result`, leaving `Option[T]` for `??`. Keeping the
            // `Result` there made native emit invalid Rust and wasm read the wrong
            // value (#629). So only honor the skip when the value's effect-Result
            // OK type is itself a Result (a real Result-fallback) or the binding is
            // an explicitly annotated Result (handled by `annotated_result_vars`).
            // A `match { ok/err }` / `== ok/err` consumer (force) needs the FULL Result
            // UNCONDITIONALLY — its OK type may be any type (base64 decode's `let bs =
            // decode_with(..)` is Result[List[Int],String], matched ok/err; the old
            // value_ok_is_result gate wrongly stripped it to List[Int], so the v1 MIR saw a
            // non-Result `match` and walled / native emitted invalid Rust). A `??`-only
            // consumer (skip, not force) keeps the #629 effect-Result[Option,_] strip rule.
            if ctx.force_skip.contains(&var.0)
                || (ctx.skip_unwrap.contains(&var.0)
                    && (ctx.annotated_result_vars.contains(&var) || value_ok_is_result(&value)))
            {
                let new_value = insert_try(value, false, ctx);
                let unwrapped = strip_top_try(new_value);
                IrStmtKind::Bind { var, mutability, ty, value: unwrapped }
            }
            // An ANNOTATED-Result binding (`let r: Result[T, E] = step()`)
            // keeps the Result: strip the Try that `insert_try` wrapped
            // around the call. Bind.ty alone cannot decide this — an
            // un-annotated `let v = boom()` where boom DECLARES `-> Result`
            // carries the identical Result Bind.ty but must auto-unwrap, so
            // the lowering records the annotated VarIds explicitly.
            else if ctx.annotated_result_vars.contains(&var) {
                let new_value = coerce_to_target(insert_try(value, false, ctx), true);
                IrStmtKind::Bind { var, mutability, ty, value: new_value }
            } else {
                let mut new_value = insert_try(value, false, ctx);
                // NOTE: a binding USED as a Result (`r ?? d`, `r == ok(v)`, `match r {
                // ok/err }`) is kept a Result by the usage-based skip set
                // (`collect_result_match_vars`), applied in `insert_try_stmt_with_skip`.
                // An earlier `if ty.is_result()` undo here was too broad — it also
                // un-did the auto-? for a plain `let v = effectCall()` whose inferred
                // type is the effect's `Result` (e.g. `effect fn boom() -> Result[..]`),
                // leaving `v` a Result and breaking error propagation. Removed.
                if !matches!(&new_value.kind, IrExprKind::Try { .. })
                    && is_result_value(&new_value)
                {
                    let inner_ty = match &new_value.ty {
                        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                        _ => new_value.ty.clone(),
                    };
                    let span = new_value.span;
                    new_value = IrExpr {
                        kind: IrExprKind::Try { expr: Box::new(new_value) },
                        ty: inner_ty,
                        span, def_id: None,
                    };
                }
                // The binding type IS the value's type. A top-level Try unwraps
                // Result→T; an `if`/`match`/block whose effect branches were just
                // auto-?'d likewise now yields T (its node type was recomputed
                // above, #717). Either way `new_value.ty` is authoritative — the
                // old `else { ty }` kept the stale effect-lifted `Result[T]` for
                // those non-Try forms, emitting `Result<T>` over `?`-branches.
                let _ = ty;
                let new_ty = new_value.ty.clone();
                // Keep the var table in sync with the unwrap: a later
                // `v = effectCall()` reads this entry to decide its own
                // wrap/strip, so a stale Result type would invert that rule.
                if ctx.var_table.get(var).ty != new_ty {
                    ctx.var_table.entries[var.0 as usize].ty = new_ty.clone();
                }
                IrStmtKind::Bind { var, mutability, ty: new_ty, value: new_value }
            }
        }
        IrStmtKind::Assign { var, value } => {
            // #485: target-directed — `x = step(x)` keeps the Try (`?`) iff
            // x is not itself Result-typed; `r = step(x)` with r: Result
            // strips it so the Result is stored intact.
            let target_is_result = ctx.var_table.get(var).ty.is_result();
            IrStmtKind::Assign {
                var, value: coerce_to_target(insert_try(value, false, ctx), target_is_result),
            }
        }
        IrStmtKind::IndexAssign { target, index, value } => {
            // `xs[i] = step(v)`: the value's target type is the list element.
            let elem_is_result = match &ctx.var_table.get(target).ty {
                Ty::Applied(TypeConstructorId::List, args) if !args.is_empty() => args[0].is_result(),
                _ => false,
            };
            IrStmtKind::IndexAssign {
                target,
                index: insert_try(index, false, ctx),
                value: coerce_to_target(insert_try(value, false, ctx), elem_is_result),
            }
        }
        IrStmtKind::MapInsert { target, key, value } => {
            // `m[k] = step(v)`: the value's target type is the map value type.
            let val_is_result = match &ctx.var_table.get(target).ty {
                Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => args[1].is_result(),
                _ => false,
            };
            IrStmtKind::MapInsert {
                target,
                key: insert_try(key, false, ctx),
                value: coerce_to_target(insert_try(value, false, ctx), val_is_result),
            }
        }
        IrStmtKind::FieldAssign { target, field, value } => {
            // `r.f = step(v)`: the value's target type is the declared field
            // type — structural Record directly, Named through the decl map.
            let field_is_result = match &ctx.var_table.get(target).ty {
                Ty::Record { fields } | Ty::OpenRecord { fields } =>
                    fields.iter().any(|(n, t)| *n == field && t.is_result()),
                Ty::Named(name, _) => ctx.record_fields.get(name)
                    .map_or(false, |fs| fs.iter().any(|(n, t)| *n == field && t.is_result())),
                _ => false,
            };
            IrStmtKind::FieldAssign {
                target, field,
                value: coerce_to_target(insert_try(value, false, ctx), field_is_result),
            }
        }
        IrStmtKind::Expr { expr } => IrStmtKind::Expr {
            expr: insert_try(expr, false, ctx),
        },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: insert_try(cond, false, ctx),
            else_: insert_try(else_, false, ctx),
        },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}

fn is_result_call(expr: &IrExpr) -> bool {
    expr.ty.is_result() && matches!(&expr.kind, IrExprKind::Call { .. })
}

fn is_result_value(expr: &IrExpr) -> bool {
    expr.ty.is_result() && matches!(&expr.kind,
        IrExprKind::Call { .. }
        | IrExprKind::ResultOk { .. }
        | IrExprKind::ResultErr { .. }
    )
}
