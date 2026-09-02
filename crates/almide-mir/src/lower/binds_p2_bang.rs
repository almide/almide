// ── the `!` rule's admission + arm helpers, include!-spliced beside binds_p2.rs ──
//
// Extracted verbatim from `LowerCtx::try_lower_bind_unwrap_return` (codopsy A:
// cog 78 / cc 61 in one frame). Every block below is the text that sat inline,
// taking exactly the values it used and returning exactly what it produced;
// the decline gates are NAMED (`Err(gate)`) and printed by the caller, so the
// ALMIDE_DBG_BANG trace and the emitted ops are byte-identical.

/// The `!` rule's admission verdict: the fn/callee shape flags and the
/// payload class the ok-continuation binds. All `Copy` facts, read once.
#[derive(Clone, Copy)]
pub(crate) struct BangAdmission {
    void_fn: bool,
    callee_is_option: bool,
    callee_fam: crate::lower::ResultFamily,
    rebox: bool,
    rebox_repr: Option<crate::Repr>,
    heap_payload_class: bool,
    adt_payload: bool,
}

/// The fn-side / callee-side shape flags the admission gates read.
struct BangFnShape {
    fn_fam: Option<crate::lower::ResultFamily>,
    void_fn: bool,
    callee_is_option: bool,
    opt_fn: bool,
}

impl LowerCtx {
    /// Verbatim from `try_lower_bind_unwrap_return`: the declared-family /
    /// void-fn / option-callee / option-fn flags (pure ctx reads).
    fn bang_fn_shape(&self, expr: &IrExpr) -> BangFnShape {
        use almide_lang::types::constructor::TypeConstructorId;
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
        BangFnShape { fn_fam, void_fn, callee_is_option, opt_fn }
    }

    /// Verbatim from `try_lower_bind_unwrap_return`: the channel gates, the
    /// family pair, the rebox decision and the payload class. `Err(gate)` is
    /// the decline the caller prints and returns `Ok(false)` for.
    fn bang_return_admission(
        &self,
        ty: &Ty,
        expr: &IrExpr,
        dbg: bool,
    ) -> Result<BangAdmission, &'static str> {
        use almide_lang::types::constructor::TypeConstructorId;
        let BangFnShape { fn_fam, void_fn, callee_is_option, opt_fn } = self.bang_fn_shape(expr);
        if fn_fam.is_none() && !void_fn && !opt_fn {
            return Err("fn-family");
        }
        // A RESULT callee inside an Option-returning fn has an err payload
        // with nowhere to go (returning the carrier would type-pun err as
        // some) — keep its honest wall.
        if opt_fn && !callee_is_option && !void_fn {
            return Err("result-in-option-fn");
        }
        if callee_is_option {
            if !void_fn && !opt_fn && !matches!(self.decl_fn_err, Some(Ty::String)) {
                return Err("option-fn-channel");
            }
        } else if !matches!(&expr.ty, Ty::Applied(TypeConstructorId::Result, _)) {
            return Err("callee-family");
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
                Err(_) => return Err("rebox-repr"),
            }
        } else {
            None
        };
        let (heap_payload_class, adt_payload) =
            self.bang_heap_payload_class(ty, callee_is_option, callee_fam, dbg)?;
        Ok(BangAdmission {
            void_fn,
            callee_is_option,
            callee_fam,
            rebox,
            rebox_repr,
            heap_payload_class,
            adt_payload,
        })
    }

    /// Verbatim from `try_lower_bind_unwrap_return`: the ok-payload class
    /// gate — `(heap_payload_class, adt_payload)`, or the decline gate.
    fn bang_heap_payload_class(
        &self,
        ty: &Ty,
        callee_is_option: bool,
        callee_fam: crate::lower::ResultFamily,
        dbg: bool,
    ) -> Result<(bool, bool), &'static str> {
        use almide_lang::types::constructor::TypeConstructorId;
        // The ok-payload bind classes this slice owns: scalar/Unit (value
        // copy), and for a HeapOk callee a String / flat heap-elem list /
        // Option/Result payload (Dup'd — see below) plus the ADT classes
        // (variant / record / tuple) the ordinary Named-call bind seeds.
        // Anything else (Value, maps, generic records) declines to the wall
        // for the next slice.
        let mut adt_payload = false;
        let heap_payload_class = if is_heap_ty(ty) {
            if !callee_is_option && callee_fam != crate::lower::ResultFamily::HeapOk {
                return Err("heap-payload-scalar-carrier");
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
                return Err("heap-payload-class");
            }
        } else {
            false
        };
        Ok((heap_payload_class, adt_payload))
    }

    /// Verbatim from `try_lower_bind_unwrap_return`: the void fn's abort
    /// message pieces, allocated BEFORE the branch — `(msg, None)` for an
    /// Option callee's static line, `(pre, Some(nl))` for a Result err's.
    fn bang_void_msg_pieces(&mut self, callee_is_option: bool) -> (ValueId, Option<ValueId>) {
        if callee_is_option {
            let msg = self.fresh_value();
            self.ops.push(Op::Alloc {
                dst: msg,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: crate::Init::Str("Error: none\n".into()),
            });
            self.live_heap_handles.push(msg);
            (msg, None)
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
            (pre, Some(nl))
        }
    }

    /// Verbatim from `try_lower_bind_unwrap_return`: the err/none EXIT arm
    /// (between the caller's `IfThen` and `EndIf`) — abort, rebox+return, or
    /// the same-family carrier return.
    fn emit_bang_exit_arm(
        &mut self,
        h: ValueId,
        v: ValueId,
        adm: &BangAdmission,
        void_msg_pieces: Option<(ValueId, Option<ValueId>)>,
    ) {
        use crate::PrimKind;
        let BangAdmission { void_fn, callee_is_option, rebox_repr, .. } = *adm;
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
    }

    /// Verbatim from `try_lower_bind_unwrap_return`: the straight-line ok
    /// continuation — bind the payload (Unit / heap Dup + seeding / scalar copy).
    fn bind_bang_ok_payload(&mut self, var: VarId, ty: &Ty, h: ValueId, adm: &BangAdmission) {
        use crate::PrimKind;
        let BangAdmission { heap_payload_class, adt_payload, .. } = *adm;
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
    }
}
