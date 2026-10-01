// Auto-? insertion, second half: the wrapper/unwrap arm group and the
// statement walkers. `include!`d by auto_try.rs (the 800-line file budget);
// it shares that module's scope and imports.

/// Single-operand wrappers and the unwrap family.
///
/// One group of `insert_try`'s arm table, arms verbatim and in source
/// order. `kind` is moved in, so a group that does not own the variant
/// hands it back as `Err` and the router tries the next group — the
/// dispatch order is exactly the original table's.
fn insert_try_wrapper(kind: IrExprKind, ty: &Ty, ctx: &mut TryCtx) -> Result<IrExprKind, IrExprKind> {
    Ok(match kind {
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
                    .map(|(k, v)| (insert_try(k, false, ctx), insert_try(v, val_is_result, ctx)))
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
        other => return Err(other),
    })
}

fn insert_try_stmt(stmt: IrStmt, ctx: &mut TryCtx) -> IrStmt {
    let kind = insert_try_stmt_bind(stmt.kind, ctx)
        .or_else(|k| insert_try_stmt_assign(k, ctx))
        .unwrap_or_else(|k| k);
    IrStmt { kind, span: stmt.span }
}

/// Binding forms. These are target-directed: a `let x: Result[..] = eff()`
/// keeps its Result, so the auto-`?` is stripped from the value.
///
/// One group of `insert_try_stmt`'s arm table, arms verbatim and in source
/// order. `kind` is moved in, so a group that does not own the variant
/// hands it back as `Err` for the next group to try.
fn insert_try_stmt_bind(kind: IrStmtKind, ctx: &mut TryCtx) -> Result<IrStmtKind, IrStmtKind> {
    Ok(match kind {
        IrStmtKind::Bind { var, mutability, ty, value } =>
            insert_try_bind(var, mutability, ty, value, ctx),
        IrStmtKind::Assign { var, value } => {
            // #485: target-directed — `x = step(x)` keeps the Try (`?`) iff
            // x is not itself Result-typed; `r = step(x)` with r: Result
            // strips it so the Result is stored intact.
            let target_is_result = ctx.var_table.get(var).ty.is_result();
            IrStmtKind::Assign {
                var, value: insert_try(value, target_is_result, ctx),
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
                value: insert_try(value, elem_is_result, ctx),
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
                value: insert_try(value, val_is_result, ctx),
            }
        }
        other => return Err(other),
    })
}


/// Auto-`?` a `let` / `var` binding.
///
/// Three outcomes, and which one applies is decided by how the binding is USED,
/// not by its own type: a binding consumed as a Result keeps the Result, an
/// explicitly annotated Result keeps it too, and everything else is unwrapped so
/// errors propagate. `Bind.ty` alone cannot separate the second case from the
/// third — an un-annotated `let v = boom()` where `boom` DECLARES `-> Result`
/// carries an identical `Bind.ty` but must auto-unwrap — which is why lowering
/// records the annotated `VarId`s explicitly.
fn insert_try_bind(
    var: VarId,
    mutability: Mutability,
    ty: Ty,
    value: IrExpr,
    ctx: &mut TryCtx,
) -> IrStmtKind {
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
    // An ANNOTATED-Result binding (`let r: Result[T, E] = step()`) keeps the
    // Result too. Bind.ty alone cannot decide this — an un-annotated
    // `let v = boom()` where boom DECLARES `-> Result` carries the identical
    // Result Bind.ty but must auto-unwrap, so the lowering records the
    // annotated VarIds explicitly. Either way the value is lowered as kept:
    // no `?` at its top, nor on its branch leaves (#2632).
    let keeps = ctx.force_skip.contains(&var.0)
        || ctx.annotated_result_vars.contains(&var)
        || (ctx.skip_unwrap.contains(&var.0) && value_ok_is_result(&value));
    if keeps {
        let kept = insert_try(value, true, ctx);
        IrStmtKind::Bind { var, mutability, ty, value: kept }
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

/// Assignment and the remaining statement forms.
///
/// One group of `insert_try_stmt`'s arm table, arms verbatim and in source
/// order. `kind` is moved in, so a group that does not own the variant
/// hands it back as `Err` for the next group to try.
fn insert_try_stmt_assign(kind: IrStmtKind, ctx: &mut TryCtx) -> Result<IrStmtKind, IrStmtKind> {
    Ok(match kind {
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
                value: insert_try(value, field_is_result, ctx),
            }
        }
        IrStmtKind::Expr { expr } => IrStmtKind::Expr {
            expr: insert_try(expr, false, ctx),
        },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: insert_try(cond, false, ctx),
            else_: insert_try(else_, false, ctx),
        },
        other => return Err(other),
    })
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
