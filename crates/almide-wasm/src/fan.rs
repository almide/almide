//! The deterministic fan combinators (C-004/C-005): sequential, LIST
//! ORDER, never wall-clock — semantics verbatim from the self-hosted
//! stdlib bodies (fan_map.almd / fan_any.almd) and the interp's
//! eval_fan_any / eval_fan:
//!   - fan.map(xs, f): collect oks in order, the FIRST err IS the result.
//!   - fan.any(xs, f): first Ok wins, an element's err skips it,
//!     all-fail (and empty) is the ledger-constant Err.
//!   - fan.any { arms }: one literal list of 0-ary thunks — first Ok
//!     short-circuits; a PURE arm's value Ok-adapts and wins (#514).
//!   - `fan { a; b }` (IrExprKind::Fan): every arm evaluates in order,
//!     payloads unwrap; the FIRST err aborts (after all arms ran) with
//!     the bare message; one arm = the bare value, else a tuple.

use almide_ir::{CallTarget, IrExpr, IrExprKind};
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// `fan.*` module calls. Ok(None) = not handled here.
    pub(crate) fn lower_fan_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        // ALMIDE_DBG_FAN=1: name the route each fan call takes (the
        // ALMIDE_DBG_ELEM precedent) — the p3 prefetch test asserts the
        // "prefetch" line so a pattern regression cannot pass silently
        // (the import-section assertion alone is vacuous: to_p3 declares
        // the async imports unconditionally).
        let dbg = std::env::var_os("ALMIDE_DBG_FAN").is_some();
        let out = match (func, args) {
            // The PREFETCH form (#1628 increment 2b): a map whose whole
            // arm body is one fs.read_text on the element starts every
            // read up front (op 40) and awaits in arm order (op 41) —
            // sequential semantics verbatim (C-004 list order, first err
            // is the result), but a host that can overlap I/O (the p3
            // component) serves the reads concurrently. Hosts that
            // cannot treat start as a no-op.
            ("map", [xs, cb]) if body_is_fs_read_text(cb) => {
                if dbg {
                    eprintln!("[fan-dbg] fan.map: prefetch lowering engaged");
                }
                Some(self.lower_fan_map_fs_prefetch(xs)?)
            }
            ("map" | "any" | "any_map", [xs, cb]) => {
                if dbg {
                    eprintln!("[fan-dbg] fan.{func}: sequential accumulator");
                }
                let first_ok_wins = func != "map";
                let (params, body) = self.hof_lambda(cb, 1)?;
                // ADR-0006 (#1663): the callback's top-level `!` IS the
                // fallible form's instantiation — the accumulator below
                // performs exactly its first-err semantics, so the Try
                // layer strips instead of propagating into the caller's
                // frame (which aborted main where native carries a value).
                let body = strip_callback_try(body);
                let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
                // The callback body VALUE is a Result block.
                let hr = self.hold_i32()?;
                let hacc = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.i32_const(0).local_set(hr);
                    i.i32_const(0).call(F_ALLOC).local_set(hacc);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                }
                self.hof_elem_into(elem, bh, ch, ih, params[0]);
                let got = self.lower(body, None)?;
                let SliceTy::Result(o, er) = got else {
                    return unsup(&format!("fan-{func}-body:{got:?}"));
                };
                let b = self.types.el(o);
                {
                    let mut i = self.f.instructions();
                    i.local_set(hr);
                    i.local_get(hr).i32_load(slot_memarg(almide_layout::SUM_TAG));
                    if first_ok_wins {
                        // err → skip this element; ok → hr wins, break.
                        i.i32_eqz().br_if(1);
                        i.i32_const(0).local_set(hr);
                    } else {
                        // err → hr IS the whole result, break.
                        i.i32_const(0).i32_ne().br_if(1);
                    }
                }
                if !first_ok_wins {
                    // collect the ok payload
                    self.f.instructions().local_get(hacc).local_get(hr);
                    self.load_ty_slot(b, almide_layout::SUM_FIELD);
                    if b.val_type() == ValType::F64 {
                        self.f.instructions().i64_reinterpret_f64();
                    }
                    let push = match b.slot_size() {
                        8 => F_LIST_PUSH_8,
                        _ => F_LIST_PUSH_4,
                    };
                    self.f.instructions().call(push).local_set(hacc);
                    self.f.instructions().i32_const(0).local_set(hr);
                }
                self.hof_step(ih);
                // loop fell through (no break): all elements consumed.
                {
                    let mut i = self.f.instructions();
                    i.local_get(hr).i32_eqz().if_(BlockType::Empty);
                    if first_ok_wins {
                        // ledger-constant all-fail Err
                        let msg = self.pool.intern("fan.any: all candidates failed");
                        i.i32_const(16)
                            .call(F_ALLOC)
                            .local_tee(hr)
                            .i32_const(1)
                            .i32_store(slot_memarg(almide_layout::SUM_TAG));
                        i.local_get(hr)
                            .i32_const(msg as i32)
                            .i32_store(slot_memarg(almide_layout::SUM_FIELD));
                    } else {
                        // ok(acc)
                        i.i32_const(16)
                            .call(F_ALLOC)
                            .local_tee(hr)
                            .i32_const(0)
                            .i32_store(slot_memarg(almide_layout::SUM_TAG));
                        i.local_get(hr)
                            .local_get(hacc)
                            .i32_store(slot_memarg(almide_layout::SUM_FIELD));
                    }
                    i.end();
                    i.local_get(hr);
                }
                for _ in 0..5 {
                    self.release_i32();
                }
                Some(if first_ok_wins {
                    SliceTy::Result(o, er)
                } else {
                    let lb = self.types.intern(SliceTy::List(self.types.intern(b)));
                    SliceTy::Result(lb, er)
                })
            }
            // Block form: ONE literal list of 0-ary thunks, statically
            // unrolled — first Ok short-circuits, a pure arm Ok-adapts
            // and wins, all-fail is the ledger Err.
            ("any", [thunks]) => {
                let IrExprKind::List { elements } = &thunks.kind else {
                    return unsup("fan-any-nonliteral-thunks");
                };
                let hr = self.hold_i32()?;
                self.f.instructions().block(BlockType::Empty);
                let mut result_ty: Option<SliceTy> = None;
                for arm in elements {
                    let (_p, body) = self.hof_lambda(arm, 0)?;
                    let body = strip_callback_try(body);
                    let got = self.lower(body, None)?;
                    match got {
                        SliceTy::Result(..) => {
                            let mut i = self.f.instructions();
                            i.local_set(hr);
                            i.local_get(hr)
                                .i32_load(slot_memarg(almide_layout::SUM_TAG))
                                .i32_eqz()
                                .br_if(0);
                            result_ty.get_or_insert(got);
                        }
                        pure => {
                            // Ok-adapt the bare value; evaluation stops.
                            let hv = self.hold_val(pure)?;
                            self.f.instructions().local_set(hv);
                            self.f
                                .instructions()
                                .i32_const(16)
                                .call(F_ALLOC)
                                .local_tee(hr)
                                .i32_const(0)
                                .i32_store(slot_memarg(almide_layout::SUM_TAG));
                            self.f.instructions().local_get(hr).local_get(hv);
                            self.store_ty_slot(pure, almide_layout::SUM_FIELD);
                            self.release_val(pure);
                            self.f.instructions().br(0);
                            result_ty
                                .get_or_insert(SliceTy::Result(self.types.intern(pure), {
                                    self.types.intern(STR)
                                }));
                        }
                    }
                }
                {
                    let msg = self.pool.intern("fan.any: all candidates failed");
                    let mut i = self.f.instructions();
                    i.i32_const(16)
                        .call(F_ALLOC)
                        .local_tee(hr)
                        .i32_const(1)
                        .i32_store(slot_memarg(almide_layout::SUM_TAG));
                    i.local_get(hr)
                        .i32_const(msg as i32)
                        .i32_store(slot_memarg(almide_layout::SUM_FIELD));
                    i.end();
                    i.local_get(hr);
                }
                self.release_i32();
                let Some(t) = result_ty else {
                    return unsup("fan-any-armless");
                };
                Some(t)
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }

    /// `fan { a; b; … }` — every arm runs in order, ok payloads unwrap;
    /// the FIRST err aborts AFTER all arms evaluated (the interp's
    /// eval_fan order), with the BARE String message. One arm = the bare
    /// value; several = a tuple of payloads.
    pub(crate) fn lower_fan_block(&mut self, exprs: &[IrExpr]) -> Result<SliceTy, EmitError> {
        let herr = self.hold_i32()?;
        self.f.instructions().i32_const(0).local_set(herr);
        let mut vals: Vec<(u32, SliceTy)> = Vec::new();
        for arm in exprs {
            let got = self.lower(arm, None)?;
            match got {
                SliceTy::Result(o, er) => {
                    if self.types.el(er) != STR {
                        return unsup("fan-block-err-ty");
                    }
                    let p = self.types.el(o);
                    let hv = self.hold_val(p)?;
                    let ha = self.hold_i32()?;
                    {
                        let mut i = self.f.instructions();
                        i.local_set(ha);
                        i.local_get(ha)
                            .i32_load(slot_memarg(almide_layout::SUM_TAG))
                            .i32_const(0)
                            .i32_ne();
                        i.local_get(herr).i32_eqz();
                        i.i32_and().if_(BlockType::Empty);
                        i.local_get(ha);
                    }
                    self.load_ty_slot(STR, almide_layout::SUM_FIELD);
                    self.f.instructions().local_set(herr).end();
                    self.f.instructions().local_get(ha);
                    self.load_ty_slot(p, almide_layout::SUM_FIELD);
                    self.f.instructions().local_set(hv);
                    self.release_i32();
                    vals.push((hv, p));
                }
                pure => {
                    let hv = self.hold_val(pure)?;
                    self.f.instructions().local_set(hv);
                    vals.push((hv, pure));
                }
            }
        }
        // first err → the bare-message abort frame
        self.f.instructions().local_get(herr).if_(BlockType::Empty);
        self.f.instructions().local_get(herr);
        self.emit_error_frame_abort();
        self.f.instructions().end();
        let out = if vals.len() == 1 {
            let (hv, p) = vals[0];
            self.f.instructions().local_get(hv);
            p
        } else {
            let tys: Vec<SliceTy> = vals.iter().map(|(_, p)| *p).collect();
            let ti = self.types.tuple(tys);
            let def = self.types.tuple_def(ti);
            let hb = self.hold_i32()?;
            self.f.instructions().i32_const(def.size as i32).call(F_ALLOC).local_set(hb);
            for ((hv, p), (fty, off)) in vals.iter().zip(def.fields.clone()) {
                debug_assert_eq!(*p, fty);
                self.f.instructions().local_get(hb).local_get(*hv);
                self.store_ty_slot(*p, off);
            }
            self.f.instructions().local_get(hb);
            self.release_i32();
            SliceTy::Tuple(ti)
        };
        for (_, p) in vals.iter().rev() {
            self.release_val(*p);
        }
        self.release_i32();
        Ok(out)
    }

    /// The two-phase prefetch map (see `body_is_fs_read_text`): phase A
    /// starts slot k per element, phase B awaits every slot in arm
    /// order. Phase B never breaks early — after the first err the
    /// remaining awaits run and their results are DISCARDED, so no
    /// started read outlives the fan (the p3 shim relies on this to
    /// leave no subtask outstanding at task exit).
    fn lower_fan_map_fs_prefetch(
        &mut self,
        xs: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        if elem != STR {
            return unsup(&format!("fan-prefetch-elem:{elem:?}"));
        }
        let hp = self.hold_i32()?;
        // ── phase A: start every read ──
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, hp);
        {
            let mut i = self.f.instructions();
            i.i32_const(40);
            i.local_get(hp).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hp).i32_load(len_memarg());
            i.i32_const(0).local_get(ih);
            i.call(F_FS_CALL).drop();
        }
        self.hof_step(ih);
        // ── phase B: await in arm order, fan.map accumulation ──
        let hr = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let h64 = self.hold_i64()?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).local_set(hr);
            i.i32_const(0).call(F_ALLOC).local_set(hacc);
            i.i32_const(0).local_set(ih);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.hof_elem_into(elem, bh, ch, ih, hp);
        {
            let mut i = self.f.instructions();
            i.i32_const(41);
            i.local_get(hp).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hp).i32_load(len_memarg());
            i.i32_const(0).local_get(ih);
            i.call(F_FS_CALL).local_set(h64);
            // an err already decided the result: drain only
            i.local_get(hr).i32_eqz().if_(BlockType::Empty);
            i.local_get(h64);
        }
        let got = self.fs_result_string()?;
        debug_assert!(matches!(got, SliceTy::Result(..)));
        {
            let mut i = self.f.instructions();
            i.local_set(hp);
            i.local_get(hp).i32_load(slot_memarg(almide_layout::SUM_TAG));
            i.if_(BlockType::Empty);
            // first err IS the whole result
            i.local_get(hp).local_set(hr);
            i.else_();
            i.local_get(hacc).local_get(hp);
            i.i32_load(slot_memarg(almide_layout::SUM_FIELD));
            i.call(F_LIST_PUSH_4).local_set(hacc);
            i.end();
            i.end();
        }
        self.hof_step(ih);
        // finale: no err → ok(acc)
        {
            let mut i = self.f.instructions();
            i.local_get(hr).i32_eqz().if_(BlockType::Empty);
            i.i32_const(16)
                .call(F_ALLOC)
                .local_tee(hr)
                .i32_const(0)
                .i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hr)
                .local_get(hacc)
                .i32_store(slot_memarg(almide_layout::SUM_FIELD));
            i.end();
            i.local_get(hr);
        }
        for _ in 0..6 {
            self.release_i32();
        }
        self.release_i64();
        let sh = self.types.intern(STR);
        let lh = self.types.intern(SliceTy::List(sh));
        Ok(SliceTy::Result(lh, sh))
    }
}

/// The fan callback's top-level `!` strips (#1663). The frontend spells
/// the fallible form's instantiation as `ok(unwrap(expr))` around the
/// callback body (observed IR; `Try{expr}` kept for robustness): for a
/// Result-typed `expr` that wrapper is the identity, and the emitter's
/// `Unwrap` lowering is ENCLOSING-FUNCTION propagation — inlined into
/// the caller it aborted main where native carries the err into the
/// fan's per-element Result. The fan accumulator IS the fallible form's
/// first-err semantics, so the wrapper strips to the raw Result expr.
fn strip_callback_try(body: &IrExpr) -> &IrExpr {
    match &body.kind {
        IrExprKind::Try { expr } => expr.as_ref(),
        IrExprKind::ResultOk { expr } => match &expr.kind {
            IrExprKind::Unwrap { expr: inner }
                if matches!(&inner.ty,
                            Ty::Applied(c, _)
                            if matches!(c, TypeConstructorId::Result)) =>
            {
                inner.as_ref()
            }
            _ => body,
        },
        _ => body,
    }
}

/// `(p) => fs.read_text(p)` — possibly under the callback's `!` (one
/// `Try` layer): the shape whose Result flows straight into the fan
/// protocol, so prefetching the read is unobservable.
fn body_is_fs_read_text(cb: &IrExpr) -> bool {
    let IrExprKind::Lambda { params, body, .. } = &cb.kind else {
        return false;
    };
    let [(param, _)] = params.as_slice() else {
        return false;
    };
    let body = strip_callback_try(body);
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } =
        &body.kind
    else {
        return false;
    };
    module.as_str() == "fs"
        && func.as_str() == "read_text"
        && matches!(args.as_slice(),
                    [IrExpr { kind: IrExprKind::Var { id }, .. }] if id == param)
}
