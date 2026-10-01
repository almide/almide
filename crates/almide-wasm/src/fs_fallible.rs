//! `fs.__fallible_for_each_line` (#1806), split from fs_meta.rs for the
//! file budget: the walk twin of the fallible fold.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use super::body_propagates;
use crate::*;

impl Emitter<'_> {
    /// `fs.__fallible_for_each_line(p, cb)`: the callback yields
    /// Result[Unit, String] per line; the first err is the whole result and
    /// later lines never see the callback. A canonical body inlines; a
    /// compound body (its own `!`s) is called as the closure it is (#1806).
    pub(crate) fn lower_fs_fallible_for_each(&mut self, p: &IrExpr, cb: &IrExpr) -> Result<SliceTy, EmitError> {
        let compound = body_propagates(cb);
        let (params, body, hcl, ti) = if compound {
            let got = self.lower_arg(cb, None, ArgMode::Borrow)?;
            let SliceTy::Fn(sig) = got else {
                return unsup(&format!("fs-fallible-each-callee-{got:?}"));
            };
            let def = self.types.fn_sig_def(sig);
            if def.params.len() != 1 {
                return unsup("fs-fallible-each-arity");
            }
            let mut ps: Vec<ValType> = vec![ValType::I32];
            ps.extend(def.params.iter().map(|t| t.val_type()));
            let ti = self.work.itype(ps, def.ret.map(SliceTy::val_type));
            let hcl = self.hold_i32()?;
            self.f.instructions().local_set(hcl);
            (Vec::new(), None, Some(hcl), Some(ti))
        } else {
            let (params, body) = self.hof_lambda(cb, 1)?;
            (params, Some(body), None, None)
        };
        self.fs_call_1(p, 12)?; // OP_READ_LINES
        let (hraw, hlen, herr) = self.fs_frames_or_err()?;
        let hr = self.hold_i32()?;
        self.f.instructions().i32_const(0).local_set(hr);
        // #3137: a called closure and an owned inline body hand the walk the
        // Result carrier's one credit — an ok is released, an err IS the
        // result; a borrowed inline body's err takes its own share.
        let owned = body.is_none_or(|b| self.rc_owned_result(b));
        let hline = self.hold_i32()?;
        let hres = self.hold_i32()?;
        self.fs_frames_foreach_borrowed(hraw, hlen, |em| {
            em.f.instructions().local_set(hline);
            em.f.instructions().local_get(hr).i32_eqz().if_(BlockType::Empty);
            match (hcl, ti, body) {
                (Some(hcl), Some(ti), _) => {
                    em.f.instructions().local_get(hcl);
                    em.f.instructions().local_get(hline);
                    em.rc_inc_top();
                    em.f.instructions().local_get(hcl).i32_load(slot_memarg(0));
                    em.f.instructions().call_indirect(0, ti);
                }
                (_, _, Some(body)) => {
                    em.f.instructions().local_get(hline).local_set(params[0]);
                    em.lower(body, None)?;
                }
                _ => return unsup("fs-fallible-each-shape"),
            }
            let mut i = em.f.instructions();
            i.local_set(hres);
            i.local_get(hres).i32_load(slot_memarg(almide_layout::SUM_TAG)).if_(BlockType::Empty);
            i.local_get(hres);
            let _ = i;
            if !owned {
                em.rc_inc_top();
            }
            let mut i = em.f.instructions();
            i.local_set(hr);
            if owned {
                i.else_();
                i.local_get(hres).call(F_DEC_FLAT);
            }
            i.end();
            i.end();
            Ok(())
        })?;
        self.release_i32();
        self.release_i32();
        self.fs_frames_release_raw(hraw, herr);
        let hs = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(herr).if_(BlockType::Result(ValType::I32));
            i.local_get(herr);
            i.else_();
            i.local_get(hr).if_(BlockType::Result(ValType::I32));
            i.local_get(hr);
            i.else_();
            i.i32_const(16)
                .call(F_ALLOC)
                .local_tee(hs)
                .i32_const(0)
                .i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hs).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_FIELD));
            i.local_get(hs);
            i.end().end();
        }
        for _ in 0..5 {
            self.release_i32();
        }
        if compound {
            self.release_i32();
        }
        Ok(SliceTy::Result(self.types.intern(SliceTy::Unit), self.types.intern(STR)))
    }
}
