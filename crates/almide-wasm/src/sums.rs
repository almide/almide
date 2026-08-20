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
