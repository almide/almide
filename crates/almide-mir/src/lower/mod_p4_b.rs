
/// `${Option[T]}` interp routing per payload type. Verbatim text move (#781).
fn interp_option_to_string(inner: &Ty) -> (&'static str, &'static str) {
    use almide_lang::types::constructor::TypeConstructorId;
    match inner {
        Ty::Int => ("option", "to_string"),
        Ty::String => ("option", "to_string_s"),
        Ty::Float => ("option", "to_string_f"),
        Ty::Bool => ("option", "to_string_b"),
        // `${Option[List[Int]]}` → `some([1, 2, 3])` / `none` — the inner list renders like
        // `${list}`, wrapped in `some(…)`. A deeper element routes to the UNLINKED `_x`.
        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::Int) => {
            ("option", "to_string_li")
        }
        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::String) => {
            ("option", "to_string_ls")
        }
        Ty::Applied(TypeConstructorId::Option, e) if e.len() == 1 && matches!(e[0], Ty::Int) => {
            ("option", "to_string_oi")
        }
        Ty::Applied(TypeConstructorId::Option, e) if e.len() == 1 && matches!(e[0], Ty::Bool) => {
            ("option", "to_string_ob")
        }
        Ty::Applied(TypeConstructorId::Option, e) if e.len() == 1 && matches!(e[0], Ty::String) => {
            ("option", "to_string_os")
        }
        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::Bool) => {
            ("option", "to_string_lb")
        }
        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::Float) => {
            ("option", "to_string_lf")
        }
        Ty::Applied(TypeConstructorId::Map, e)
            if e.len() == 2 && matches!(e[0], Ty::String) && matches!(e[1], Ty::Int) =>
        {
            ("option", "to_string_msi")
        }
        Ty::Applied(TypeConstructorId::Result, e)
            if e.len() == 2 && matches!(e[0], Ty::Int) && matches!(e[1], Ty::String) =>
        {
            ("option", "to_string_ri")
        }
        Ty::Applied(TypeConstructorId::Result, e)
            if e.len() == 2 && matches!(e[0], Ty::String) && matches!(e[1], Ty::String) =>
        {
            ("option", "to_string_rs")
        }
        Ty::Applied(TypeConstructorId::Option, e)
            if e.len() == 1
                && matches!(&e[0], Ty::Applied(TypeConstructorId::Option, e2)
                    if e2.len() == 1 && matches!(e2[0], Ty::Int)) =>
        {
            ("option", "to_string_ooi")
        }
        Ty::Applied(TypeConstructorId::Option, e)
            if e.len() == 1
                && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                    if e2.len() == 1 && matches!(e2[0], Ty::Int)) =>
        {
            ("option", "to_string_ooli")
        }
        Ty::Applied(TypeConstructorId::Result, e)
            if e.len() == 2
                && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                    if e2.len() == 1 && matches!(e2[0], Ty::Int))
                && matches!(e[1], Ty::String) =>
        {
            ("option", "to_string_rli")
        }
        _ => ("option", "to_string_x"),
    }
    }

/// `${Result[T, E]}` interp routing per (ok, err) pair. Verbatim text move (#781).
fn interp_result_to_string(ok: &Ty, err: &Ty) -> (&'static str, &'static str) {
    use almide_lang::types::constructor::TypeConstructorId;
        match (ok, err) {
            (Ty::Int, Ty::String) => ("result", "to_string"),
            (Ty::String, Ty::String) => ("result", "to_string_ss"),
            // `${Result[List[Int], String]}` → `ok([1, 2, 3])` / `err("<quoted>")`.
            (Ty::Applied(TypeConstructorId::List, e), Ty::String)
                if e.len() == 1 && matches!(e[0], Ty::Int) =>
            {
                ("result", "to_string_li")
            }
            (Ty::Applied(TypeConstructorId::List, e), Ty::String)
                if e.len() == 1 && matches!(e[0], Ty::String) =>
            {
                ("result", "to_string_ls")
            }
            (Ty::Bool, Ty::String) => ("result", "to_string_b"),
            (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
                if e.len() == 1 && matches!(e[0], Ty::Int) =>
            {
                ("result", "to_string_oi")
            }
            (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
                if e.len() == 1 && matches!(e[0], Ty::String) =>
            {
                ("result", "to_string_os")
            }
            (Ty::Applied(TypeConstructorId::Result, e), Ty::String)
                if e.len() == 2 && matches!(e[0], Ty::Int) && matches!(e[1], Ty::String) =>
            {
                ("result", "to_string_ri")
            }
            (Ty::Applied(TypeConstructorId::List, e), Ty::String)
                if e.len() == 1 && matches!(e[0], Ty::Bool) =>
            {
                ("result", "to_string_lb")
            }
            (Ty::Float, Ty::String) => ("result", "to_string_f"),
            (Ty::Applied(TypeConstructorId::List, e), Ty::String)
                if e.len() == 1 && matches!(e[0], Ty::Float) =>
            {
                ("result", "to_string_lf")
            }
            (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
                if e.len() == 1
                    && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                        if e2.len() == 1 && matches!(e2[0], Ty::String)) =>
            {
                ("result", "to_string_osl")
            }
            (Ty::Applied(TypeConstructorId::Map, e), Ty::String)
                if e.len() == 2 && matches!(e[0], Ty::String) && matches!(e[1], Ty::Int) =>
            {
                ("result", "to_string_msi")
            }
            (Ty::Applied(TypeConstructorId::Option, e), Ty::String)
                if e.len() == 1
                    && matches!(&e[0], Ty::Applied(TypeConstructorId::List, e2)
                        if e2.len() == 1 && matches!(e2[0], Ty::Int)) =>
            {
                ("result", "to_string_oli")
            }
            _ => ("result", "to_string_x"),
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

/// The generated container-repr callee for a `${List[<record>]}` / `${Option[<record>]}` /
/// `${List[<variant>]}` interp part, or `None` for every other type (falls to the
/// `interp_to_string_call` table). Record-vs-variant discrimination mirrors the bare-part
/// arms: a `Named` that resolves in the record registry is a record, else a variant.
fn container_repr_name(ty: &Ty, registry: &RecordLayouts) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let named = |t: &Ty| -> Option<(String, bool)> {
        let Ty::Named(n, _) = t else { return None };
        Some((n.as_str().to_string(), resolve_aggregate(t, registry).is_some()))
    };
    match ty {
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
            // A generic-variant INSTANTIATION element (`${forest}` over
            // `List[Tree[Int]]` — C-010): the instantiation-KEYED walker.
            if let Ty::Named(n, args) = &a[0] {
                if !args.is_empty() && resolve_aggregate(&a[0], registry).is_none() {
                    return Some(format!(
                        "__repr_list_{}",
                        crate::lower::repr_inst_ident(n.as_str(), args)
                    ));
                }
            }
            let (n, is_rec) = named(&a[0])?;
            let n_fn = crate::lower::drop_fn_ident(&n);
            Some(if is_rec {
                format!("__repr_list_rec_{n_fn}")
            } else {
                format!("__repr_list_{n_fn}")
            })
        }
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
            // An instantiation payload (`${opt}` over `Option[Tree[String]]`).
            if let Ty::Named(n, args) = &a[0] {
                if !args.is_empty() && resolve_aggregate(&a[0], registry).is_none() {
                    return Some(format!(
                        "__repr_opt_{}",
                        crate::lower::repr_inst_ident(n.as_str(), args)
                    ));
                }
            }
            let (n, is_rec) = named(&a[0])?;
            let n_fn = crate::lower::drop_fn_ident(&n);
            if !is_rec {
                // A CUSTOM-variant payload (`${opt_tree}` over `Option[Tree]` — C-009):
                // the generated `__repr_opt_<V>` (some/none over the variant repr).
                // Option/Result payloads keep their existing table routing.
                if matches!(&a[0], Ty::Named(vn, _)
                    if !matches!(vn.as_str(), "Option" | "Result" | "Value"))
                {
                    return Some(format!("__repr_opt_{n_fn}"));
                }
                return None;
            }
            Some(format!("__repr_opt_rec_{n_fn}"))
        }
        // `${Map[String, <record/variant>]}` — the paired-slot map repr (quoted keys,
        // element repr values, `[:]` when empty).
        Ty::Applied(TypeConstructorId::Map, a)
            if a.len() == 2 && matches!(a[0], Ty::String) =>
        {
            let (n, _is_rec) = named(&a[1])?;
            let n_fn = crate::lower::drop_fn_ident(&n);
            Some(format!("__repr_map_{n_fn}"))
        }
        _ => None,
    }
}

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

/// The SYNTHETIC call names the recursive Display ([`display_value`]) introduces for a
/// single value of type `ty` — the `<module>.to_string`-family wrappers, recursively. A
/// scalar/string/float/list value contributes ONE name; a record/tuple value contributes
/// none itself but recurses via [`aggregate_synthetic_names`] into its fields. This DOES
/// NOT count the value's OWN inner calls (it counts the WRAPPERS the desugar adds, not the
/// operand) — keeping the `count_ir_calls` operand-descent free of double counting.
fn value_synthetic_names(ty: &Ty, registry: &RecordLayouts, out: &mut Vec<String>) {
    match ty {
        // A nested record/tuple expands INLINE (recursive `__str_concat` + field formatters).
        Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..) if resolve_aggregate(ty, registry).is_some() => {
            aggregate_synthetic_names(ty, registry, out);
        }
        // Every OTHER value type routes to exactly ONE `to_string`-family call — the SAME single
        // wrapper [`display_value`] / [`interp_part_leaf`] emit (Int → int.to_string, Float →
        // float.to_string_compound, String → string.quote, List → list.to_string*, Map/Set/Option/
        // Result → the unlinked `<module>.to_string` that walls). Keyed off `display_leaf_call` so
        // the gate's count is BY CONSTRUCTION the lowering's emitted call set.
        _ => {
            if let Some((m, f)) = display_leaf_call(ty) {
                out.push(format!("{m}.{f}"));
            }
        }
    }
}

/// The SYNTHETIC call names the recursive Display ([`display_aggregate`]) introduces for an
/// aggregate of type `ty`: one `__str_concat` per `ConcatStr` fold the expansion builds
/// (= the number of `concat_all` parts at this level) plus the field formatters recursively.
/// MIRRORS `display_aggregate`'s structure EXACTLY so the gate credits precisely the
/// synthetic CallFns the lowering emits (count == lower for the aggregate, by construction).
fn aggregate_synthetic_names(ty: &Ty, registry: &RecordLayouts, out: &mut Vec<String>) {
    // A non-resolvable aggregate (structural record, unregistered) yields no Display tree —
    // the part declines and the whole interp credits 0 (matched by `interp_synthetic_call_names`).
    let Some((type_name, is_tuple, fields)) = resolve_aggregate(ty, registry) else {
        return;
    };
    if !is_tuple && type_name.is_none() {
        return; // structural record has no Display → walls, credits 0
    }
    // `concat_all` parts at this level: opening + (per field: a leading ", " for idx>0,
    // a "field: " label for a record, the field formatter) + closing.
    //   record: 1 (open) + Σ_i [ (i>0 → 1) + 1 (label) + 1 (formatter) ] + 1 (close)
    //   tuple:  1 (open) + Σ_i [ (i>0 → 1) +            1 (formatter) ] + 1 (close)
    let mut concat_parts = 2; // open + close
    for (idx, _) in fields.iter().enumerate() {
        if idx > 0 {
            concat_parts += 1; // ", "
        }
        if !is_tuple {
            concat_parts += 1; // "field: "
        }
        concat_parts += 1; // the field formatter expression
    }
    for _ in 0..concat_parts {
        out.push("__str_concat".to_string());
    }
    for (_, fty) in &fields {
        value_synthetic_names(fty, registry, out);
    }
}

/// Count the synthetic `CallFn`s [`desugar_string_interp`] yields for `parts` — the
/// `ConcatStr` and `module.to_string`-family call NODES of the desugared tree. The corpus
/// gate adds exactly this to its IR call count for each interp (it counts the same tree),
/// so the MIR calls the lowering emits are 1:1 backed. `None` (a part with no admitted
/// Display) ⇒ 0 (the interp stays Opaque, lowering emits no synthetic call).
pub fn interp_str_synthetic_call_count(parts: &[IrStringPart], registry: &RecordLayouts) -> usize {
    interp_synthetic_call_names(parts, registry).len()
}

/// The SYNTHETIC call names [`desugar_string_interp`] introduces for `parts`: one
/// `__str_concat` per TOP-LEVEL fold step (= `parts.len()`: K parts over the `""` seed ⇒ K
/// concats) and, per non-passthrough part, the Display wrappers it adds — a scalar part one
/// `<module>.to_string`, a RECORD/TUPLE part the full recursive `__str_concat` + field-
/// formatter set ([`aggregate_synthetic_names`]). It DOES NOT include the operands' OWN
/// inner calls (a `${g(x)}` callee) — those live in the original part exprs and are reached
/// separately by `count_ir_calls`'s descent, so no double count. Empty (a `None` desugar —
/// a part with no admitted Display) ⇒ the interp stays Opaque, crediting none.
pub fn interp_synthetic_call_names(parts: &[IrStringPart], registry: &RecordLayouts) -> Vec<String> {
    // A part with no admitted Display ⇒ the whole interp is non-desugarable (the lowering
    // returns `None` and defers to Opaque), so it credits zero synthetic calls.
    if desugar_string_interp(parts, registry).is_none() {
        return Vec::new();
    }
    let mut names = Vec::with_capacity(parts.len() * 2);
    // The TOP-LEVEL fold: K parts over the `""` seed ⇒ K `__str_concat` (the interp's own
    // outer concatenation — a record/tuple part is ONE top-level part here, its INNER
    // `__str_concat`s are added by `value_synthetic_names` below).
    for _ in 0..parts.len() {
        names.push("__str_concat".to_string());
    }
    for p in parts {
        let IrStringPart::Expr { expr } = p else {
            continue;
        };
        if matches!(expr.ty, Ty::String) {
            continue; // a String part is a no-call passthrough
        }
        // A TOP-LEVEL record/tuple part mirrors `interp_part_leaf`'s decision tree
        // EXACTLY (the mir == ir contract): an ANON record is ALWAYS one generated
        // `__repr_anonrec_<hash>` call; an expand-foldable named/tuple part credits the
        // full recursive tree; a non-expandable NAMED record one `__repr_rec_<R>`; any
        // other non-expandable aggregate one `compound.to_string` (the wall).
        if matches!(expr.ty, Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..))
            && resolve_aggregate(&expr.ty, registry).is_some()
        {
            if let Ty::Record { fields } = &expr.ty {
                names.push(format!(
                    "__repr_{}",
                    crate::lower::anon_record_drop_name(fields)
                ));
            } else if aggregate_part_expandable(expr, registry) {
                aggregate_synthetic_names(&expr.ty, registry, &mut names);
            } else if let Ty::Named(name, _) = &expr.ty {
                names.push(format!(
                    "__repr_rec_{}",
                    crate::lower::drop_fn_ident(name.as_str())
                ));
            } else {
                names.push("compound.to_string".to_string());
            }
        } else if let Some(n) = container_repr_name(&expr.ty, registry) {
            // Mirrors `interp_part_leaf`'s container-repr arm: ONE generated call node.
            names.push(n);
        } else {
            value_synthetic_names(&expr.ty, registry, &mut names);
        }
    }
    names
}

/// Is a WHOLE interpolation DESUGARABLE (every part has an admitted Display)? When true, the
/// lowering folds it to a `ConcatStr` chain; when false, it stays the deferred Opaque.
/// (Desugarable does NOT imply LINKABLE — a Float part desugars but float.to_string is
/// unlinked, so the function walls at render. Use the registry to split proven-vs-walled;
/// this predicate only answers "does the lowering fold it".)
pub fn interp_str_desugarable(parts: &[IrStringPart], registry: &RecordLayouts) -> bool {
    desugar_string_interp(parts, registry).is_some()
}

/// Does `module.func` return a real MATERIALIZED `Result[Int, String]` (the DynListStr len-as-tag
/// layout)? Its result may be tracked in `materialized_results` so an `Ok`/`Err` `match` over it
/// EXECUTES. NARROW to fns actually self-hosted — any other Result is a deferred `Opaque` (len 0,
/// would misread as `Ok`). `int.parse` is the canonical for string.to_int/to_integer/parse_int.
/// The CallFn name for a stdlib `module.func` call, routing the REPR-POLYMORPHIC list combinators
/// to their `_str` variant when the RESULT is a `List[heap]` (e.g. `list.map` over a `List[String]`
/// → `list.map_str`, a DynListStr-result impl). The element repr (i64 vs i32 handle) demands a
/// separate variant; the variant reads/writes via the heap-aware prim ops. Scalar-result lists keep
/// the plain name. `module.func` is unchanged for everything else.
pub(crate) fn list_heap_call_name(
    module: &str,
    func: &str,
    arg_tys: &[Ty],
    result_ty: &Ty,
    // Is the Map KEY type (of the first-arg/result Map) a NULLARY-ONLY variant?
    // Computed by the caller (LowerCtx has the variant_layouts; this router is a
    // free fn) — gates the `_vtag` tag-normalized map family. `map_key_scalar_rec`
    // is the all-Int/Bool-field record-key twin, gating `_srec`.
    map_key_nullary: bool,
    map_key_scalar_rec: bool,
) -> String {
    // A MONO-SPECIALIZED stdlib call name (`result.or_else__Int_String_String` —
    // the optimizer suffixes a generic intrinsic's instantiation) must route by
    // its BASE name: the registry links base names only, so the suffixed form
    // fell through every router arm to an UNLINKED dotted name and walled the fn
    // (fuzz B-198's or_else). The instantiation's types are already in
    // `arg_tys`/`result_ty` — the suffix carries no information the router needs.
    let func = func.split_once("__").map_or(func, |(base, _)| base);
    // #781: the monolithic 780-line dispatch (cog 324) is decomposed into
    // per-module routers. Routing ORDER is load-bearing and preserved: the
    // heap-accumulator `fold` guard fires BEFORE the per-module tables (a
    // scalar-acc fold over heap elements falls through to `list.fold_str`).
    if module == "random" && matches!(func, "choice" | "shuffle") {
        return random_call_name(func, arg_tys);
    }
    if module == "fan" && func == "map" {
        return fan_map_call_name(arg_tys, result_ty);
    }
    if func == "fold" && matches!(module, "list" | "map" | "set") && is_heap_ty(result_ty) {
        return heap_fold_call_name(module, arg_tys, result_ty);
    }
    let routed = match module {
        "list" => list_call_name(func, arg_tys, result_ty),
        "set" => set_call_name(func, arg_tys, result_ty),
        "map" => map_call_name(func, arg_tys, result_ty, map_key_nullary, map_key_scalar_rec),
        "result" | "option" if func == "unwrap_or" => unwrap_or_call_name(module, arg_tys),
        "option" => option_call_name(func, arg_tys, result_ty),
        "result" => result_call_name(func, arg_tys, result_ty),
        // `value.keys` IS `json.keys` (one impl, two stdlib names) — remap to the
        // registered self-host; every other value.* rides its own dotted name.
        "value" if func == "keys" => Some("json.keys".to_string()),
        _ => None,
    };
    routed.unwrap_or_else(|| format!("{module}.{func}"))
}

/// Route the payload-polymorphic `option` combinators by PAYLOAD/RESULT repr. The
/// self-host impls (`option_map.almd`) are `Option[Int]`-typed — SCALAR payloads
/// only (Int/Bool/Float ride the i64 slot identically). A HEAP payload or result
/// (`option.map(some("hi"), (s) => s + "!")`) invoked the scalar impl anyway: the
/// closure declares the `$closure_fnN_h` (i32-result) type while `__opt_map_some`
/// calls through `$closure_fnN` (i64) — the "indirect call type mismatch" TRAP on
/// the verified default (the #790 option.map row, main-reachable). Route those to
/// an UNREGISTERED wall suffix instead — the fn walls, v0 runs the shape correctly
/// (the same honest-wall pattern as the map `_skv_wall` family).
fn option_call_name(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    // `option.to_list` keys on the PAYLOAD: a flat heap payload (String /
    // List[scalar] / scalar tuple) rides the co-owning `_rc` variant (the raw slot
    // copy aliased the payload un-owned — double free); a richer payload walls.
    if func == "to_list" {
        if let Some(Ty::Applied(TC::Option, a)) = arg_tys.first() {
            if a.len() == 1 && is_heap_ty(&a[0]) {
                if matches!(a[0], Ty::String) || is_flat_scalar_block_ty(&a[0]) {
                    return Some("option.to_list_rc".to_string());
                }
                return Some("option.to_list_x".to_string());
            }
        }
    }
    // ONE mismatch axis is the CLOSURE's RESULT repr: params always ride the
    // widened i64 slots, and an Option-returning closure uses the same `_h` table
    // type the impl declares (flat_map / or_else match by construction; filter's
    // pred is scalar-result; flatten / zip take no closure at all). The two shapes
    // whose USER closure result repr can diverge from the scalar-typed impl:
    //   - `option.map` with a HEAP mapped payload (impl `f: (Int) -> Int` = i64
    //     result; a `(s) => s + "!"` closure declares the i32 `_h` type)
    //   - `option.unwrap_or_else` with a HEAP payload (impl `f: () -> Int`)
    let heap_option =
        |t: &Ty| matches!(t, Ty::Applied(TC::Option, a) if a.len() == 1 && is_heap_ty(&a[0]));
    match func {
        // The heap twins declare the closure heap-typed, so the `_h` CallIndirect
        // table type matches by construction (option_map.almd's `_h` family).
        "map" if heap_option(result_ty) => Some("option.map_h".to_string()),
        // filter/flatten/or_else heap twins: the OTHER axis is OWNERSHIP — the
        // kept payload must SHARE (Dup) into the rebuilt some(); the scalar
        // rewrap raw-copied the handle un-owned (or_else: fuzz seed-20260718
        // index 622, correct output then an __rc_dec trap at scope end).
        "filter" if heap_option(result_ty) => Some("option.filter_h".to_string()),
        "flatten" if heap_option(result_ty) => Some("option.flatten_h".to_string()),
        "or_else" if heap_option(result_ty) => Some("option.or_else_h".to_string()),
        // `to_result` over ANY heap payload: the heap twin builds the CAP-AS-TAG
        // Result the consumers read (the scalar impl's len-as-tag misread). The twin's
        // internals are payload-type-independent (one handle slot, Dup-shared into
        // ok()), so a NESTED heap payload (`Option[Result[Int, String]]` —
        // to_result_nested_share) rides the same routine as the String payload; the
        // co-own (+1) discipline is exactly the fix for the v0 double-free that
        // fixture pinned.
        "to_result"
            if matches!(arg_tys.first(), Some(Ty::Applied(TC::Option, a))
                if a.len() == 1 && is_heap_ty(&a[0])) =>
        {
            Some("option.to_result_h".to_string())
        }
        "unwrap_or_else" if is_heap_ty(result_ty) => {
            Some("option.unwrap_or_else_h".to_string())
        }
        _ => None,
    }
}