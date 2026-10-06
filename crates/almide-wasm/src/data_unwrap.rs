//! The `!` / `?` extraction marker (`lower_try_unwrap`) and the ok-path
//! release of an owned carrier — split from data.rs for the file budget.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::witness_unwrap::Leaves;
use crate::*;

impl Emitter<'_> {
            // `!` — three enclosing shapes (the interp's eval_try_unwrap):
            //   effect fn  -> PROPAGATE (return the err block as-is; err
            //                 blocks of any Result(_, E) share one layout),
            //   main       -> ABORT with the native frame
            //                 ("Error: {msg}" + exit 1),
            //   pure fn    -> same abort (the checker forbids propagating
            //                 `!` outside effect fns; a pure-Option/Result
            //                 fn's `!` is #1410-propagating — refused).
            // `?` (Try) and `!` (Unwrap) are ONE marker in the oracle:
            // eval.rs dispatches Try | Unwrap to the same eval_try_unwrap.
    /// #2509 — the OK path of `expr!` releases the carrier it read the
    /// payload out of, when that carrier is an OWNED anonymous temporary.
    ///
    /// The err path already moves an owned carrier out (no `$inc` above);
    /// the ok path kept neither half of the pair, so every extraction from
    /// an unowned-by-anyone carrier leaked the 16 B block AND the credit it
    /// held on the payload. The usual `f(x)!` never showed it: arg_temps
    /// parks a Result/Option-TYPED operand in a local, and that local's dec
    /// releases both. A MOVE-MODE effect call (mut_param.rs) is typed with
    /// the RAW payload, so the park never fires and the carrier the wasm ABI
    /// built had no owner at all — `poke(buf, v)!` grew `buf`'s count by one
    /// per call, the latent leak #2503's rc-gated copy turned into an OOM.
    ///
    /// WHICH RULE, and why it is not a free-too-early. `$dec_flat` releases
    /// the carrier SPINE ONLY: the payload handle was loaded before it and
    /// is a value on the stack, not a pointer into the block, and the flat
    /// dec never recurses into the payload slot. So the carrier's ONE credit
    /// on the payload transfers to the extracted value — the same move the
    /// err path makes with the whole block — and the node is marked owned so
    /// no consumer takes the borrowed-source `+1` on top of it. The two
    /// cases the brief separates coincide under this rule: an argument
    /// FOLDED INTO the result (the move-mode buffer) arrives holding the
    /// carrier's credit and the write-back's `rc_share_guard` pays for the
    /// caller's own second holder, while an argument still LIVE after the
    /// call keeps the credit its own binding has held all along — neither
    /// one loses a holder here, because nothing but the carrier is released.
    /// A borrowed carrier (`rc_owned_result` false: a parked local, a var)
    /// is left exactly as before, so no route can free a block twice.
    /// #2755: main's `!` ABORT (`Error: {msg}`, exit 1) — and an
    /// out-of-bounds index — as a recorded TERMINAL. The message address is
    /// on the stack. The process ends here (the exit import, then
    /// `unreachable`), so no credit of the frame is released: the exit
    /// ledger records an `Abort` plan that releases nothing (E083 pins that
    /// the window reaches the `unreachable` with no release in it) and the
    /// witness ends the path with the checker's abort terminal, which
    /// discharges every outstanding credit (`CBranchAbort`, format v6). The
    /// bytes are the plain abort frame's.
    pub(crate) fn abort_frame(&mut self) {
        let plan = self.exit_plan(crate::exit_plan::Continuation::Abort);
        self.emit_exit(&plan);
        self.emit_error_frame_abort();
    }

    fn release_ok_carrier(&mut self, e: &IrExpr, owned_carrier: bool) {
        if !owned_carrier {
            return;
        }
        self.f.instructions().local_get(self.scr_i32_local).call(F_DEC_FLAT);
        // The extraction now hands its consumer one credit: the bind,
        // assign, store and argument routes must not add another.
        self.owned_call_marks.mark(e);
    }

    /// `List[String]` — the error type native `!` JOINS into a String
    /// channel (`map_err_join`) instead of rendering its repr
    /// ([`Self::propagate_err_joined`]).
    fn is_str_list(&self, t: SliceTy) -> bool {
        matches!(t, SliceTy::List(h) if self.types.el(h) == STR)
    }

    pub(crate) fn lower_try_unwrap(
        &mut self,
        e: &IrExpr,
        expr: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        Ok({

                // C-216: a marker node TYPED Option is the effect-RESULT-
                // layer strip on a declared-Option effect call — identity.
                let node_ty = slice_ty_of(&e.ty, self.types);
                // Propagation returns the operand's err block INTO the
                // enclosing frame — sound only when the err slot types
                // agree (they share one layout then).
                let fn_err = match self.fn_ret {
                    Some(SliceTy::Result(_, fe)) => Some(self.types.el(fe)),
                    _ => None,
                };
                let in_effect = fn_err.is_some();
                // A raise leaf `err(m)!` in main (the C-132 err-carrying
                // `!` site re-raises its message this way, #2917) aborts with
                // `m` directly: the err block it names would only be built to
                // be read back.
                if !in_effect
                    && self.in_main
                    && let IrExprKind::ResultErr { expr: m } = &crate::data::err_channel::through_empty_blocks(expr).kind
                    && slice_ty_of(&m.ty, self.types) == Some(STR)
                    && let Some(node) = node_ty
                {
                    self.lower(m, Some(STR))?;
                    self.witness_abort_message(m);
                    self.abort_frame();
                    // No value ever leaves: a consumer takes no credit of it.
                    self.owned_call_marks.mark(e);
                    return Ok(node);
                }
                // #1067: `!` in a pure Option-returning fn PROPAGATES a
                // none as none (a Result operand there stays refused —
                // no oracle row pins its shape).
                let in_option_fn =
                    !in_effect && matches!(self.fn_ret, Some(SliceTy::Option(_)));
                if in_option_fn {
                    match self.lower(expr, None)? {
                        SliceTy::Option(h) => {
                            let et = self.types.el(h);
                            let owned_carrier = self.rc_owned_result(expr);
                            self.f
                                .instructions()
                                .local_tee(self.scr_i32_local)
                                .i32_eqz()
                                .if_(BlockType::Empty);
                            let wc = self.witness_unwrap_open(expr, owned_carrier, false);
                            self.witness_unwrap_propagate(wc, false);
                            let plan = self.exit_plan(crate::exit_plan::Continuation::ReturnError);
                            self.emit_exit(&plan);
                            self.f
                                .instructions()
                                .i32_const(almide_layout::NULL_ADDR as i32)
                                .return_()
                                .end()
                                .local_get(self.scr_i32_local);
                            self.witness_unwrap_exit(wc, Leaves::Nothing);
                            self.witness_unwrap_close();
                            self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                            self.release_ok_carrier(e, owned_carrier);
                            self.witness_unwrap_ok(wc, owned_carrier);
                            return Ok(et);
                        }
                        _ => return unsup("unwrap-propagating"),
                    }
                }
                match self.lower(expr, None)? {
                    got @ SliceTy::Option(_)
                        if node_ty == Some(got) =>
                    {
                        // Identity: pass the Option through untouched.
                        got
                    }
                    SliceTy::Option(h) => {
                        let et = self.types.el(h);
                        let owned_carrier = self.rc_owned_result(expr);
                        self.f
                            .instructions()
                            .local_tee(self.scr_i32_local)
                            .i32_eqz()
                            .if_(BlockType::Empty);
                        let wc = self.witness_unwrap_open(expr, owned_carrier, false);
                        if in_effect {
                            if fn_err != Some(STR) {
                                return unsup("unwrap-none-err-ty");
                            }
                            // err("none") — #556: `!` on none propagates
                            // an Err whose message is "none".
                            let none_msg = self.pool.intern("none");
                            self.f
                                .instructions()
                                .i32_const(16)
                                .call(F_ALLOC)
                                .local_tee(self.tmp_i32_local)
                                .i32_const(1)
                                .i32_store(slot_memarg(almide_layout::SUM_TAG))
                                .local_get(self.tmp_i32_local)
                                .i32_const(none_msg as i32)
                                .i32_store(slot_memarg(almide_layout::SUM_FIELD));
                            self.witness_unwrap_propagate(wc, false);
                            let plan = self.exit_plan(crate::exit_plan::Continuation::ReturnError);
                            self.emit_exit(&plan);
                            self.f.instructions().local_get(self.tmp_i32_local).return_();
                            self.witness_unwrap_exit(wc, Leaves::Fresh);
                        } else if self.in_main {
                            let none_msg = self.pool.intern("none");
                            self.f.instructions().i32_const(none_msg as i32);
                            self.abort_frame();
                        } else {
                            self.witness_unwrap_decline("trap");
                            self.f.instructions().unreachable();
                        }
                        self.f.instructions().end().local_get(self.scr_i32_local);
                        self.witness_unwrap_close();
                        self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                        self.release_ok_carrier(e, owned_carrier);
                        self.witness_unwrap_ok(wc, owned_carrier);
                        et
                    }
                    SliceTy::Result(o, er) => {
                        let et = self.types.el(o);
                        let ert = self.types.el(er);
                        // Who owns the CARRIER block this extraction reads?
                        // An owned operand is an anonymous temporary this
                        // frame holds the only credit of; a borrowed one is
                        // a local (arg_temps parks the usual `f(x)!` carrier
                        // in one) that some other route releases.
                        let owned_carrier = self.rc_owned_result(expr);
                        self.f
                            .instructions()
                            .local_tee(self.scr_i32_local)
                            .i32_load(slot_memarg(almide_layout::SUM_TAG))
                            .i32_const(0)
                            .i32_ne()
                            .if_(BlockType::Empty);
                        let wc = self.witness_unwrap_open(expr, owned_carrier, true);
                        if in_effect && fn_err == Some(STR) && ert != STR && !self.is_str_list(ert) {
                            // ADR-0021 D2 / #2725: a typed error `!`-ed into a
                            // String channel — the channel carries its repr text.
                            // The error's IR type picks its digits (a UInt64 reads
                            // unsigned, a Float32 prints binary32 — #3187).
                            let err_ir = crate::display::ir_arg(Some(&expr.ty), 1).cloned();
                            self.propagate_err_as_repr(SliceTy::Result(o, er), ert, err_ir.as_ref(), owned_carrier, wc)?;
                        } else if in_effect && fn_err == Some(STR) && self.is_str_list(ert) {
                            self.witness_unwrap_decline("err-joined");
                            self.propagate_err_joined(SliceTy::Result(o, er), ert, owned_carrier)?;
                        } else if in_effect {
                            if fn_err != Some(ert) {
                                return unsup("unwrap-err-ty-mismatch");
                            }
                            // The propagated block is the operand's: a BORROWED
                            // operand (a local the exit below releases) hands the
                            // caller a share; an owned temporary moves out.
                            if !owned_carrier {
                                self.f.instructions().local_get(self.scr_i32_local).call(F_INC);
                            }
                            self.witness_unwrap_propagate(wc, !owned_carrier);
                            let plan = self.exit_plan(crate::exit_plan::Continuation::ReturnError);
                            self.emit_exit(&plan);
                            self.f.instructions().local_get(self.scr_i32_local).return_();
                            self.witness_unwrap_exit(wc, Leaves::Carrier);
                        } else if self.in_main && ert == STR {
                            self.f.instructions().local_get(self.scr_i32_local);
                            self.load_ty_slot(ert, almide_layout::SUM_FIELD);
                            self.abort_frame();
                        } else {
                            self.witness_unwrap_decline("trap");
                            self.f.instructions().unreachable();
                        }
                        self.f.instructions().end().local_get(self.scr_i32_local);
                        self.witness_unwrap_close();
                        self.load_ty_slot(et, almide_layout::SUM_FIELD);
                        self.release_ok_carrier(e, owned_carrier);
                        self.witness_unwrap_ok(wc, owned_carrier);
                        et
                    }
                    other => return unsup(&format!("unwrap-of:{other:?}")),
                }
        })
    }
}
