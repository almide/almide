//! The fallible `fs.fold_lines` (#1806), split from fs_meta.rs for the file
//! budget: per line the callback yields Result[acc, String] and the first
//! err is the whole result. A canonical body inlines; a compound one is
//! called as the closure it is.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use super::body_propagates;
use crate::*;

impl Emitter<'_> {
    /// The fallible fold: read_lines frames; per line the callback yields
    /// Result[acc, String] — the first err becomes the WHOLE result and
    /// later lines never see the callback (a done-flag guards the walk).
    pub(crate) fn lower_fs_fallible_fold(
        &mut self,
        p: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        let Some(acc_ty) = slice_ty_of(&init.ty, self.types) else {
            return unsup(&format!("fs-fallible-fold-acc:{}", ty_name(&init.ty)));
        };
        // #1806: the canonical callback (`(a, l) => step(a, l)!`) arrives
        // with its marker stripped — the body IS the Result call, so it
        // inlines into this frame. A COMPOUND body (`{ …; step(a, l)! }`)
        // keeps its `!`s: it is a real fallible closure with its own
        // channel, and inlining it would route those `!`s into the
        // ENCLOSING fn's carrier — the err escaped `main` where native
        // bound it. Such a body is called as the closure it is.
        if body_propagates(cb) {
            return self.lower_fs_fallible_fold_closure(p, init, cb, acc_ty);
        }
        let (params, body) = self.hof_lambda(cb, 2)?;
        self.lower_arg(init, Some(acc_ty), ArgMode::Retain)?;
        self.f.instructions().local_set(params[0]);
        self.fs_call_1(p, crate::fs::OP_FOLD_LINES)?;
        let (hraw, hlen, herr, hmsg) = self.fs_stream_frames_or_err()?;
        let hr = self.hold_i32()?;
        self.f.instructions().i32_const(0).local_set(hr);
        // #3137: the body's Result carrier. An owned body (the canonical
        // `step(a, l)` call) hands the walk its one credit: an ok payload
        // moves into the accumulator and the shell is released; an err IS
        // the result. A borrowed body's carrier is shared instead.
        let owned = self.rc_owned_result(body);
        let acc_dec = self.elem_is_handle(acc_ty).then(|| self.dec_fn_of(acc_ty));
        let hres = self.hold_i32()?;
        let record = self.witness_fallible_walk(Some((params[0], acc_ty)), owned);
        let heap_acc = record && acc_dec.is_some();
        let act = if heap_acc {
            crate::fs::witness_walkers::WalkAcc::Outer(params[0])
        } else {
            crate::fs::witness_walkers::WalkAcc::Carried(None)
        };
        self.fs_frames_foreach_borrowed(hraw, hlen, Some((cb, act)), |em, _| {
            em.f.instructions().local_set(params[1]);
            // once an err landed, the callback never runs again
            em.f.instructions().local_get(hr).i32_eqz().if_(BlockType::Empty);
            em.witness_fallible_open(record);
            em.lower(body, None)?;
            let wc = em.witness_fallible_carrier(record);
            let mut i = em.f.instructions();
            i.local_set(hres);
            i.local_get(hres).i32_load(slot_memarg(almide_layout::SUM_TAG)).i32_eqz();
            i.if_(BlockType::Empty);
            if let Some(dec) = acc_dec {
                i.local_get(params[0]).call(dec);
            }
            i.local_get(hres);
            let _ = i;
            em.load_ty_slot(acc_ty, almide_layout::SUM_FIELD);
            if !owned {
                em.share_handle_top(acc_ty);
            }
            em.f.instructions().local_set(params[0]);
            if owned {
                em.f.instructions().local_get(hres).call(F_DEC_FLAT);
            }
            em.f.instructions().else_().local_get(hres);
            if !owned {
                em.rc_inc_top();
            }
            let mut i = em.f.instructions();
            i.local_set(hr);
            i.end();
            i.end();
            em.witness_fallible_close(wc, true, heap_acc.then_some(params[0]));
            Ok(())
        })?;
        self.release_i32();
        self.fs_frames_release_raw(hraw, herr);
        self.fs_stream_land_err(hmsg, hr, true);
        self.witness_fallible_result(heap_acc, params[0], acc_ty);
        let hs = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(herr).if_(BlockType::Result(ValType::I32));
            if let Some(dec) = acc_dec {
                i.local_get(params[0]).call(dec);
            }
            i.local_get(herr);
            i.else_();
            i.local_get(hr).if_(BlockType::Result(ValType::I32));
            if let Some(dec) = acc_dec {
                i.local_get(params[0]).call(dec);
            }
            i.local_get(hr);
            i.else_();
            i.i32_const(16)
                .call(F_ALLOC)
                .local_tee(hs)
                .i32_const(0)
                .i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hs).local_get(params[0]);
        }
        self.store_ty_slot(acc_ty, almide_layout::SUM_FIELD);
        self.f.instructions().local_get(hs).end().end();
        for _ in 0..6 {
            self.release_i32();
        }
        Ok(SliceTy::Result(self.types.intern(acc_ty), self.types.intern(STR)))
    }

    /// The compound-callback fold (#1806): the lambda is lowered ONCE as a
    /// closure value and called per line through the funcref table, so a
    /// `!` inside its body propagates through the closure's own Result
    /// channel — `err(e)` is the callback's answer, which ends the walk and
    /// becomes the fold's result, exactly as the inline canonical form and
    /// both other legs answer.
    pub(crate) fn lower_fs_fallible_fold_closure(
        &mut self,
        p: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
        acc_ty: SliceTy,
    ) -> Result<SliceTy, EmitError> {
        let got = self.lower_arg(cb, None, ArgMode::Borrow)?;
        let SliceTy::Fn(sig) = got else {
            return unsup(&format!("fs-fallible-fold-callee-{got:?}"));
        };
        let def = self.types.fn_sig_def(sig);
        if def.params.len() != 2 {
            return unsup("fs-fallible-fold-arity");
        }
        let (p_acc, p_line) = (def.params[0], def.params[1]);
        let mut ps: Vec<ValType> = vec![ValType::I32];
        ps.extend(def.params.iter().map(|t| t.val_type()));
        let ti = self.work.itype(ps, def.ret.map(SliceTy::val_type));
        let hcl = self.hold_i32()?;
        self.f.instructions().local_set(hcl);
        let hacc = self.hold_for(acc_ty)?;
        self.lower_arg(init, Some(acc_ty), ArgMode::Retain)?;
        self.f.instructions().local_set(hacc);
        self.fs_call_1(p, crate::fs::OP_FOLD_LINES)?;
        let (hraw, hlen, herr, hmsg) = self.fs_stream_frames_or_err()?;
        let hr = self.hold_i32()?;
        self.f.instructions().i32_const(0).local_set(hr);
        let acc_droppable = self.rc_droppable(p_acc);
        let line_droppable = self.rc_droppable(p_line);
        // #3137: the walk keeps its own credit on the accumulator (the
        // callee got a +1 of its own), so an ok rebind releases the
        // replaced one; the call's Result carrier is owned — its payload
        // moves into the accumulator and the shell is released.
        let acc_dec = self.elem_is_handle(acc_ty).then(|| self.dec_fn_of(acc_ty));
        let hline = self.hold_i32()?;
        let hres = self.hold_i32()?;
        // #2755: the walk holds the accumulator in `hacc` — a frame owner
        // bound before the loop, as the inline fold's param is.
        let record = self.witness_fallible_walk(Some((hacc, acc_ty)), true);
        let heap_acc = record && acc_dec.is_some();
        let act = if heap_acc {
            crate::fs::witness_walkers::WalkAcc::Outer(hacc)
        } else {
            crate::fs::witness_walkers::WalkAcc::Carried(None)
        };
        self.fs_frames_foreach_borrowed(hraw, hlen, Some((cb, act)), |em, line| {
            em.f.instructions().local_set(hline);
            em.f.instructions().local_get(hr).i32_eqz().if_(BlockType::Empty);
            em.witness_fallible_open(record);
            // Closure convention (calls.rs): env first, then the args —
            // each a borrowed view, so the RC-3 callee-owned guard +1s
            // it (the closure's epilogue decs its params).
            em.f.instructions().local_get(hcl);
            em.f.instructions().local_get(hacc);
            if acc_droppable {
                em.rc_inc_top();
                em.witness_walk_share(heap_acc.then_some(hacc), None);
            }
            em.f.instructions().local_get(hline);
            if line_droppable {
                em.rc_inc_top();
                em.witness_walk_share(None, line);
            }
            em.f.instructions().local_get(hcl).i32_load(slot_memarg(0));
            em.f.instructions().call_indirect(0, ti);
            let wc = em.witness_fallible_carrier(record);
            let mut i = em.f.instructions();
            i.local_set(hres);
            i.local_get(hres).i32_load(slot_memarg(almide_layout::SUM_TAG)).i32_eqz();
            i.if_(BlockType::Empty);
            if let Some(dec) = acc_dec {
                i.local_get(hacc).call(dec);
            }
            i.local_get(hres);
            let _ = i;
            em.load_ty_slot(acc_ty, almide_layout::SUM_FIELD);
            let mut i = em.f.instructions();
            i.local_set(hacc);
            i.local_get(hres).call(F_DEC_FLAT);
            i.else_();
            i.local_get(hres).local_set(hr);
            i.end();
            i.end();
            em.witness_fallible_close(wc, true, heap_acc.then_some(hacc));
            Ok(())
        })?;
        self.release_i32();
        self.release_i32();
        self.fs_frames_release_raw(hraw, herr);
        self.fs_stream_land_err(hmsg, hr, true);
        self.witness_fallible_result(heap_acc, hacc, acc_ty);
        let hs = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(herr).if_(BlockType::Result(ValType::I32));
            if let Some(dec) = acc_dec {
                i.local_get(hacc).call(dec);
            }
            i.local_get(herr);
            i.else_();
            i.local_get(hr).if_(BlockType::Result(ValType::I32));
            if let Some(dec) = acc_dec {
                i.local_get(hacc).call(dec);
            }
            i.local_get(hr);
            i.else_();
            i.i32_const(16)
                .call(F_ALLOC)
                .local_tee(hs)
                .i32_const(0)
                .i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hs).local_get(hacc);
        }
        self.store_ty_slot(acc_ty, almide_layout::SUM_FIELD);
        self.f.instructions().local_get(hs).end().end();
        // hs, hr, hmsg, herr, hlen, hraw are i32 holds; hacc is typed; hcl is i32.
        for _ in 0..6 {
            self.release_i32();
        }
        self.release_for(acc_ty);
        self.release_i32();
        Ok(SliceTy::Result(self.types.intern(acc_ty), self.types.intern(STR)))
    }
}
