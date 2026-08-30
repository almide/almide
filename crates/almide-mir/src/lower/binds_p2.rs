impl LowerCtx {
    /// The type-driven scope-end drop handle for a `Map[String, <Named>]` value (the
    /// desugared map literal's `from_list_hobj` result, split layout): a VARIANT value
    /// routes to the generated `$__drop_map_<V>` (key rc_dec + flat/recursive value free,
    /// generated for EVERY variant); a SCALAR-ONLY record to `$__drop_map_rec_<R>`
    /// (both slots flat rc_dec). A heap-field RECORD value returns `None` — no generated
    /// sweep exists, so the bind keeps the honest deferral/wall (never a leaky flat link).
    pub(crate) fn map_named_value_drop(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::Map, a) = ty else { return None };
        if a.len() != 2 || !matches!(a[0], Ty::String) {
            return None;
        }
        let Ty::Named(n, _) = &a[1] else { return None };
        let ns = n.as_str();
        if self.variant_layouts.by_type.contains_key(ns) {
            return Some(format!("map_{}", crate::lower::drop_fn_ident(ns)));
        }
        if crate::lower::canonical_record_key(&self.record_layouts, ns).is_some()
            && self
                .aggregate_field_tys(&a[1])
                .is_some_and(|(_, tys)| tys.iter().all(|t| !is_heap_ty(t)))
        {
            return Some(format!("map_rec_{}", crate::lower::drop_fn_ident(ns)));
        }
        None
    }

    /// `List[(<scalar>, <RICH variant V>)]` — the `list.enumerate_h` result
    /// (#1496). Slot0 @12 is scalar, slot1 @20 is a variant owning further
    /// heap, so the exact free is the generated `$__drop_list_int_<V>` —
    /// `DropListStr`'s flat sweep would free the tuple blocks and leak every
    /// element's tree. Mirrors `map_named_value_drop` above: admission ⊆
    /// generation, because `is_rich_variant_ty` asks the same question the
    /// drop generator's filter does; anything it cannot name returns `None`
    /// and the caller keeps the honest deferral/wall.
    pub(crate) fn list_int_variant_drop(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::List, a) = ty else { return None };
        let [Ty::Tuple(tys)] = &a[..] else { return None };
        let [k, v] = &tys[..] else { return None };
        if is_heap_ty(k) {
            return None;
        }
        let vn = self.variant_layouts.is_rich_variant_ty(v, &|rn| {
            crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
        })?;
        Some(format!("list_int_{}", crate::lower::drop_fn_ident(&vn)))
    }

    pub(crate) fn lower_bind(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        // `let r = e!` (Unwrap — effect-fn error propagation) bound to a let/var was a deferred
        // `Const`/`Alloc{Opaque}` = a SILENT MISCOMPILE (`int.parse(s)!` bound 0, `g()!` empty).
        // The early-return brick HAS now arrived (`Op::Return`, R2): under the probe the ONE
        // rule lowers it linearly; outside the admitted shapes (and always with the probe off,
        // where the position desugars still run first) WALL it — NEVER bind a silently-wrong
        // value (the ② cardinal rule). Both scalar + heap paths.
        if matches!(&value.kind, IrExprKind::Unwrap { .. }) {
            if self.try_lower_bind_unwrap_return(var, ty, value)? {
                return Ok(());
            }
            return Err(LowerError::Unsupported(
                "unwrap `!` bound to a let/var cannot be faithfully computed (needs early-return \
                 propagation; a Const/Opaque would be a silently wrong value) not in this brick"
                    .into(),
            ));
        }
        // A `Try` bind (the auto-`?` node the codec DERIVE decoders synthesize
        // per `?`-bound field — user auto-? was removed by ADR-0008/E041) has
        // the SAME propagation semantics as `!`. Under the probe the position
        // desugars that used to restructure it are off, so route it through
        // the one rule; a decline falls through to the existing chain (the
        // nested-arm reach wall stays the honest fallback). Probe-off is
        // untouched — the desugar ladder still owns the node.
        if crate::lower::bang_return_probe() && matches!(&value.kind, IrExprKind::Try { .. }) {
            if self.try_lower_bind_unwrap_return(var, ty, value)? {
                return Ok(());
            }
        }
        // A BLOCK-valued bind (`let a = { let n = 5; n * n }` — an inlined pipe-lambda, or any block
        // in value position): lower the block's statements as effects in the current scope, then bind
        // `var` to the block's TAIL by recursing. Without this the Block falls through to the scalar
        // path's deferred `Const` and mis-lowers to 0. A block-local `let` extends to the outer scope
        // — a conservative, memory-safe lifetime extension (the same discipline as a deferred reassign).
        if let IrExprKind::Block { stmts, expr: Some(tail) } = &value.kind {
            for stmt in stmts {
                self.lower_stmt(stmt)?;
            }
            return self.lower_bind(var, ty, tail);
        }
        // A SHARED-CELL var (captured by a lambda AND mutated — cells.rs): bind it
        // into a 1-slot heap cell instead of a plain local, so the closure and the
        // enclosing scope share storage. Only the admitted inner classes take a cell;
        // an unadmitted class binds normally and `lift_lambda`'s mutated-capture
        // gate refuses the lift — an honest wall, never the value-copy miscompile.
        if self.cell_vars.contains(&var) {
            if let Some(class) = self.cell_class_of_ctx(ty) {
                return self.lower_cell_bind(var, ty, value, class);
            }
        }
        // Decomposed (#781, cog 272): the SCALAR path and the HEAP path are
        // verbatim text moves into `lower_bind_scalar` / `lower_bind_heap` —
        // behavior proven by the classify wall-list byte-identity ladder.
        if !is_heap_ty(ty) {
            return self.lower_bind_scalar(var, ty, value);
        }
        self.lower_bind_heap(var, ty, value)
    }

    /// THE ONE `!` RULE (return-op-eradication R2, law 6): `let x = f()!` in a
    /// declared-Result fn lowers to a linear err-check + frame exit + payload
    /// bind — no continuation nesting, no tail duplication, no loop flag:
    ///
    ///   v = lower(f())                      // the callee's Result carrier
    ///   tag = load32(handle(v) + 4)         // scalar family: len-as-tag
    ///   IfThen(tag) {                       // err ⇒ this path EXITS the frame
    ///     drops(live_heap_handles ∖ {v})    // derived from the live walk
    ///     Return(v)                         // the carrier IS the fn's err value
    ///   } EndIf
    ///   x = load64(handle(v) + 12)          // straight-line ok-payload bind
    ///
    /// Increment 1 admits: probe ON, declared-Result fn, SCALAR family on BOTH
    /// sides (the err layout is family-uniform, so the callee's carrier is a
    /// valid fn-ret value with NO rebox), scalar/Unit payload, matching err
    /// types. Everything else declines to the wall (the R2 decline matrix).
    /// The callee lowers through the ordinary `lower_bind` machinery (full
    /// tracking/seeding) under a [`Self::speculate`] snapshot, so a decline
    /// leaves no half-emitted ops.
    pub(crate) fn try_lower_bind_unwrap_return(
        &mut self,
        var: VarId,
        ty: &Ty,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        use crate::PrimKind;
        use almide_lang::types::constructor::TypeConstructorId;
        if !crate::lower::bang_return_probe() {
            return Ok(false);
        }
        let dbg = std::env::var_os("ALMIDE_DBG_BANG").is_some();
        macro_rules! decline {
            ($gate:expr) => {{
                if dbg {
                    eprintln!("BANG-DECLINE {} :: {}", $gate, self.fn_name);
                }
                return Ok(false);
            }};
        }
        let (IrExprKind::Unwrap { expr } | IrExprKind::Try { expr }) = &value.kind else {
            return Ok(false);
        };
        // A DECLARED-Result fn admits by its declared family; a LIFTED effect
        // fn by its synthetic `Result[T, String]` carrier's family — BOTH are
        // now computed at ctx build (`decl_ret_family` covers the lift; the
        // first cut hard-coded Scalar here and mislabeled heap-ret lifted fns
        // into the rebox path — the fs_fold_lines_range wrong-value).
        let fn_fam = self.decl_ret_family;
        // A VOID fn (a `main`/test block with no Result channel — the lifted
        // Unit convention): its err path cannot RETURN a carrier, it ABORTS
        // with the v0-identical "Error: <msg>\n" line. Same one rule, the
        // exit action differs — Zig's `DefersToEmit` axis: one walk, a mode
        // flag for what the exit does.
        // A VOID fn has NO Result channel at all: not a declared Result, not
        // an auto-wrapped lift (`decl_ret_family` covers both), and not the
        // Result ABI flag. Its `!` aborts instead of returning.
        // A VOID fn has NO Result channel at all: not a declared Result, not
        // an auto-wrapped lift (`decl_ret_family` covers both), and not the
        // Result ABI flag. Its `!` aborts instead of returning.
        //
        // NARROWED to a SCALAR-family callee: a lifted `effect fn -> <heap>`
        // builds its carrier through the scalar-family materializer (len@4 as
        // the tag) while `result_family` types it HeapOk (tag@16) — reading
        // @16 there took the err branch on an OK carrier and `die`d on string
        // bytes (bang_fold3). Closing that producer/consumer split is its own
        // slice; until then a heap-ok callee in a void fn declines.
        // …and the fn must ACTUALLY return Unit: a pure fn returning a real
        // value (`fn use_first_class() -> (Int, Int)`) has no Result channel
        // either, but its `!` is NOT an abort — inside a fallible lambda it
        // is the lambda's own propagation (fallible_lambda L1: aborting there
        // killed the process mid-test instead of yielding the `??` fallback).
        // #1437 L1 lifted the heap-callee narrowing: under the probe every
        // lifted heap carrier is wrapped and @16-readable, and a declared
        // Result callee always was — so a void fn's `!` admits ANY Result
        // family (the abort message is the err String @12, family-uniform)
        // and an OPTION callee (none aborts with the manufactured "none",
        // matching the desugar's v0 line byte for byte).
        let void_fn = fn_fam.is_none()
            && !self.ret_is_result_abi
            && matches!(self.decl_ret_ty_is_unit, true);
        // An OPTION callee (`let x = find(k)!` over `-> T?`): the carrier has
        // no err channel — the none path CONSTRUCTS the fn's err("none") (the
        // desugar's build_option_unwrap_match contract) — so it requires a
        // String-err Result channel on the FN side (declared Result[_, String]
        // or the lifted synthetic carrier; a custom-err fn would type-pun the
        // manufactured message, an Option-returning fn propagates none itself
        // and a void fn aborts — all three keep their position desugar).
        let callee_is_option =
            matches!(&expr.ty, Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1);
        // An OPTION-RETURNING fn (declared `-> T?`): its `!` on an Option
        // callee PROPAGATES THE NONE ITSELF (#1067 — the pass-through is
        // repr-identical: every Option none is a len@4 = 0 block), so the
        // exit arm is the SAME drops + Return(carrier) the same-family
        // Result case uses — no rebox, no manufactured message. Identified
        // by the declared-variant flag with NO err channel (decl_fn_err is
        // None exactly for a declared Option / an Option-typed lambda body).
        let opt_fn = self.decl_ret_is_result && self.decl_fn_err.is_none();
        if fn_fam.is_none() && !void_fn && !opt_fn {
            decline!("fn-family");
        }
        // A RESULT callee inside an Option-returning fn has an err payload
        // with nowhere to go (returning the carrier would type-pun err as
        // some) — keep its honest wall.
        if opt_fn && !callee_is_option && !void_fn {
            decline!("result-in-option-fn");
        }
        if callee_is_option {
            if !void_fn && !opt_fn && !matches!(self.decl_fn_err, Some(Ty::String)) {
                decline!("option-fn-channel");
            }
        } else if !matches!(&expr.ty, Ty::Applied(TypeConstructorId::Result, _)) {
            decline!("callee-family");
        }
        let callee_fam = crate::lower::result_family(&expr.ty);
        let fn_fam = fn_fam.unwrap_or(callee_fam);
        // SAME family on both sides: the callee's carrier IS a valid fn-ret
        // value on the err path (the err layout is family-uniform), returned
        // with NO rebox. CROSS-family: the err String is extracted @12 (same
        // offset both families), Dup'd (inc strictly before any release — the
        // Lean oproj law), the carrier released, and the message reboxed via
        // `materialize_result_err_str` — whose Err block is the FAMILY
        // SUPERSET (len@4=1 for len-as-tag readers AND tag@16=1 for
        // cap-as-tag readers), so ONE constructor serves both directions.
        let rebox = !void_fn && !opt_fn && (callee_is_option || callee_fam != fn_fam);
        let rebox_repr = if rebox {
            match crate::lower::repr_of(&expr.ty) {
                Ok(r) => Some(r),
                Err(_) => decline!("rebox-repr"),
            }
        } else {
            None
        };
        // The ok-payload bind classes this slice owns: scalar/Unit (value
        // copy), and for a HeapOk callee a String / flat heap-elem list /
        // Option/Result payload (Dup'd — see below) plus the ADT classes
        // (variant / record / tuple) the ordinary Named-call bind seeds.
        // Anything else (Value, maps, generic records) declines to the wall
        // for the next slice.
        let mut adt_payload = false;
        let heap_payload_class = if is_heap_ty(ty) {
            if !callee_is_option && callee_fam != crate::lower::ResultFamily::HeapOk {
                decline!("heap-payload-scalar-carrier");
            }
            if matches!(ty, Ty::String)
                || crate::lower::is_heap_elem_list_ty(ty)
                || matches!(
                    ty,
                    Ty::Applied(TypeConstructorId::Option | TypeConstructorId::Result, _)
                )
            {
                true
            } else if matches!(ty, Ty::Named(n, a)
                    if a.is_empty() && self.variant_layouts.by_type.contains_key(n.as_str()))
                || self.aggregate_field_tys(ty).is_some_and(|(_, tys)| {
                    self.record_or_anon_drop_type_name(ty).is_some()
                        || tys.iter().all(|f| !is_heap_ty(f))
                })
                // …plus the residue classes the ordinary bind's seeding also
                // owns: a VALUE payload (runtime-tag-dispatched DropValue via
                // value_handles), a SCALAR-element list/set (flat block
                // free), a tuple whose heap slots are all Strings (the masked
                // one-level sweep frees exactly those slots), and a
                // Map[String, scalar] (the split layout whose DropListStr
                // sweep rc_decs exactly the n key Strings — the route
                // seed_call_named_heap_drop_route_b already carries).
                || crate::lower::is_value_ty(ty)
                || matches!(ty, Ty::Applied(TypeConstructorId::Map, a)
                    if a.len() == 2 && matches!(a[0], Ty::String) && !is_heap_ty(&a[1]))
                || matches!(ty, Ty::Applied(TypeConstructorId::List | TypeConstructorId::Set, a)
                    if a.len() == 1 && !is_heap_ty(&a[0]))
                || matches!(ty, Ty::Tuple(ts)
                    if ts.iter().all(|f| !is_heap_ty(f) || matches!(f, Ty::String)))
            {
                // A user ADT payload — variant (rich or flat) or record/tuple
                // (recursive-drop, anonrec, or all-scalar): the SAME classes a
                // plain `let r = f()` Named-call bind admits, seeded by the
                // same routine (`seed_call_named_heap_read_shape` below), so
                // the admission envelope and the drop/read soundness story are
                // exactly the ordinary bind's. A record outside that envelope
                // (a generic decl whose one-level mask would leak a nested
                // heap field) still declines.
                adt_payload = true;
                true
            } else {
                if dbg {
                    eprintln!("BANG-PAYLOAD-TY {:?} :: {}", ty, self.fn_name);
                }
                decline!("heap-payload-class");
            }
        } else {
            false
        };
        // The err components must agree — the pass-through would type-pun a
        // mismatched err payload (the collect_map! class; v0 map_err-coerces).
        if !callee_is_option && self.unwrap_tail_err_mismatch(expr) {
            decline!("err-mismatch");
        }
        // Lower the callee through the FULL existing bind machinery onto a
        // synthetic var, under a speculation snapshot: a decline (or a carrier
        // the rule's ownership story cannot hold — it needs an OWNED live
        // handle: the err path moves it out, the ok path leaves it to the
        // scope-end drop) rolls back with no half-emitted ops.
        let attempt = self.speculate(|ctx| {
            let tmp = VarId(crate::lower::desugar_var_seed());
            ctx.lower_bind(tmp, &expr.ty, expr).ok()?;
            let v = *ctx.value_of.get(&tmp)?;
            if !ctx.live_heap_handles.contains(&v) {
                return None;
            }
            Some(v)
        });
        let Some(v) = attempt else { decline!("callee-lowering") };
        if dbg {
            eprintln!(
                "BANG-FIRE {} :: callee_ty={:?} fam={:?} opt={} rebox={} void={}",
                self.fn_name, expr.ty, callee_fam, callee_is_option, rebox, void_fn
            );
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![v] });
        // The err tag: Scalar family reads len-as-tag @4; HeapOk reads the
        // dedicated tag slot @16 (len is pinned to 1 there).
        // The void abort's message pieces are allocated BEFORE the branch (the
        // overflow-abort precedent: pre-branch alloc, in-arm die, continue
        // path frees at scope end — no ownership event inside an arm). An
        // OPTION none has no payload, so its line is fully static; a Result
        // err's line is `"Error: " + <msg @12> + "\n"` (build_main_die_line's
        // exact spelling), concatenated IN the arm with unreachable balancing
        // drops after the die (the arm machinery's own shape).
        let void_msg_pieces = if void_fn {
            if callee_is_option {
                let msg = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst: msg,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::Str("Error: none\n".into()),
                });
                self.live_heap_handles.push(msg);
                Some((msg, None))
            } else {
                let pre = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst: pre,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::Str("Error: ".into()),
                });
                self.live_heap_handles.push(pre);
                let nl = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst: nl,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::Str("\n".into()),
                });
                self.live_heap_handles.push(nl);
                Some((pre, Some(nl)))
            }
        } else {
            None
        };
        let tag_off = if callee_is_option {
            4
        } else {
            match callee_fam {
                crate::lower::ResultFamily::Scalar => 4,
                crate::lower::ResultFamily::HeapOk => 16,
            }
        };
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        // Result: nonzero tag = err (the exit). Option: len@4 == 0 = none —
        // invert to an eq-0 scalar so the SAME IfThen(exit) frame serves both.
        let exit_cond = if callee_is_option {
            let zero = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: zero, value: 0 });
            let is_none = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: is_none, op: crate::IntOp::Eq, a: tag, b: zero });
            is_none
        } else {
            tag
        };
        self.ops.push(Op::IfThen { cond: exit_cond, dst: None });
        // The exit path never mutates `live_heap_handles` — the surviving ok
        // continuation still owns everything; the arm only EMITS the drops.
        if void_fn {
            // ABORT exit: die with the err message — the v0 unit-main
            // convention ("Error: <msg>\n" is prefixed by the die runtime's
            // own line, exactly as build_main_die_line's split form feeds it).
            // The err String sits @12 in BOTH families; the load is a BORROW
            // and `prim.die` never returns, so no ownership event is needed
            // (the process ends — the same accounting the overflow-abort
            // shape uses).
            let (line, balance) = match void_msg_pieces {
                Some((msg, None)) => (msg, Vec::new()),
                Some((pre, Some(nl))) => {
                    let eb = self.load_at_offset(h, 12, PrimKind::LoadHandle);
                    let t1 = self.fresh_value();
                    self.ops.push(Op::CallFn {
                        dst: Some(t1),
                        name: "__str_concat".to_string(),
                        args: vec![crate::CallArg::Handle(pre), crate::CallArg::Handle(eb)],
                        result: Some(crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
                    });
                    let t2 = self.fresh_value();
                    self.ops.push(Op::CallFn {
                        dst: Some(t2),
                        name: "__str_concat".to_string(),
                        args: vec![crate::CallArg::Handle(t1), crate::CallArg::Handle(nl)],
                        result: Some(crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
                    });
                    (t2, vec![t1, t2])
                }
                None => unreachable!("void_fn set but no message pieces"),
            };
            // `prim.die` takes the message's ADDRESS (i64), not the i32
            // handle — the same `Handle`-then-`Die` pair the overflow abort
            // emits (calls_p4_b.rs:589-591). The concat temporaries are
            // released AFTER the die — unreachable, but they keep the arm's
            // ownership net at zero (the arm machinery's drop_arm_locals
            // shape; the process never executes them).
            let mh = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(mh), args: vec![line] });
            self.ops.push(Op::Prim { kind: PrimKind::Die, dst: None, args: vec![mh] });
            for t in balance {
                self.ops.push(Op::Drop { v: t });
            }
        } else if let Some(repr) = rebox_repr {
            // The err piece moved into the reboxed block: an OPTION carrier has
            // none — manufacture the desugar-identical "none" message; a
            // Result carrier's err String is extracted @12 and Dup'd (inc
            // strictly before any release — the Lean oproj law).
            let e_dup = if callee_is_option {
                let msg = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst: msg,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::Str("none".into()),
                });
                msg
            } else {
                let eb = self.load_at_offset(h, 12, PrimKind::LoadHandle);
                let e_dup = self.fresh_value();
                self.ops.push(Op::Dup { dst: e_dup, src: eb });
                e_dup
            };
            let live: Vec<ValueId> = self.live_heap_handles.clone();
            for other in live {
                if other != v {
                    let op = self.drop_op_for(other);
                    self.ops.push(op);
                }
            }
            let vop = self.drop_op_for(v);
            self.ops.push(vop);
            let new = self.materialize_result_err_str(e_dup, repr);
            self.ops.push(Op::Return { val: Some(new) });
        } else {
            let live: Vec<ValueId> = self.live_heap_handles.clone();
            for other in live {
                if other != v {
                    let op = self.drop_op_for(other);
                    self.ops.push(op);
                }
            }
            self.ops.push(Op::Return { val: Some(v) });
        }
        self.ops.push(Op::EndIf { val: None });
        // Straight-line ok continuation: bind the payload; the carrier stays
        // live and the ordinary scope-end (recursive) drop releases it. A
        // SCALAR payload is a value COPY (load64). A HEAP payload is a BORROW
        // (LoadHandle) immediately made an OWNED second reference (`Dup` —
        // inc strictly before any release of the parent, which here is not
        // until scope end), then seeded with its type's read/drop facts so
        // downstream reads dispatch and the scope-end drop takes the right
        // route.
        if matches!(ty, Ty::Unit) {
            let d = self.fresh_value();
            self.ops.push(Op::Const { dst: d });
            self.value_of.insert(var, d);
        } else if heap_payload_class {
            let borrowed = self.load_at_offset(h, 12, PrimKind::LoadHandle);
            let payload = self.fresh_value();
            self.ops.push(Op::Dup { dst: payload, src: borrowed });
            self.live_heap_handles.push(payload);
            // EVERY payload class rides the ordinary Named-call bind's
            // seeding — the SAME route+read pair, in the same order: the
            // drop-route chain (map key-sweeps, list_<R>, lenlist, the
            // List[Value]/value sets), then the read shapes (record field
            // reads, variant read-shape via seed_variant_param, materialized
            // lists). One seeding story, not a per-class fork: the classic
            // branch's hand-rolled flat_elems missed value_elem_lists on a
            // List[Value] payload, and the codec decoder's element reads came
            // back wrong ("expected Str" on an OK decode — the t3 probe).
            self.seed_call_named_heap_drop_route(payload, ty);
            self.seed_call_named_heap_read_shape(payload, ty);
            self.seed_variant_value_shape(payload, ty);
            if adt_payload {
                // A USER VARIANT payload additionally routes its scope-end
                // drop: RICH recurses via the generated `$__drop_<V>`
                // (named_route -> DropVariant), FLAT frees one level under
                // the default Drop.
                if let Ty::Named(n, args) = ty {
                    if args.is_empty()
                        && self.variant_layouts.needs_recursive_drop(n.as_str(), &|rn| {
                            crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
                        })
                    {
                        self.value_drops.entry(payload).or_default().named_route =
                            Some(n.as_str().to_string());
                    }
                }
            }
            self.value_of.insert(var, payload);
        } else {
            let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
            self.value_of.insert(var, payload);
        }
        Ok(true)
    }

    /// The SCALAR half of [`Self::lower_bind`] (`!is_heap_ty(ty)`): Copy values,
    /// no ownership accounting — executable scalar calls / literals / arithmetic /
    /// if- and match-values / `??` / var copies, else the deferred `Const`
    /// (strict mode walls it). Verbatim text move.
    fn lower_bind_scalar(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        // Scalar binding: a Copy value, no ownership accounting. A RESOLVABLE
        // scalar call (`let n = add(2, 3)`, `let m = string.len(s)`) is lowered to
        // a real executable `CallFn` (args materialized, the scalar result bound)
        // so it RUNS. Any other scalar value — arithmetic, a literal, an
        // unresolvable Method/Computed call — keeps the deferred `Const` + elided-
        // caps marker: its CONTENT is carried by a later brick, its calls still
        // folded for capabilities (`var n = obj.m()` elided ⇒ honest caps taint).
        if let Some(dst) = self.try_lower_scalar_call(value, ty) {
            self.value_of.insert(var, dst);
            return Ok(());
        }
        if self.try_lower_bind_scalar_literal(var, value) {
            return Ok(());
        }
        if self.try_lower_bind_scalar_operator_or_prim(var, value) {
            return Ok(());
        }
        // A scalar `if`/`match` VALUE (`let step = if c then 0 else 1`) EXECUTES — only
        // the taken arm runs — via the if-marker machinery; a non-literal `match` or a
        // non-scalar subject falls through to the deferred `Const`.
        if self.try_lower_bind_scalar_if_value(var, ty, value) {
            return Ok(());
        }
        if self.try_lower_bind_scalar_match(var, ty, value)? {
            return Ok(());
        }
        if self.try_lower_bind_scalar_unwrap_or(var, value)? {
            return Ok(());
        }
        if self.try_lower_bind_scalar_var_alias(var, value) {
            return Ok(());
        }
        if self.try_lower_bind_scalar_projection(var, value) {
            return Ok(());
        }
        self.lower_bind_scalar_deferred_const(var, value)
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether an Int/Float/Bool LITERAL binds to its real materialized scalar value.
    /// `true` means `var` is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_literal(&mut self, var: VarId, value: &IrExpr) -> bool {
        // An INT literal carries its real value (`ConstInt` → `(i64.const v)`),
        // the scalar-value foundation; other scalars stay the deferred `Const`. A FLOAT
        // literal carries its f64 BITS the same way (the float-floor render reinterprets).
        // A BOOL literal materializes to ConstInt 0/1 (else `var b=true` stays a deferred
        // Const 0, and `if b` / `match b { true=>.. }` always takes the false arm).
        if let IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. } = &value.kind
        {
            if let Some(dst) = self.lower_scalar_value(value) {
                self.value_of.insert(var, dst);
                return true;
            }
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a BinOp/UnOp/prim-floor RuntimeCall computes inside the executable scalar
    /// subset (rolling `ops` back to the entry mark when it does not). `true` means `var`
    /// is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_operator_or_prim(&mut self, var: VarId, value: &IrExpr) -> bool {
        // A scalar Int Add/Sub/Mul computes its real value (IntBinOp), and a
        // scalar prim-floor call (`let n = prim.load32(a)`) becomes an Op::Prim —
        // both via lower_scalar_value; outside the subset it rolls back to `Const`.
        // A UnOp (`let hc = not list.is_empty(xs)`, `let m = -n`) goes the SAME way —
        // without it, `not <call>` fell to the deferred `Const` below (the operand call
        // unemitted, the var silently 0 → the `not list.is_empty` render_el miscompile).
        if let IrExprKind::BinOp { .. }
        | IrExprKind::UnOp { .. }
        | IrExprKind::RuntimeCall { .. } = &value.kind
        {
            let mark = self.ops.len();
            if let Some(dst) = self.lower_scalar_value(value) {
                self.value_of.insert(var, dst);
                return true;
            }
            self.ops.truncate(mark);
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether an `if`-VALUE runs through the if-marker machinery. `true` means `var` is
    /// bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_if_value(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> bool {
        if let IrExprKind::If { cond, then, else_ } = &value.kind {
            if let Some(dst) = self.try_lower_scalar_if(cond, then, else_, ty) {
                self.value_of.insert(var, dst);
                return true;
            }
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// the ordered `match`-VALUE strategy chain — tuple extract, custom variant, the
    /// Option/Result variant match (which WALLS outside its subset), the single-arm
    /// multi-bind tuple destructure, then the desugared-`if` arm. `Ok(true)` means `var`
    /// is bound and the caller returns `Ok(())` immediately; `Ok(false)` falls through to
    /// the next scalar strategy. Same check ORDER, same rollbacks, same wall.
    fn try_lower_bind_scalar_match(
        &mut self,
        var: VarId,
        ty: &Ty,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        let IrExprKind::Match { subject, arms } = &value.kind else {
            return Ok(false);
        };
        if self.try_lower_bind_scalar_tuple_extract(var, subject, arms) {
            return Ok(true);
        }
        // A CUSTOM variant (user ADT) subject — tag@slot0 dispatch (ADT brick 3).
        // `let v = match t { Num(n) => n, … }`. Without this the ctor-pattern match
        // fell through to a deferred Const 0 (a silent miscompile).
        if let Some(dst) = self.try_lower_custom_variant_match(subject, arms, ty) {
            self.value_of.insert(var, dst);
            return Ok(true);
        }
        if self.try_lower_bind_scalar_variant_match(var, ty, subject, arms)? {
            return Ok(true);
        }
        if self.try_lower_bind_scalar_tuple_destructure_arm(var, ty, subject, arms) {
            return Ok(true);
        }
        if self.try_lower_bind_scalar_desugared_if_arm(var, ty, subject, arms) {
            return Ok(true);
        }
        Ok(false)
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a single-arm tuple-destructure `match` extracting ONE scalar component
    /// loads that component's slot (rolling `ops` back on a miss). `true` means `var` is
    /// bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_tuple_extract(
        &mut self,
        var: VarId,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> bool {
        // A single-arm tuple-destructure `let n = match pair { (_, n) => n }` extracting a
        // SCALAR component — semantically `let n = pair.<i>` (the non-tail tuple-accumulator
        // `fold` cursor extraction). Load the real scalar slot value (a Copy — no ownership).
        if let Some((idx, elem_ty)) = self.tuple_extract_match_index(subject, arms) {
            if !is_heap_ty(&elem_ty) {
                let synth = Self::synth_tuple_index(subject, idx, elem_ty);
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(&synth) {
                    self.value_of.insert(var, dst);
                    return true;
                }
                self.ops.truncate(mark);
            }
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether an Option/Result-subject `match` executes as a tag-read value-match — and
    /// the WALL that a variant subject outside the executable subset takes instead of a
    /// silently-wrong Const-0. `Ok(true)` means `var` is bound and the caller returns
    /// `Ok(())` immediately; `Ok(false)` only for a NON-variant subject.
    fn try_lower_bind_scalar_variant_match(
        &mut self,
        var: VarId,
        ty: &Ty,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> Result<bool, LowerError> {
        // A VARIANT (Option/Result) subject — execute the tag-read value-match
        // (only the taken arm runs, the scalar payload bound). A ctor pattern is not
        // `subj == lit`, so it can't reach `desugar_match_to_if`; without this the
        // result stayed an unset deferred Const (a silent 0).
        if is_variant_ty(&subject.ty) {
            if let Some(dst) = self.try_lower_variant_value_match(subject, arms, ty) {
                self.value_of.insert(var, dst);
                return Ok(true);
            }
            // Outside the executable subset a Const-0 would silently pick a wrong
            // arm — WALL (the discipline: an unfaithful variant match rejects, never
            // emits a deferred 0).
            return Err(LowerError::shaped(
                subject.span,
                WallShape::VariantValueMatch,
                "variant (Option/Result) match bound to a let/var outside the \
                 executable subset cannot be faithfully computed (a Const-0 would \
                 silently pick a wrong arm) not in this brick",
            ));
        }
        Ok(false)
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a single-arm tuple `match` binding MULTIPLE components lowers each from its
    /// layout slot and then the arm body (rolling `ops`/`live_heap_handles` back on a
    /// miss). `true` means `var` is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_tuple_destructure_arm(
        &mut self,
        var: VarId,
        ty: &Ty,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> bool {
        // A single-arm tuple-destructure `let r = match t { (a, b) => <body> }` binding
        // MULTIPLE components (not the single-extract case above): bind each component from its
        // tuple SLOT (the layout-aware loader), then lower the arm body as the bound value.
        // WITHOUT this the multi-bind tuple match fell to the deferred `Const 0` below (a, b
        // read 0). SCALAR result only (a heap arm value needs the merged-result path); rolls
        // back to the Const on a miss.
        if matches!(subject.ty, Ty::Tuple(_))
            && arms.len() == 1
            && arms[0].guard.is_none()
            && matches!(&arms[0].pattern, almide_ir::IrPattern::Tuple { .. })
            && !is_heap_ty(ty)
        {
            // Guard-clause flattening (codopsy7 max-depth sweep): the original nested
            // `if let`/`if` chain is rewritten as `let-else` guards inside a labeled block —
            // any failure `break`s straight to the SAME rollback (`ops`/`live_heap_handles`
            // truncate) the original chain's implicit fall-through reached, then falls
            // through to the next match strategy below. Same check order, same rollback,
            // same success path (`return Ok(())`); pure control-flow rewrite.
            let almide_ir::IrPattern::Tuple { elements } = &arms[0].pattern else {
                unreachable!("matches! above already proved this arm's pattern is Tuple")
            };
            let mark = self.ops.len();
            let lhh = self.live_heap_handles.len();
            'single_arm_tuple: {
                // Materialize the tuple subject as a borrowed handle (its slots are real).
                let Ok(Some(CallArg::Handle(subj))) = self
                    .lower_call_args(std::slice::from_ref(subject))
                    .map(|v| v.into_iter().next())
                else {
                    break 'single_arm_tuple;
                };
                if !self.try_lower_tuple_destructure(elements, subj, Some(&subject.ty)) {
                    break 'single_arm_tuple;
                }
                let Some(dst) = self.lower_scalar_value(&arms[0].body) else {
                    break 'single_arm_tuple;
                };
                self.value_of.insert(var, dst);
                return true;
            }
            self.ops.truncate(mark);
            self.live_heap_handles.truncate(lhh);
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether the `match` desugars to a literal-arm `if` (or binder/guard `Block`) that
    /// `lower_scalar_arm` runs, rolling `ops`/`live_heap_handles` back on a miss. `true`
    /// means `var` is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_desugared_if_arm(
        &mut self,
        var: VarId,
        ty: &Ty,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> bool {
        if let Some(if_expr) = self.desugar_match_to_if(subject, arms, ty) {
            // `If` (literal arms) OR `Block` (`{ let x = subj; if … }` for a
            // binder/guarded arm) — `lower_scalar_arm` runs both; roll back on a miss.
            let mark = self.ops.len();
            let lhh = self.live_heap_handles.len();
            if let Some(dst) = self.lower_scalar_arm(&if_expr) {
                self.value_of.insert(var, dst);
                return true;
            }
            self.ops.truncate(mark);
            self.live_heap_handles.truncate(lhh);
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a scalar `??` executes as a tag read + payload/fallback — and the WALL a
    /// VARIANT operand outside that subset takes instead of a silently-wrong Const-0.
    /// `Ok(true)` means `var` is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_unwrap_or(
        &mut self,
        var: VarId,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        // `let idx = string.index_of(s, x) ?? -1` — a `??` over a materialized Option
        // EXECUTES to a scalar (tag read + payload/fallback), unwrapping the self-host
        // Option[Int] fns; outside the subset a `??` over a VARIANT operand can't read
        // the tag (e.g. a USER-function Option/Result result not yet tracked as
        // materialized) — a Const-0 would be a wrong value, so WALL (never silently 0).
        if let IrExprKind::UnwrapOr { expr, fallback } = &value.kind {
            if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, true) {
                self.value_of.insert(var, dst);
                return Ok(true);
            }
            if is_variant_ty(&expr.ty) {
                return Err(LowerError::Unsupported(
                    "?? over an Option/Result operand outside the executable subset (e.g. a \
                     user-function result not tracked as materialized) cannot be faithfully \
                     computed (a Const-0 would be a wrong value) not in this brick"
                        .into(),
                ));
            }
        }
        Ok(false)
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a bare-`Var` scalar RHS aliases its source value — and whether a MUTABLE
    /// binding instead gets its OWN local via the `+ 0` identity copy. `true` means `var`
    /// is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_var_alias(&mut self, var: VarId, value: &IrExpr) -> bool {
        // `let v = w` aliasing a SCALAR var — v denotes the SAME value (a scalar is freely
        // duplicable: no copy, no ownership). Without this, a bare-Var scalar RHS fell to the
        // deferred `Const` below and silently became 0 (the param-alias zeroing trap).
        //
        // A SCALAR `let/var v = w` ALWAYS gets its own value, seeded with a type-agnostic
        // i64 copy (`v = w + 0` — integer-add of 0 is identity on the i64-uniform bits of
        // Int/Float/Bool). Two mirror-image corruptions forced this, one per direction:
        //   - a MUTABLE `var v = w` that aliased w's local: a later `v = …` would
        //     `SetLocal` w's slot and SILENTLY CORRUPT w (the sha1 `var a = h0; … a = temp`
        //     trap that clobbered h0);
        //   - an immutable `let v = w` where W ITSELF is loop-carried (#1322): the alias
        //     denotes w's stable local, so v reads w's POST-assignment value — the affine
        //     gcd swap `let t = y; y = x % y; x = t` degenerated to x == y (t=0 on every
        //     leg the v1 spine renders; v0/codegen-v3 was correct). "let is never
        //     reassigned" was true but aimed at the wrong side of the alias.
        // The copy anchors the READ at bind position, which is the value semantics both
        // directions need. A HEAP `let v = w` keeps the alias: heap reassignment inside
        // loops/arms is walled or deferred, never SetLocal'd in place.
        if let IrExprKind::Var { id } = &value.kind {
            if let Ok(src) = self.value_for(*id) {
                if !is_heap_ty(&value.ty) {
                    let zero = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: zero, value: 0 });
                    let dst = self.fresh_value();
                    self.ops.push(Op::IntBinOp {
                        dst,
                        op: crate::IntOp::Add,
                        a: src,
                        b: zero,
                    });
                    self.value_of.insert(var, dst);
                } else {
                    self.value_of.insert(var, src);
                }
                return true;
            }
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a SCALAR field/element/global projection loads its real slot value
    /// (rolling `ops` back on a miss). `true` means `var` is bound and the caller returns
    /// `Ok(())` immediately.
    fn try_lower_bind_scalar_projection(&mut self, var: VarId, value: &IrExpr) -> bool {
        // `let d = r.x` / `let d = t.0` / `let d = xs[i]` — a SCALAR field / element
        // projection LOADS the real value from the materialized aggregate's layout slot
        // (the VALUE MODEL); `xs[i]` is a bounds-checked `$elem_addr` load. Outside the
        // materialized subset it rolls back to the deferred `Const`.
        // A `Var` RHS reaches here only when the alias arm above MISSED (`value_for`
        // resolves locals, not globals): `let id = region_count` — a GLOBAL read. The
        // scalar-value path routes it through `value_or_global` (a mutable global's
        // slot Load / an immutable one's const materialization), a fresh dst either
        // way — no alias to protect, so the mutable-binding `+0` copy is not needed.
        if let IrExprKind::Member { .. }
        | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. }
        | IrExprKind::Var { .. } = &value.kind
        {
            let mark = self.ops.len();
            if let Some(dst) = self.lower_scalar_value(value) {
                self.value_of.insert(var, dst);
                return true;
            }
            self.ops.truncate(mark);
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// the TERMINAL deferred `Const` — the value every scalar strategy above declined.
    /// Strict value mode walls it instead; the permissive caps-counting path defers and
    /// still folds the elided calls for capabilities.
    fn lower_bind_scalar_deferred_const(
        &mut self,
        var: VarId,
        value: &IrExpr,
    ) -> Result<(), LowerError> {
        let dst = self.fresh_value();
        self.value_of.insert(var, dst);
        if crate::lower::strict_values() {
            if std::env::var("ALMIDE_BOUNDED_DEBUG").is_ok() {
                eprintln!("[bounded-debug] deferred bind var={var:?} value kind = {}",
                    match &value.kind {
                        IrExprKind::RuntimeCall { symbol, .. } => format!("RuntimeCall {}", symbol.as_str()),
                        IrExprKind::Call { .. } => "Call".to_string(),
                        other => { let d = format!("{other:?}"); d.chars().take(120).collect() },
                    });
            }
            return Err(crate::lower::strict_const_wall("binding"));
        }
        self.ops.push(Op::Const { dst });
        self.record_elided_calls(value);
        Ok(())
    }

}
