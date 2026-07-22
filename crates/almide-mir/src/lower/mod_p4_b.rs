
/// `${Option[T]}` interp routing per payload type. Verbatim text move (#781).
///
/// A static lookup table with mutually-exclusive `Ty` arms (no `Ty` value
/// can match two of them) — grouped into per-category sub-tables tried in
/// sequence via `.or_else()`, each with its OWN `_ => None` fallback. Since
/// the arms are mutually exclusive by construction, at most one group ever
/// returns `Some`, so trying them in any order is behaviorally identical to
/// the single flat match; the final `.unwrap_or(..)` is the original `_`
/// fallback.
fn interp_option_to_string(inner: &Ty) -> (&'static str, &'static str) {
    interp_option_to_string_scalar(inner)
        .or_else(|| interp_option_to_string_list(inner))
        .or_else(|| interp_option_to_string_simple_applied(inner))
        .or_else(|| interp_option_to_string_nested(inner))
        .unwrap_or(("option", "to_string_x"))
}

fn interp_option_to_string_scalar(inner: &Ty) -> Option<(&'static str, &'static str)> {
    match inner {
        Ty::Int => Some(("option", "to_string")),
        Ty::String => Some(("option", "to_string_s")),
        Ty::Float => Some(("option", "to_string_f")),
        Ty::Bool => Some(("option", "to_string_b")),
        _ => None,
    }
}

/// `${Option[List[Int]]}` → `some([1, 2, 3])` / `none` — the inner list renders like
/// `${list}`, wrapped in `some(…)`. A deeper element routes to the UNLINKED `_x`.
fn interp_option_to_string_list(inner: &Ty) -> Option<(&'static str, &'static str)> {
    use almide_lang::types::constructor::TypeConstructorId;
    match inner {
        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::Int) => {
            Some(("option", "to_string_li"))
        }
        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::String) => {
            Some(("option", "to_string_ls"))
        }
        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::Bool) => {
            Some(("option", "to_string_lb"))
        }
        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::Float) => {
            Some(("option", "to_string_lf"))
        }
        _ => None,
    }
}

fn interp_option_to_string_simple_applied(inner: &Ty) -> Option<(&'static str, &'static str)> {
    use almide_lang::types::constructor::TypeConstructorId;
    match inner {
        Ty::Applied(TypeConstructorId::Option, e) if e.len() == 1 && matches!(e[0], Ty::Int) => {
            Some(("option", "to_string_oi"))
        }
        Ty::Applied(TypeConstructorId::Option, e) if e.len() == 1 && matches!(e[0], Ty::Bool) => {
            Some(("option", "to_string_ob"))
        }
        Ty::Applied(TypeConstructorId::Option, e) if e.len() == 1 && matches!(e[0], Ty::String) => {
            Some(("option", "to_string_os"))
        }
        Ty::Applied(TypeConstructorId::Map, e)
            if e.len() == 2 && matches!(e[0], Ty::String) && matches!(e[1], Ty::Int) =>
        {
            Some(("option", "to_string_msi"))
        }
        Ty::Applied(TypeConstructorId::Result, e)
            if e.len() == 2 && matches!(e[0], Ty::Int) && matches!(e[1], Ty::String) =>
        {
            Some(("option", "to_string_ri"))
        }
        Ty::Applied(TypeConstructorId::Result, e)
            if e.len() == 2 && matches!(e[0], Ty::String) && matches!(e[1], Ty::String) =>
        {
            Some(("option", "to_string_rs"))
        }
        _ => None,
    }
}

fn interp_option_to_string_nested(inner: &Ty) -> Option<(&'static str, &'static str)> {
    use almide_lang::types::constructor::TypeConstructorId;
    match inner {
        Ty::Applied(TypeConstructorId::Option, e)
            if e.len() == 1
                && matches!(&e[0], Ty::Applied(TypeConstructorId::Option, e2)
                    if e2.len() == 1 && matches!(e2[0], Ty::Int)) =>
        {
            Some(("option", "to_string_ooi"))
        }
        Ty::Applied(TypeConstructorId::Option, e)
            if e.len() == 1
                && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                    if e2.len() == 1 && matches!(e2[0], Ty::Int)) =>
        {
            Some(("option", "to_string_ooli"))
        }
        Ty::Applied(TypeConstructorId::Result, e)
            if e.len() == 2
                && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                    if e2.len() == 1 && matches!(e2[0], Ty::Int))
                && matches!(e[1], Ty::String) =>
        {
            Some(("option", "to_string_rli"))
        }
        _ => None,
    }
}

/// `${Result[T, E]}` interp routing per (ok, err) pair. Verbatim text move (#781).
/// Same mutually-exclusive-arms, grouped-`.or_else()` split as
/// [`interp_option_to_string`] above, applied to the `(ok, err)` pair table.
fn interp_result_to_string(ok: &Ty, err: &Ty) -> (&'static str, &'static str) {
    interp_result_to_string_scalar(ok, err)
        .or_else(|| interp_result_to_string_list(ok, err))
        .or_else(|| interp_result_to_string_simple_applied(ok, err))
        .or_else(|| interp_result_to_string_nested(ok, err))
        .unwrap_or(("result", "to_string_x"))
}

fn interp_result_to_string_scalar(ok: &Ty, err: &Ty) -> Option<(&'static str, &'static str)> {
    match (ok, err) {
        (Ty::Int, Ty::String) => Some(("result", "to_string")),
        (Ty::String, Ty::String) => Some(("result", "to_string_ss")),
        (Ty::Bool, Ty::String) => Some(("result", "to_string_b")),
        (Ty::Float, Ty::String) => Some(("result", "to_string_f")),
        _ => None,
    }
}

/// `${Result[List[Int], String]}` → `ok([1, 2, 3])` / `err("<quoted>")`.
fn interp_result_to_string_list(ok: &Ty, err: &Ty) -> Option<(&'static str, &'static str)> {
    use almide_lang::types::constructor::TypeConstructorId;
    match (ok, err) {
        (Ty::Applied(TypeConstructorId::List, e), Ty::String)
            if e.len() == 1 && matches!(e[0], Ty::Int) =>
        {
            Some(("result", "to_string_li"))
        }
        (Ty::Applied(TypeConstructorId::List, e), Ty::String)
            if e.len() == 1 && matches!(e[0], Ty::String) =>
        {
            Some(("result", "to_string_ls"))
        }
        (Ty::Applied(TypeConstructorId::List, e), Ty::String)
            if e.len() == 1 && matches!(e[0], Ty::Bool) =>
        {
            Some(("result", "to_string_lb"))
        }
        (Ty::Applied(TypeConstructorId::List, e), Ty::String)
            if e.len() == 1 && matches!(e[0], Ty::Float) =>
        {
            Some(("result", "to_string_lf"))
        }
        _ => None,
    }
}

fn interp_result_to_string_simple_applied(ok: &Ty, err: &Ty) -> Option<(&'static str, &'static str)> {
    use almide_lang::types::constructor::TypeConstructorId;
    match (ok, err) {
        (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
            if e.len() == 1 && matches!(e[0], Ty::Int) =>
        {
            Some(("result", "to_string_oi"))
        }
        (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
            if e.len() == 1 && matches!(e[0], Ty::String) =>
        {
            Some(("result", "to_string_os"))
        }
        (Ty::Applied(TypeConstructorId::Map, e), Ty::String)
            if e.len() == 2 && matches!(e[0], Ty::String) && matches!(e[1], Ty::Int) =>
        {
            Some(("result", "to_string_msi"))
        }
        _ => None,
    }
}

fn interp_result_to_string_nested(ok: &Ty, err: &Ty) -> Option<(&'static str, &'static str)> {
    use almide_lang::types::constructor::TypeConstructorId;
    match (ok, err) {
        (Ty::Applied(TypeConstructorId::Result, e), Ty::String)
            if e.len() == 2 && matches!(e[0], Ty::Int) && matches!(e[1], Ty::String) =>
        {
            Some(("result", "to_string_ri"))
        }
        (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
            if e.len() == 1
                && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                    if e2.len() == 1 && matches!(e2[0], Ty::String)) =>
        {
            Some(("result", "to_string_osl"))
        }
        (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
            if e.len() == 1
                && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                    if e2.len() == 1 && matches!(e2[0], Ty::Int)) =>
        {
            Some(("result", "to_string_oli"))
        }
        _ => None,
    }
}

/// Does a record/tuple/list/scalar VALUE of type `ty` materialize with REAL slots the recursive
/// Display can read — the STATIC (IR-type-only) predicate the gate and the lowering BOTH consult so
/// they agree on expand-vs-wrap BY CONSTRUCTION (no runtime-`materialized_aggregates` divergence).
/// Matches exactly what the construction path materializes:
///   - Int/Bool/Float/String          → yes (scalar / single heap leaf)
///   - List[scalar]                    → yes (scalar-element block); List[heap] → NO (not materialized)
///   - a registered record/tuple whose every field is itself `field_displayable` → yes (the
///     nested-aggregate construction admits a SCALAR-ONLY nested block; a heap-IN-nested field would
///     leak under the single-level mask, so it is NO)
///   - Map/Set/Option/Result/variant/unresolved → NO
fn field_displayable(ty: &Ty, registry: &RecordLayouts) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Int | Ty::Bool | Ty::Float | Ty::String => true,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => !is_heap_ty(&a[0]),
        Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..) => match resolve_aggregate(ty, registry) {
            // A NESTED aggregate must be SCALAR-ONLY (the construction's `lower_owned_heap_field`
            // admits only a scalar-only nested block — a heap-in-nested field would leak).
            Some((_, _, fields)) => fields.iter().all(|(_, t)| !is_heap_ty(t)),
            None => false,
        },
        _ => false,
    }
}

/// Is a record/tuple interpolation PART statically EXPAND-foldable — i.e. the lowering will
/// materialize it and read its real slots? True iff the part expr is a `Var` (a materialized
/// aggregate binding; a literal/call result is not a tracked block) AND every field of the
/// (resolvable) aggregate is `field_displayable`. The gate and the lowering both gate on THIS, so
/// the synthetic-call count the gate credits equals the calls the lowering emits — for both the
/// EXPAND path (recursive tree) and the WALL path (one `compound.to_string`).
pub(crate) fn aggregate_part_expandable(expr: &IrExpr, registry: &RecordLayouts) -> bool {
    if !matches!(expr.kind, IrExprKind::Var { .. }) {
        return false; // a literal `${P{..}}` / a call `${f()}` is not a tracked materialized block
    }
    match resolve_aggregate(&expr.ty, registry) {
        Some((_, _, fields)) => fields.iter().all(|(_, t)| field_displayable(t, registry)),
        None => false,
    }
}

/// Build the String-producing LEAF for ONE interpolation part, by type:
///   - a literal text part → a `LitStr` (no call),
///   - a String-typed part → the expr itself (identity, no call),
///   - an EXPAND-foldable RECORD/TUPLE part (a materialized Var with displayable fields) → the
///     recursive layout-driven Display ([`display_aggregate`]), an INLINE `ConcatStr` tree of
///     per-field formatters; a NON-expandable record/tuple part → ONE unlinked `compound.to_string`
///     wrapper (the function walls at render — never a wrong byte),
///   - any other part with a pure `module.to_string` → `module.to_string(expr)`.
/// Returns `None` for a part whose type has no admitted Display at all (an unresolved type) — the
/// caller then declines the whole desugar.
fn interp_part_leaf(p: &IrStringPart, registry: &RecordLayouts) -> Option<IrExpr> {
    match p {
        IrStringPart::Lit { value } => Some(lit_str(value)),
        IrStringPart::Expr { expr } if matches!(expr.ty, Ty::String) => Some(expr.clone()),
        // A record/tuple part: EXPAND if the lowering will materialize it; else wrap in the
        // unlinked `compound.to_string` so the function walls (the SAME decision the gate makes).
        IrStringPart::Expr { expr }
            if matches!(expr.ty, Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..))
                && resolve_aggregate(&expr.ty, registry).is_some() =>
        {
            // An ANONYMOUS record ALWAYS takes the generated sorted-field repr: v0 sorts
            // anon fields by name, while the inline display_aggregate expansion reads the
            // STRUCTURAL (source) order — expanding it would emit wrong bytes.
            if let Ty::Record { fields } = &expr.ty {
                // An ANONYMOUS record part — route to the generated
                // `__repr_anonrec_<hash>` (sorted-field render); an unemitted shape
                // (a nested payload) leaves the call unlinked = the honest wall.
                Some(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named {
                            name: sym(&format!(
                                "__repr_{}",
                                crate::lower::anon_record_drop_name(fields)
                            )),
                        },
                        args: vec![expr.clone()],
                        type_args: Vec::new(),
                    },
                    ty: Ty::String,
                    span: None,
                    def_id: None,
                })
            } else if aggregate_part_expandable(expr, registry) {
                display_aggregate(expr, &expr.ty, registry)
            } else if let Ty::Named(name, _) = &expr.ty {
                // A NAMED record outside the inline-expand subset (a recursive record, a
                // List[record] field — the compound_repr class): route to the GENERATED
                // `__repr_rec_<R>` (render-pipeline-injected). A record the generator does
                // not emit leaves the call unlinked — the same honest render wall the
                // `compound.to_string` wrapper gives, with the SAME call-count (one node).
                Some(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named {
                            name: sym(&format!(
                                "__repr_rec_{}",
                                crate::lower::drop_fn_ident(name.as_str())
                            )),
                        },
                        args: vec![expr.clone()],
                        type_args: Vec::new(),
                    },
                    ty: Ty::String,
                    span: None,
                    def_id: None,
                })
            } else {
                Some(to_string_call("compound", "to_string", expr.clone()))
            }
        }
        // A custom-VARIANT part (`"${Overflow(\"x\")}"` / a bound variant var): route
        // to the GENERATED `__repr_<V>` (render-pipeline-injected; the classify gate
        // counts the same call node). A variant the generator does not emit (a field
        // outside Int/Bool/String/nested-variant) leaves an unlinked call — the same
        // honest render wall the `compound.to_string` wrapper gives records.
        IrStringPart::Expr { expr }
            if matches!(&expr.ty, Ty::Named(..))
                && resolve_aggregate(&expr.ty, registry).is_none() =>
        {
            let Ty::Named(name, targs) = &expr.ty else { unreachable!() };
            // A GENERIC-variant instance (`${l}` over `ReprEither[Int, String]`) takes
            // the INSTANTIATION-KEYED repr — the exact key the generator derives
            // (`repr_inst_ident`), so the call links iff the instantiation is emitted.
            let rname = if targs.is_empty() {
                format!("__repr_{}", crate::lower::drop_fn_ident(name.as_str()))
            } else {
                format!("__repr_{}", crate::lower::repr_inst_ident(name.as_str(), targs))
            };
            Some(IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: sym(&rname) },
                    args: vec![expr.clone()],
                    type_args: Vec::new(),
                },
                ty: Ty::String,
                span: None,
                def_id: None,
            })
        }
        // A CONTAINER of a named record/variant (`${pts}` over List[Point], `${op}` over
        // Option[Point], `${shapes}` over List[Shape]) — route to the GENERATED container
        // repr (`__repr_list_rec_<R>` / `__repr_opt_rec_<R>` / `__repr_list_<V>`, emitted by
        // `generate_variant_repr_sources` for exactly the container/element pairs collected
        // from interp parts). An unemitted pair leaves the call unlinked — the honest wall
        // (same contract as the bare `__repr_rec_<R>` arm above).
        IrStringPart::Expr { expr }
            if container_repr_name(&expr.ty, registry).is_some() =>
        {
            let name = container_repr_name(&expr.ty, registry).expect("this arm's guard already proved container_repr_name(..).is_some() for the same &expr.ty");
            Some(IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: sym(&name) },
                    args: vec![expr.clone()],
                    type_args: Vec::new(),
                },
                ty: Ty::String,
                span: None,
                def_id: None,
            })
        }
        // `${List[(Int, String)]}` / `${Option[(Bool, Bool)]}` — SCALAR-component
        // tuple containers (list.sort / list.min/max results, the C-053 class): route
        // to the generated `__repr_list_tup_<key>` / `__repr_opt_tup_<key>` walkers.
        // A component outside Int/Bool/String keeps the existing `_x` wall routing.
        // `(String, Int)` is EXCLUDED on the List side — `${List[(String, Int)]}`
        // keeps its established `list.to_string_lsi` route (string_rle).
        IrStringPart::Expr { expr }
            if matches!(&expr.ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                    if a.len() == 1
                        && matches!(&a[0], Ty::Tuple(ts)
                            if crate::lower::tuple_repr_ident(ts).is_some()
                                && !(ts.len() == 2
                                    && matches!(ts[0], Ty::String)
                                    && matches!(ts[1], Ty::Int)))) =>
        {
            let Ty::Applied(_, a) = &expr.ty else { unreachable!() };
            let Ty::Tuple(ts) = &a[0] else { unreachable!() };
            let key = crate::lower::tuple_repr_ident(ts).expect("this arm's guard already proved tuple_repr_ident(ts).is_some() for the same ts");
            Some(IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: sym(&format!("__repr_list_tup_{key}")) },
                    args: vec![expr.clone()],
                    type_args: Vec::new(),
                },
                ty: Ty::String,
                span: None,
                def_id: None,
            })
        }
        IrStringPart::Expr { expr }
            if matches!(&expr.ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a)
                    if a.len() == 1
                        && matches!(&a[0], Ty::Tuple(ts)
                            if crate::lower::tuple_repr_ident(ts).is_some())) =>
        {
            let Ty::Applied(_, a) = &expr.ty else { unreachable!() };
            let Ty::Tuple(ts) = &a[0] else { unreachable!() };
            let key = crate::lower::tuple_repr_ident(ts).expect("this arm's guard already proved tuple_repr_ident(ts).is_some() for the same ts");
            Some(IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: sym(&format!("__repr_opt_tup_{key}")) },
                    args: vec![expr.clone()],
                    type_args: Vec::new(),
                },
                ty: Ty::String,
                span: None,
                def_id: None,
            })
        }
        // `${[a]}` over a List[<STRUCTURAL record>] (an inferred literal element — the
        // r5 C-072 class): route to the generated `__repr_list_anonrec_<hash>` walker
        // (its element repr renders NOMINALLY when the shape resolves to a declared
        // record, sorted-anonymous otherwise). An unemitted shape (a nested payload)
        // leaves the call unlinked — the honest wall.
        IrStringPart::Expr { expr }
            if matches!(&expr.ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                    if a.len() == 1 && matches!(a[0], Ty::Record { .. })) =>
        {
            let Ty::Applied(_, a) = &expr.ty else { unreachable!() };
            let Ty::Record { fields } = &a[0] else { unreachable!() };
            Some(IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named {
                        name: sym(&format!(
                            "__repr_list_{}",
                            crate::lower::anon_record_drop_name(fields)
                        )),
                    },
                    args: vec![expr.clone()],
                    type_args: Vec::new(),
                },
                ty: Ty::String,
                span: None,
                def_id: None,
            })
        }
        IrStringPart::Expr { expr } => {
            let (module, func) = interp_to_string_call(&expr.ty)?;
            Some(to_string_call(module, func, expr.clone()))
        }
    }
}

// `container_repr_name` and its helpers moved to `mod_p4_g.rs` (codopsy8 complexity sweep:
// the pattern-2 decomposition above grew this file past the 800-line `max-lines` threshold).
// `value_synthetic_names` / `aggregate_synthetic_names` / `interp_str_synthetic_call_count` /
// `interp_synthetic_call_names` / `interp_str_desugarable` / `list_heap_call_name` and its
// helpers / `option_call_name` and its helpers moved to `mod_p4_h.rs` (codopsy9: the grouped
// `.or_else()` split above grew this file past 800 lines again).

/// Desugar a STRING INTERPOLATION `"…${e}…"` into a left-nested `ConcatStr` fold,
/// seeded by an empty `""` literal: `(((("" ++ p0) ++ p1) … ) ++ p_{K-1})`. Each
/// part is wrapped in its type's `to_string` ([`interp_part_leaf`]) — a Lit/String
/// part is a no-call leaf, every other part a single `module.to_string` call.
/// Concatenating with the leading `""` is byte-identical to v0's `emit_string_interp`
/// (`"" ++ bytes == bytes`), so the folded String matches v0 in EVERY position.
///
/// This is the SINGLE source the lowering ([`LowerCtx::try_lower_string_interp`])
/// AND the corpus caps gate (`count_ir_calls` in classify_corpus) BOTH consult: the
/// gate counts the call NODES of the very tree the lowering emits, so the synthetic
/// MIR `Op::CallFn`s are 1:1 backed by IR call nodes — `mir_calls == ir_calls` for an
/// in-profile interp BY CONSTRUCTION (no `mir > ir` over-count, no spurious caps
/// taint). Soundness rests on one invariant: when this returns `Some(tree)`, every
/// leaf lowers to exactly one `CallFn` (a pure `module.to_string`, admitted by
/// `purity::is_pure`) or a no-call passthrough — so `try_lower_concat_str` never
/// rolls back. Returns `None` (the interp stays the deferred Opaque, credited 0 by
/// the gate) iff a part has no admitted `to_string` module — a memory-safe defer.
///
/// THE WALL DOES THE HEAVY LIFTING: a part whose `to_string` is UNLINKED (Float /
/// compound — registered in `PURE_MODULES` but not in the self-host runtime) still
/// desugars to a real `CallFn`, so the enclosing function emits an unlinked call and
/// the render wall (`try_render_wasm_program`) REJECTS it as `Unsupported`. Such a
/// function is OUT of profile, so it can never contribute a `count != lower`
/// mismatch — the only IN-profile interps are the fully-linkable ones (Lit/String/
/// Int/Bool), where `count == lower` is trivially exact.
pub fn desugar_string_interp(parts: &[IrStringPart], registry: &RecordLayouts) -> Option<IrExpr> {
    let mut acc = lit_str("");
    for p in parts {
        let leaf = interp_part_leaf(p, registry)?;
        acc = IrExpr {
            kind: IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatStr,
                left: Box::new(acc),
                right: Box::new(leaf),
            },
            ty: Ty::String,
            span: None,
            def_id: None,
        };
    }
    Some(acc)
}
