//! The option/result combinator family — the INTRINSIC cells (`= _` in
//! stdlib/option.almd / stdlib/result.almd), extended BY MATRIX in one
//! landing per the API-family doctrine. Source-level cells (flatten,
//! to_list, zip, or_else on the result side, …) already compile from
//! their stdlib match bodies through the linked-module path and are NOT
//! duplicated here.
//!
//! Callbacks are the literal-lambda idiom via `hof_lambda` (Fn-typed
//! values refuse honestly). Pass-through sides REUSE the subject block —
//! sums are never mutated in place, so sharing is unobservable.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// `option.*` / `result.*` intrinsic combinators. Ok(None) = not a
    /// cell of this matrix — the caller falls through to the linked path.
    pub(crate) fn lower_sum_combinator(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let out = match (module, func, args) {
            ("result", "is_ok" | "is_err", [r]) => {
                let SliceTy::Result(..) = self.lower(r, None)? else {
                    return unsup(&format!("result-{func}-of-nonresult"));
                };
                let mut i = self.f.instructions();
                i.i32_load(slot_memarg(almide_layout::SUM_TAG));
                if func == "is_ok" {
                    i.i32_eqz();
                } else {
                    i.i32_const(0).i32_ne();
                }
                Some(BOOL)
            }
            ("option", "is_some" | "is_none", [o]) => {
                let SliceTy::Option(_) = self.lower(o, None)? else {
                    return unsup(&format!("option-{func}-of-nonoption"));
                };
                let mut i = self.f.instructions();
                i.i32_eqz();
                if func == "is_some" {
                    i.i32_eqz();
                }
                Some(BOOL)
            }
            ("result", "map" | "map_err", [r, f]) => self.lower_result_map(func, r, f)?,
            // partition: one pass, oks/errs each an upper-bound alloc with
            // a final len patch (the filter doctrine).
            ("result", "partition", [xs]) => {
                let el = match self.lower(xs, None)? {
                    SliceTy::List(h) => self.types.el(h),
                    other => return unsup(&format!("result-partition-of:{other:?}")),
                };
                let SliceTy::Result(o, er) = el else {
                    return unsup(&format!("result-partition-el:{el:?}"));
                };
                let (t, e) = (self.types.el(o), self.types.el(er));
                let (ts, es) = (t.slot_size() as i32, e.slot_size() as i32);
                let hb = self.hold_i32()?;
                let hok = self.hold_i32()?;
                let herr = self.hold_i32()?;
                let hwo = self.hold_i32()?;
                let hwe = self.hold_i32()?;
                let hi = self.hold_i32()?;
                let hr = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hb);
                    i.local_get(hb)
                        .i32_load(len_memarg())
                        .i32_const(ts)
                        .i32_mul()
                        .i32_const(4)
                        .i32_div_u()
                        .call(F_ALLOC)
                        .local_set(hok);
                    i.local_get(hb)
                        .i32_load(len_memarg())
                        .i32_const(es)
                        .i32_mul()
                        .i32_const(4)
                        .i32_div_u()
                        .call(F_ALLOC)
                        .local_set(herr);
                    i.i32_const(0).local_set(hwo);
                    i.i32_const(0).local_set(hwe);
                    i.i32_const(0).local_set(hi);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(hi).local_get(hb).i32_load(len_memarg()).i32_ge_u().br_if(1);
                    i.local_get(hb).local_get(hi).i32_add().i32_load(slot_memarg(0)).local_set(hr);
                    i.local_get(hr)
                        .i32_load(slot_memarg(almide_layout::SUM_TAG))
                        .i32_eqz()
                        .if_(BlockType::Empty);
                    i.local_get(hok).local_get(hwo).i32_add();
                    i.local_get(hr);
                }
                self.load_ty_slot(t, almide_layout::SUM_FIELD);
                self.store_ty_slot(t, 0);
                {
                    let mut i = self.f.instructions();
                    i.local_get(hwo).i32_const(ts).i32_add().local_set(hwo);
                    i.else_();
                    i.local_get(herr).local_get(hwe).i32_add();
                    i.local_get(hr);
                }
                self.load_ty_slot(e, almide_layout::SUM_FIELD);
                self.store_ty_slot(e, 0);
                {
                    let mut i = self.f.instructions();
                    i.local_get(hwe).i32_const(es).i32_add().local_set(hwe);
                    i.end();
                    i.local_get(hi).i32_const(4).i32_add().local_set(hi);
                    i.br(0).end().end();
                    i.local_get(hok).local_get(hwo).i32_store(len_memarg());
                    i.local_get(herr).local_get(hwe).i32_store(len_memarg());
                }
                let ti = self.types.tuple(vec![SliceTy::List(o), SliceTy::List(er)]);
                let def = self.types.tuple_def(ti);
                let (off_ok, off_err) = (def.fields[0].1, def.fields[1].1);
                let size = def.size;
                {
                    let mut i = self.f.instructions();
                    i.i32_const(size as i32).call(F_ALLOC).local_set(hr);
                    i.local_get(hr).local_get(hok).i32_store(slot_memarg(off_ok));
                    i.local_get(hr).local_get(herr).i32_store(slot_memarg(off_err));
                    i.local_get(hr);
                }
                for _ in 0..7 {
                    self.release_i32();
                }
                Some(SliceTy::Tuple(ti))
            }
            ("result", "flat_map", [r, f]) => {
                let SliceTy::Result(o, _) = self.lower(r, None)? else {
                    return unsup("result-flat_map-of-nonresult");
                };
                let (params, body) = self.hof_lambda(f, 1)?;
                let rb = match slice_ty_of(&body.ty, self.types) {
                    Some(t @ SliceTy::Result(..)) => t,
                    _ => return unsup(&format!("result-flat_map-ret:{}", ty_name(&body.ty))),
                };
                let a = self.types.el(o);
                let hs = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hs);
                    i.local_get(hs)
                        .i32_load(slot_memarg(almide_layout::SUM_TAG))
                        .i32_const(0)
                        .i32_ne();
                    i.if_(BlockType::Result(ValType::I32));
                    i.local_get(hs);
                    i.else_();
                }
                self.f.instructions().local_get(hs);
                self.load_ty_slot(a, almide_layout::SUM_FIELD);
                self.f.instructions().local_set(params[0]);
                self.lower(body, Some(rb))?;
                self.f.instructions().end();
                self.release_i32();
                Some(rb)
            }
            ("result", "unwrap_or_else", [r, f]) => {
                let SliceTy::Result(o, er) = self.lower(r, None)? else {
                    return unsup("result-uoe-of-nonresult");
                };
                let (params, body) = self.hof_lambda(f, 1)?;
                let (a, e) = (self.types.el(o), self.types.el(er));
                let hs = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hs);
                    i.local_get(hs)
                        .i32_load(slot_memarg(almide_layout::SUM_TAG))
                        .i32_const(0)
                        .i32_ne();
                    i.if_(BlockType::Result(a.val_type()));
                }
                self.f.instructions().local_get(hs);
                self.load_ty_slot(e, almide_layout::SUM_FIELD);
                self.f.instructions().local_set(params[0]);
                self.lower(body, Some(a))?;
                self.f.instructions().else_().local_get(hs);
                self.load_ty_slot(a, almide_layout::SUM_FIELD);
                self.f.instructions().end();
                self.release_i32();
                Some(a)
            }
            ("result", "to_option" | "to_err_option", [r]) => {
                let want_ok = func == "to_option";
                let SliceTy::Result(o, er) = self.lower(r, None)? else {
                    return unsup(&format!("result-{func}-of-nonresult"));
                };
                let side_h = if want_ok { o } else { er };
                let side = self.types.el(side_h);
                let hs = self.hold_i32()?;
                let hb = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hs);
                    i.local_get(hs).i32_load(slot_memarg(almide_layout::SUM_TAG));
                    if want_ok {
                        i.i32_const(0).i32_ne();
                    } else {
                        i.i32_eqz();
                    }
                    i.if_(BlockType::Result(ValType::I32));
                    i.i32_const(0);
                    i.else_();
                    i.i32_const(side.slot_size() as i32).call(F_ALLOC).local_tee(hb);
                    let _ = i;
                }
                self.f.instructions().local_get(hs);
                self.load_ty_slot(side, almide_layout::SUM_FIELD);
                self.store_ty_slot(side, almide_layout::OPTION_FIELD);
                self.f.instructions().local_get(hb).end();
                self.release_i32();
                self.release_i32();
                Some(SliceTy::Option(side_h))
            }
            ("option", "map" | "flat_map", [o_arg, f]) => {
                let flat = func == "flat_map";
                let SliceTy::Option(h) = self.lower(o_arg, None)? else {
                    return unsup(&format!("option-{func}-of-nonoption"));
                };
                let a = self.types.el(h);
                let (params, body) = self.hof_lambda(f, 1)?;
                let Some(b) = slice_ty_of(&body.ty, self.types) else {
                    return unsup(&format!("option-{func}-ret:{}", ty_name(&body.ty)));
                };
                if flat && !matches!(b, SliceTy::Option(_)) {
                    return unsup("option-flat_map-ret-nonoption");
                }
                let hs = self.hold_i32()?;
                let hb = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hs);
                    i.local_get(hs).i32_eqz();
                    i.if_(BlockType::Result(ValType::I32));
                    i.i32_const(0);
                    i.else_();
                }
                self.f.instructions().local_get(hs);
                self.load_ty_slot(a, almide_layout::OPTION_FIELD);
                self.f.instructions().local_set(params[0]);
                let out_ty = if flat {
                    self.lower(body, Some(b))?;
                    b
                } else {
                    self.f
                        .instructions()
                        .i32_const(b.slot_size() as i32)
                        .call(F_ALLOC)
                        .local_tee(hb);
                    self.lower(body, Some(b))?;
                    self.store_ty_slot(b, almide_layout::OPTION_FIELD);
                    self.f.instructions().local_get(hb);
                    SliceTy::Option(self.types.intern(b))
                };
                self.f.instructions().end();
                self.release_i32();
                self.release_i32();
                Some(out_ty)
            }
            ("option", "flatten", [o_arg]) => {
                let SliceTy::Option(h) = self.lower(o_arg, None)? else {
                    return unsup("option-flatten-of-nonoption");
                };
                let inner = self.types.el(h);
                let SliceTy::Option(_) = inner else {
                    return unsup("option-flatten-elem-nonoption");
                };
                let hs = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hs);
                    i.local_get(hs).i32_eqz();
                    i.if_(BlockType::Result(ValType::I32));
                    i.i32_const(0);
                    i.else_();
                }
                self.f.instructions().local_get(hs);
                self.load_ty_slot(inner, almide_layout::OPTION_FIELD);
                self.f.instructions().end();
                self.release_i32();
                Some(inner)
            }
            ("option", "unwrap_or_else", [o_arg, f]) => {
                let SliceTy::Option(h) = self.lower(o_arg, None)? else {
                    return unsup("option-uoe-of-nonoption");
                };
                let a = self.types.el(h);
                let (_params, body) = self.hof_lambda(f, 0)?;
                let hs = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hs);
                    i.local_get(hs).i32_eqz();
                    i.if_(BlockType::Result(a.val_type()));
                }
                self.lower(body, Some(a))?;
                self.f.instructions().else_().local_get(hs);
                self.load_ty_slot(a, almide_layout::OPTION_FIELD);
                self.f.instructions().end();
                self.release_i32();
                Some(a)
            }
            ("option", "or_else", [o_arg, f]) => {
                let got @ SliceTy::Option(_) = self.lower(o_arg, None)? else {
                    return unsup("option-or_else-of-nonoption");
                };
                let (_params, body) = self.hof_lambda(f, 0)?;
                let hs = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hs);
                    i.local_get(hs).i32_eqz();
                    i.if_(BlockType::Result(ValType::I32));
                }
                self.lower(body, Some(got))?;
                self.f.instructions().else_().local_get(hs).end();
                self.release_i32();
                Some(got)
            }
            ("option", "filter", [o_arg, f]) => {
                let got @ SliceTy::Option(h) = self.lower(o_arg, None)? else {
                    return unsup("option-filter-of-nonoption");
                };
                let a = self.types.el(h);
                let (params, body) = self.hof_lambda(f, 1)?;
                let hs = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hs);
                    i.local_get(hs).i32_eqz();
                    i.if_(BlockType::Result(ValType::I32));
                    i.i32_const(0);
                    i.else_();
                }
                self.f.instructions().local_get(hs);
                self.load_ty_slot(a, almide_layout::OPTION_FIELD);
                self.f.instructions().local_set(params[0]);
                self.lower(body, Some(BOOL))?;
                {
                    let mut i = self.f.instructions();
                    i.if_(BlockType::Result(ValType::I32));
                    i.local_get(hs);
                    i.else_();
                    i.i32_const(0);
                    i.end();
                    i.end();
                }
                self.release_i32();
                Some(got)
            }
            ("option", "zip", [a_arg, b_arg]) => {
                let SliceTy::Option(ha) = self.lower(a_arg, None)? else {
                    return unsup("option-zip-of-nonoption");
                };
                let hla = self.hold_i32()?;
                self.f.instructions().local_set(hla);
                let SliceTy::Option(hb) = self.lower(b_arg, None)? else {
                    return unsup("option-zip-of-nonoption");
                };
                let (a, b) = (self.types.el(ha), self.types.el(hb));
                let ti = self.types.tuple(vec![a, b]);
                let def = self.types.tuple_def(ti);
                let (aoff, boff, size) = (def.fields[0].1, def.fields[1].1, def.size);
                let hlb = self.hold_i32()?;
                let hp = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hlb);
                    i.local_get(hla)
                        .i32_eqz()
                        .local_get(hlb)
                        .i32_eqz()
                        .i32_or();
                    i.if_(BlockType::Result(ValType::I32));
                    i.i32_const(0);
                    i.else_();
                    i.i32_const(size as i32).call(F_ALLOC).local_tee(hp);
                    let _ = i;
                }
                self.f.instructions().local_get(hla);
                self.load_ty_slot(a, almide_layout::OPTION_FIELD);
                self.store_ty_slot(a, aoff);
                self.f.instructions().local_get(hp).local_get(hlb);
                self.load_ty_slot(b, almide_layout::OPTION_FIELD);
                self.store_ty_slot(b, boff);
                // the tuple base doubles as the some-payload: a tuple-typed
                // some stores the BLOCK ADDRESS at OPTION_FIELD
                let hs = self.hold_i32()?;
                self.f
                    .instructions()
                    .i32_const(4)
                    .call(F_ALLOC)
                    .local_tee(hs)
                    .local_get(hp)
                    .i32_store(slot_memarg(almide_layout::OPTION_FIELD));
                self.f.instructions().local_get(hs).end();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Some(SliceTy::Option(self.types.intern(SliceTy::Tuple(ti))))
            }
            ("option", "to_list", [o_arg]) => {
                let SliceTy::Option(h) = self.lower(o_arg, None)? else {
                    return unsup("option-to_list-of-nonoption");
                };
                let a = self.types.el(h);
                let hs = self.hold_i32()?;
                let hb = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hs);
                    i.local_get(hs).i32_eqz();
                    i.if_(BlockType::Result(ValType::I32));
                    i.i32_const(0).call(F_ALLOC);
                    i.else_();
                    i.i32_const(a.slot_size() as i32).call(F_ALLOC).local_tee(hb);
                    let _ = i;
                }
                self.f.instructions().local_get(hs);
                self.load_ty_slot(a, almide_layout::OPTION_FIELD);
                self.store_ty_slot(a, 0);
                self.f.instructions().local_get(hb).end();
                self.release_i32();
                self.release_i32();
                Some(SliceTy::List(h))
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }

    fn lower_result_map(
        &mut self,
        func: &str,
        r: &IrExpr,
        f: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        Ok({

                let on_ok = func == "map";
                let SliceTy::Result(o, er) = self.lower(r, None)? else {
                    return unsup(&format!("result-{func}-of-nonresult"));
                };
                let (params, body) = self.hof_lambda(f, 1)?;
                let Some(b) = slice_ty_of(&body.ty, self.types) else {
                    return unsup(&format!("result-{func}-ret:{}", ty_name(&body.ty)));
                };
                let side = self.types.el(if on_ok { o } else { er });
                let hs = self.hold_i32()?;
                let hb = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hs);
                    i.local_get(hs).i32_load(slot_memarg(almide_layout::SUM_TAG));
                    if on_ok {
                        i.i32_const(0).i32_ne();
                    } else {
                        i.i32_eqz();
                    }
                    i.if_(BlockType::Result(ValType::I32));
                    // pass-through side: the block is reused as-is
                    i.local_get(hs);
                    i.else_();
                }
                self.f.instructions().local_get(hs);
                self.load_ty_slot(side, almide_layout::SUM_FIELD);
                self.f.instructions().local_set(params[0]);
                self.f
                    .instructions()
                    .i32_const(16)
                    .call(F_ALLOC)
                    .local_tee(hb)
                    .i32_const(if on_ok { 0 } else { 1 })
                    .i32_store(slot_memarg(almide_layout::SUM_TAG));
                self.f.instructions().local_get(hb);
                self.lower(body, Some(b))?;
                self.store_ty_slot(b, almide_layout::SUM_FIELD);
                self.f.instructions().local_get(hb).end();
                self.release_i32();
                self.release_i32();
                let bi = self.types.intern(b);
                Some(if on_ok { SliceTy::Result(bi, er) } else { SliceTy::Result(o, bi) })
        })
    }
}
