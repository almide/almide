//! Ordering and slicing list surfaces (sort/sort_by/min/max/take/
//! chunk/windows) — split from list.rs for the complexity budget; the
//! dispatcher falls through here first (Ok(None) = not this family).

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    pub(crate) fn lower_list_order_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        match (func, args) {
            // First n elements (native `take(n as usize)`): a NEGATIVE
            // n reinterprets huge and takes the WHOLE list.
            ("take", [xs, n]) => {
                let h = match self.lower(xs, None)? {
                    SliceTy::List(h) => h,
                    other => return unsup(&format!("list-take-of:{other:?}")),
                };
                let stride = self.types.el(h).slot_size() as i32;
                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                self.lower(n, Some(INT))?;
                let hn = self.hold_i64()?;
                let hc = self.hold_i32()?;
                let ho = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hn);
                // bytes = n < 0 ? len : min(n*stride, len)
                i.local_get(hb).i32_load(len_memarg());
                i.local_get(hn).i64_const(stride as i64).i64_mul();
                i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u();
                i.local_get(hn).i64_const(stride as i64).i64_mul();
                i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u();
                i.i64_gt_s().select().i32_wrap_i64();
                i.local_get(hn).i64_const(0).i64_lt_s();
                i.select().local_set(hc);
                i.local_get(hc).call(F_ALLOC).local_set(ho);
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hc);
                i.memory_copy(0, 0);
                i.local_get(ho);
                let _ = i;
                self.release_i32();
                self.release_i32();
                self.release_i64();
                self.release_i32();
                Ok(Some(Some(SliceTy::List(h))))
            }
            ("min" | "max", [xs]) => self.lower_list_min_max(func, xs).map(Some),
            // Insertion sort over a FRESH copy — stable, and for scalar
            // elements any correct sort is value-identical to native's
            // `v.sort()`. Float order is IEEE totalOrder (f64::total_cmp):
            // key = bits ^ ((bits >>s 63) >>u 1), compared signed.
            ("sort", [xs]) => {
                let h = match self.lower(xs, None)? {
                    SliceTy::List(h) => h,
                    other => return unsup(&format!("list-sort-of:{other:?}")),
                };
                let elem = self.types.el(h);
                if !matches!(elem, INT | FLOAT | STR) {
                    return unsup(&format!("list-sort-elem:{elem:?}"));
                }
                let stride = elem.slot_size() as i32;
                self.f.instructions().call(F_BLOCK_COPY);
                let hb = self.hold_i32()?;
                let hn = self.hold_i32()?;
                let hi = self.hold_i32()?;
                let hj = self.hold_i32()?;
                let ht = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.local_set(hb);
                i.local_get(hb).i32_load(len_memarg()).i32_const(stride).i32_div_u().local_set(hn);
                i.i32_const(1).local_set(hi);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(hi).local_get(hn).i32_ge_u().br_if(1);
                i.local_get(hi).local_set(hj);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(hj).i32_eqz().br_if(1);
                // a = elem[j-1], b = elem[j]; break unless a > b
                let addr = |i: &mut wasm_encoder::InstructionSink<'_>, idx: u32, back: i32| {
                    i.local_get(hb)
                        .local_get(idx)
                        .i32_const(back)
                        .i32_sub()
                        .i32_const(stride)
                        .i32_mul()
                        .i32_add();
                };
                match elem {
                    INT | FLOAT => {
                        let key = |i: &mut wasm_encoder::InstructionSink<'_>| {
                            if elem == FLOAT {
                                // totalOrder key transform on the raw bits
                                let t = ht;
                                i.local_set(t);
                                i.local_get(t);
                                i.local_get(t).i64_const(63).i64_shr_s().i64_const(1).i64_shr_u();
                                i.i64_xor();
                            }
                        };
                        addr(&mut i, hj, 1);
                        i.i64_load(slot_memarg(0));
                        key(&mut i);
                        let hk = ht; // ht reused as raw scratch inside key only
                        let _ = hk;
                        addr(&mut i, hj, 0);
                        i.i64_load(slot_memarg(0));
                        key(&mut i);
                        i.i64_le_s().br_if(1);
                    }
                    _ => {
                        addr(&mut i, hj, 1);
                        i.i32_load(slot_memarg(0));
                        addr(&mut i, hj, 0);
                        i.i32_load(slot_memarg(0));
                        i.call(F_STR_CMP).i32_const(0).i32_le_s().br_if(1);
                    }
                }
                // swap elem[j-1] <-> elem[j]
                if stride == 8 {
                    addr(&mut i, hj, 0);
                    i.i64_load(slot_memarg(0)).local_set(ht);
                    addr(&mut i, hj, 0);
                    addr(&mut i, hj, 1);
                    i.i64_load(slot_memarg(0)).i64_store(slot_memarg(0));
                    addr(&mut i, hj, 1);
                    i.local_get(ht).i64_store(slot_memarg(0));
                } else {
                    addr(&mut i, hj, 0);
                    i.i32_load(slot_memarg(0)).i64_extend_i32_u().local_set(ht);
                    addr(&mut i, hj, 0);
                    addr(&mut i, hj, 1);
                    i.i32_load(slot_memarg(0)).i32_store(slot_memarg(0));
                    addr(&mut i, hj, 1);
                    i.local_get(ht).i32_wrap_i64().i32_store(slot_memarg(0));
                }
                i.local_get(hj).i32_const(1).i32_sub().local_set(hj);
                i.br(0).end().end();
                i.local_get(hi).i32_const(1).i32_add().local_set(hi);
                i.br(0).end().end();
                i.local_get(hb);
                let _ = i;
                self.release_i64();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(Some(Some(SliceTy::List(h))))
            }
            ("chunk" | "windows", [xs, n_arg]) => {
                self.lower_list_chunk_windows(func, xs, n_arg).map(Some)
            }
            ("sort_by", [xs, cb]) => self.lower_list_sort_by(xs, cb).map(Some),
            _ => Ok(None),
        }
    }

    /// Keys precomputed ONCE per element into a parallel array (#560 —
    /// per-comparison evaluation was an observable divergence for
    /// side-effectful keys), then the list.sort insertion sort moves
    /// keys and values in lockstep. Stable; key orders are the scalar
    /// three (Int/Str Ord, Float totalOrder).
    fn lower_list_min_max(
        &mut self,
        func: &str,
        xs: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {

                let is_min = func == "min";
                let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
                if !matches!(elem, INT | FLOAT | STR | BOOL) {
                    return unsup(&format!("list-{func}-elem:{elem:?}"));
                }
                let wide = elem.slot_size() == 8;
                let hbest = self.hold_i64()?;
                let hcur = self.hold_i64()?;
                let hfound = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.i32_const(0).local_set(hfound);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(ih).local_get(ch).i32_ge_u().br_if(1);
                i.local_get(bh)
                    .local_get(ih)
                    .i32_const(elem.slot_size() as i32)
                    .i32_mul()
                    .i32_add();
                if wide {
                    i.i64_load(slot_memarg(0));
                } else {
                    i.i32_load(slot_memarg(0)).i64_extend_i32_u();
                }
                i.local_set(hcur);
                i.local_get(hfound).i32_eqz().if_(BlockType::Empty);
                i.local_get(hcur).local_set(hbest);
                i.i32_const(1).local_set(hfound);
                i.else_();
                // beats = cur < best (min) / cur > best (max)
                match elem {
                    FLOAT => {
                        for l in [hcur, hbest] {
                            i.local_get(l);
                            i.local_get(l).i64_const(63).i64_shr_s().i64_const(1).i64_shr_u();
                            i.i64_xor();
                        }
                        if is_min {
                            i.i64_lt_s();
                        } else {
                            i.i64_gt_s();
                        }
                    }
                    INT => {
                        i.local_get(hcur).local_get(hbest);
                        if is_min {
                            i.i64_lt_s();
                        } else {
                            i.i64_gt_s();
                        }
                    }
                    BOOL => {
                        i.local_get(hcur).local_get(hbest);
                        if is_min {
                            i.i64_lt_u();
                        } else {
                            i.i64_gt_u();
                        }
                    }
                    _ => {
                        i.local_get(hcur).i32_wrap_i64();
                        i.local_get(hbest).i32_wrap_i64();
                        i.call(F_STR_CMP).i32_const(0);
                        if is_min {
                            i.i32_lt_s();
                        } else {
                            i.i32_gt_s();
                        }
                    }
                }
                i.if_(BlockType::Empty);
                i.local_get(hcur).local_set(hbest);
                i.end();
                i.end();
                i.local_get(ih).i32_const(1).i32_add().local_set(ih);
                i.br(0).end().end();
                let _ = i;
                // found ? some(best) : none — best is raw bits in an i64
                // local; stores are bitwise, so f64 needs no reinterpret.
                let hres = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_get(hfound).if_(BlockType::Result(ValType::I32));
                i.i32_const(elem.slot_size() as i32).call(F_ALLOC).local_tee(hres);
                i.local_get(hbest);
                let m = slot_memarg(almide_layout::OPTION_FIELD);
                if wide {
                    i.i64_store(m);
                } else {
                    i.i32_wrap_i64().i32_store(m);
                }
                i.local_get(hres);
                i.else_();
                i.i32_const(0);
                i.end();
                let _ = i;
                self.release_i32();
                self.release_i32();
                self.release_i64();
                self.release_i64();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(Some(SliceTy::Option(self.types.intern(elem))))
    }

    fn lower_list_chunk_windows(
        &mut self,
        func: &str,
        xs: &IrExpr,
        n_arg: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {

                let windows = func == "windows";
                let h = match self.lower(xs, None)? {
                    SliceTy::List(h) => h,
                    other => return unsup(&format!("list-{func}-of:{other:?}")),
                };
                let elem = self.types.el(h);
                let stride = elem.slot_size() as i32;
                let hxs = self.hold_i32()?;
                self.f.instructions().local_set(hxs);
                self.lower(n_arg, Some(INT))?;
                let hn = self.hold_i64()?;
                let msg = self.pool.intern(if windows {
                    "window size must be positive"
                } else {
                    "chunk size must be positive"
                });
                {
                    let mut i = self.f.instructions();
                    i.local_set(hn);
                    i.local_get(hn).i64_eqz();
                    i.if_(BlockType::Empty);
                    i.i32_const(msg as i32);
                }
                self.emit_error_frame_abort();
                let htot = self.hold_i64()?;
                let hnum = self.hold_i32()?;
                let ho = self.hold_i32()?;
                let hk = self.hold_i32()?;
                let hrow = self.hold_i32()?;
                let hcs = self.hold_i64()?;
                {
                    let mut i = self.f.instructions();
                    i.end();
                    i.local_get(hxs)
                        .i32_load(len_memarg())
                        .i32_const(stride)
                        .i32_div_u()
                        .i64_extend_i32_u()
                        .local_set(htot);
                    if windows {
                        // n < 0 or n > total → zero windows
                        i.local_get(hn).i64_const(0).i64_lt_s();
                        i.local_get(hn).local_get(htot).i64_gt_s();
                        i.i32_or().if_(BlockType::Result(ValType::I32));
                        i.i32_const(0);
                        i.else_();
                        i.local_get(htot).local_get(hn).i64_sub().i64_const(1).i64_add().i32_wrap_i64();
                        i.end();
                        i.local_set(hnum);
                    } else {
                        // negative n → everything in one chunk
                        i.local_get(hn).i64_const(0).i64_lt_s().if_(BlockType::Empty);
                        // select(v1, v2, cond) = cond ? v1 : v2
                        i.local_get(htot)
                            .i64_const(1)
                            .local_get(htot)
                            .i64_const(0)
                            .i64_gt_s()
                            .select()
                            .local_set(hn);
                        i.end();
                        i.local_get(htot).local_get(hn).i64_div_s();
                        i.local_get(htot).local_get(hn).i64_rem_s().i64_const(0).i64_ne().i64_extend_i32_u();
                        i.i64_add().i32_wrap_i64().local_set(hnum);
                    }
                    i.local_get(hnum).i32_const(2).i32_shl().call(F_ALLOC).local_set(ho);
                    i.i32_const(0).local_set(hk);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(hk).local_get(hnum).i32_ge_u().br_if(1);
                    // row size: windows → n; chunk → min(n, total - k*n)
                    if windows {
                        i.local_get(hn).local_set(hcs);
                    } else {
                        // cs = n > remaining ? remaining : n
                        i.local_get(htot).local_get(hk).i64_extend_i32_u().local_get(hn).i64_mul().i64_sub();
                        i.local_get(hn);
                        i.local_get(hn);
                        i.local_get(htot).local_get(hk).i64_extend_i32_u().local_get(hn).i64_mul().i64_sub();
                        i.i64_gt_s().select().local_set(hcs);
                    }
                    i.local_get(hcs).i32_wrap_i64().i32_const(stride).i32_mul().call(F_ALLOC).local_set(hrow);
                    // copy from source: start = windows ? k : k*n (elements)
                    i.local_get(hrow).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                    i.local_get(hxs).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                    if windows {
                        i.local_get(hk).i32_const(stride).i32_mul().i32_add();
                    } else {
                        i.local_get(hk)
                            .i64_extend_i32_u()
                            .local_get(hn)
                            .i64_mul()
                            .i32_wrap_i64()
                            .i32_const(stride)
                            .i32_mul()
                            .i32_add();
                    }
                    i.local_get(hcs).i32_wrap_i64().i32_const(stride).i32_mul();
                    i.memory_copy(0, 0);
                    i.local_get(ho).local_get(hk).i32_const(2).i32_shl().i32_add();
                    i.local_get(hrow).i32_store(slot_memarg(0));
                    i.local_get(hk).i32_const(1).i32_add().local_set(hk);
                    i.br(0).end().end();
                    i.local_get(ho);
                }
                self.release_i64();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i64();
                self.release_i64();
                self.release_i32();
                Ok(Some(SliceTy::List(self.types.intern(SliceTy::List(h)))))
    }

    fn lower_list_sort_by(&mut self, xs: &IrExpr, cb: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let Some(k) = slice_ty_of(&body.ty, self.types) else {
            return unsup(&format!("list-sort-by-key:{}", ty_name(&body.ty)));
        };
        if !matches!(k, INT | FLOAT | STR) {
            return unsup(&format!("list-sort-by-key:{k:?}"));
        }
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-sort-by-of:{other:?}")),
        };
        let elem = self.types.el(h);
        let (vstride, kstride) = (elem.slot_size() as i32, k.slot_size() as i32);
        self.f.instructions().call(F_BLOCK_COPY);
        let hb = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hkeys = self.hold_i32()?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let ht = self.hold_i64()?;
        {
            let mut i = self.f.instructions();
            i.local_tee(hb);
            i.i32_load(len_memarg()).i32_const(vstride).i32_div_u().local_set(hn);
            i.local_get(hn).i32_const(kstride).i32_mul().call(F_ALLOC).local_set(hkeys);
            i.i32_const(0).local_set(hi);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hi).local_get(hn).i32_ge_u().br_if(1);
            i.local_get(hb).local_get(hi).i32_const(vstride).i32_mul().i32_add();
        }
        self.load_ty_slot(elem, 0);
        self.f.instructions().local_set(params[0]);
        self.f
            .instructions()
            .local_get(hkeys)
            .local_get(hi)
            .i32_const(kstride)
            .i32_mul()
            .i32_add();
        self.lower(body, Some(k))?;
        self.store_ty_slot(k, 0);
        {
            let mut i = self.f.instructions();
            i.local_get(hi).i32_const(1).i32_add().local_set(hi);
            i.br(0).end().end();
            // lockstep insertion sort on (keys, vals)
            let kaddr = |i: &mut wasm_encoder::InstructionSink<'_>, back: i32| {
                i.local_get(hkeys)
                    .local_get(hj)
                    .i32_const(back)
                    .i32_sub()
                    .i32_const(kstride)
                    .i32_mul()
                    .i32_add();
            };
            let vaddr = |i: &mut wasm_encoder::InstructionSink<'_>, back: i32| {
                i.local_get(hb)
                    .local_get(hj)
                    .i32_const(back)
                    .i32_sub()
                    .i32_const(vstride)
                    .i32_mul()
                    .i32_add();
            };
            i.i32_const(1).local_set(hi);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hi).local_get(hn).i32_ge_u().br_if(1);
            i.local_get(hi).local_set(hj);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hj).i32_eqz().br_if(1);
            match k {
                INT | FLOAT => {
                    let key = |i: &mut wasm_encoder::InstructionSink<'_>, t: u32, float: bool| {
                        if float {
                            i.local_set(t);
                            i.local_get(t);
                            i.local_get(t).i64_const(63).i64_shr_s().i64_const(1).i64_shr_u();
                            i.i64_xor();
                        }
                    };
                    let float = k == FLOAT;
                    kaddr(&mut i, 1);
                    i.i64_load(slot_memarg(0));
                    key(&mut i, ht, float);
                    kaddr(&mut i, 0);
                    i.i64_load(slot_memarg(0));
                    key(&mut i, ht, float);
                    i.i64_le_s().br_if(1);
                }
                _ => {
                    kaddr(&mut i, 1);
                    i.i32_load(slot_memarg(0));
                    kaddr(&mut i, 0);
                    i.i32_load(slot_memarg(0));
                    i.call(F_STR_CMP).i32_const(0).i32_le_s().br_if(1);
                }
            }
            // swap keys then vals, both through the i64 bits scratch
            for (wide, addr) in [
                (kstride == 8, &kaddr as &dyn Fn(&mut wasm_encoder::InstructionSink<'_>, i32)),
                (vstride == 8, &vaddr),
            ] {
                addr(&mut i, 0);
                if wide {
                    i.i64_load(slot_memarg(0)).local_set(ht);
                } else {
                    i.i32_load(slot_memarg(0)).i64_extend_i32_u().local_set(ht);
                }
                addr(&mut i, 0);
                addr(&mut i, 1);
                if wide {
                    i.i64_load(slot_memarg(0)).i64_store(slot_memarg(0));
                } else {
                    i.i32_load(slot_memarg(0)).i32_store(slot_memarg(0));
                }
                addr(&mut i, 1);
                if wide {
                    i.local_get(ht).i64_store(slot_memarg(0));
                } else {
                    i.local_get(ht).i32_wrap_i64().i32_store(slot_memarg(0));
                }
            }
            i.local_get(hj).i32_const(1).i32_sub().local_set(hj);
            i.br(0).end().end();
            i.local_get(hi).i32_const(1).i32_add().local_set(hi);
            i.br(0).end().end();
            i.local_get(hb);
        }
        self.release_i64();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(SliceTy::List(h)))
    }
}
