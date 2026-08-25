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
                // select(v1, v2, cond) = cond ? v1 : v2 — the min form
                // pushes LEN as v1 under cond (n*stride > len). The
                // original operand order computed MAX and returned the
                // whole list; no claimed fixture observed take's VALUE
                // (fourth select inversion — the class now carries its
                // own fixture, els PR pending).
                i.local_get(hb).i32_load(len_memarg());
                i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u();
                i.local_get(hn).i64_const(stride as i64).i64_mul();
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
            ("remove_at", [xs, idx]) => self.lower_list_remove_at(xs, idx).map(Some),
            ("reverse", [xs]) => self.lower_list_reverse(xs).map(Some),
            ("drop_end", [xs, n]) => self.lower_list_drop_end(xs, n).map(Some),
            ("take_end", [xs, n]) => self.lower_list_take_end(xs, n).map(Some),
            // Bottom-up MERGE SORT over a fresh copy (O(n log n) — the
            // first perf measurement showed insertion sort 26x behind the
            // incumbent on 2k elements). Take-from-left on `<=` (stable);
            // scalar values make any correct sort value-identical to
            // native. The result may live in either ping-pong buffer —
            // both are layout-true blocks with the right len header.
            ("sort", [xs]) => {
                let h = match self.lower(xs, None)? {
                    SliceTy::List(h) => h,
                    other => return unsup(&format!("list-sort-of:{other:?}")),
                };
                let elem = self.types.el(h);
                if !matches!(elem, INT | FLOAT | STR | BOOL) {
                    return unsup(&format!("list-sort-elem:{elem:?}"));
                }
                self.f.instructions().call(F_BLOCK_COPY);
                self.emit_merge_sort(elem)?;
                Ok(Some(Some(SliceTy::List(h))))
            }
            ("chunk" | "windows", [xs, n_arg]) => {
                self.lower_list_chunk_windows(func, xs, n_arg).map(Some)
            }
            ("sort_by", [xs, cb]) => self.lower_list_sort_by(xs, cb).map(Some),
            // skip(n as usize): a NEGATIVE n reinterprets huge — EMPTY
            // (take's mirror keeps the WHOLE list; the asymmetry is v0's).
            ("drop", [xs, n]) => {
                let h = match self.lower(xs, None)? {
                    SliceTy::List(h) => h,
                    other => return unsup(&format!("list-drop-of:{other:?}")),
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
                // keep_bytes = n < 0 || n*stride >= len ? 0 : len - n*stride
                i.i32_const(0);
                i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u();
                i.local_get(hn).i64_const(stride as i64).i64_mul().i64_sub().i32_wrap_i64();
                i.local_get(hn).i64_const(0).i64_lt_s();
                i.local_get(hn)
                    .i64_const(stride as i64)
                    .i64_mul()
                    .local_get(hb)
                    .i32_load(len_memarg())
                    .i64_extend_i32_u()
                    .i64_ge_s();
                i.i32_or();
                i.select().local_set(hc);
                i.local_get(hc).call(F_ALLOC).local_set(ho);
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hb)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hb)
                    .i32_load(len_memarg())
                    .i32_add()
                    .local_get(hc)
                    .i32_sub();
                i.local_get(hc);
                i.call(F_COPY);
                i.local_get(ho);
                let _ = i;
                self.release_i32();
                self.release_i32();
                self.release_i64();
                self.release_i32();
                Ok(Some(Some(SliceTy::List(h))))
            }
            // insert at min(i as usize, len): a NEGATIVE index appends
            // at the END (the huge-usize reinterpretation, v0 verbatim).
            ("insert", [xs, idx, v]) => {
                let h = match self.lower(xs, None)? {
                    SliceTy::List(h) => h,
                    other => return unsup(&format!("list-insert-of:{other:?}")),
                };
                let elem = self.types.el(h);
                let stride = elem.slot_size() as i32;
                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                self.lower(idx, Some(INT))?;
                let hn = self.hold_i64()?;
                self.f.instructions().local_set(hn);
                self.lower(v, Some(elem))?;
                let hv = self.hold_val(elem)?;
                let hoff = self.hold_i32()?;
                let ho = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hv);
                // off_bytes = min(i, len_elems)*stride; negative → len
                i.local_get(hb).i32_load(len_memarg());
                i.local_get(hn).i64_const(stride as i64).i64_mul().i32_wrap_i64();
                i.local_get(hn)
                    .i64_const(0)
                    .i64_lt_s()
                    .local_get(hn)
                    .i64_const(stride as i64)
                    .i64_mul()
                    .local_get(hb)
                    .i32_load(len_memarg())
                    .i64_extend_i32_u()
                    .i64_gt_s()
                    .i32_or();
                i.select().local_set(hoff);
                i.local_get(hb).i32_load(len_memarg()).i32_const(stride).i32_add().call(F_ALLOC).local_set(ho);
                // prefix
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hoff);
                i.call(F_COPY);
                // the element
                i.local_get(ho).local_get(hoff).i32_add();
                i.local_get(hv);
                let _ = i;
                self.store_ty_slot(elem, 0);
                let mut i = self.f.instructions();
                // suffix
                i.local_get(ho)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hoff)
                    .i32_add()
                    .i32_const(stride)
                    .i32_add();
                i.local_get(hb)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hoff)
                    .i32_add();
                i.local_get(hb).i32_load(len_memarg()).local_get(hoff).i32_sub();
                i.call(F_COPY);
                i.local_get(ho);
                let _ = i;
                self.release_i32();
                self.release_i32();
                self.release_val(elem);
                self.release_i64();
                self.release_i32();
                Ok(Some(Some(SliceTy::List(h))))
            }
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

impl Emitter<'_> {
    /// `[block]` -> `[sorted block]`: ping-pong bottom-up merge sort.
    /// Comparison is take-from-left on `left <= right` in the scalar
    /// orders (Int/Bool signed-or-flag i64, Float via the totalOrder key
    /// transform, Str via $str_cmp).
    fn emit_merge_sort(&mut self, elem: SliceTy) -> Result<(), EmitError> {
        let stride = elem.slot_size() as i32;
        let ha = self.hold_i32()?;
        let hb2 = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hlo = self.hold_i32()?;
        let hmid = self.hold_i32()?;
        let hhi = self.hold_i32()?;
        let hi_ = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hsrc = self.scr_i32_local;
        let htmp = self.tmp_i32_local;
        {
            let mut i = self.f.instructions();
            i.local_set(ha);
            i.local_get(ha).i32_load(len_memarg()).i32_const(stride).i32_div_u().local_set(hn);
            i.local_get(hn).i32_const(stride).i32_mul().call(F_ALLOC).local_set(hb2);
            i.i32_const(1).local_set(hw);
            // while w < n
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hw).local_get(hn).i32_ge_u().br_if(1);
            i.i32_const(0).local_set(hlo);
            // for lo in steps of 2w
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hlo).local_get(hn).i32_ge_u().br_if(1);
            // mid = min(lo + w, n); hi = min(lo + 2w, n)
            i.local_get(hlo).local_get(hw).i32_add().local_set(hmid);
            i.local_get(hn)
                .local_get(hmid)
                .local_get(hmid)
                .local_get(hn)
                .i32_gt_u()
                .select()
                .local_set(hmid);
            i.local_get(hlo).local_get(hw).i32_const(1).i32_shl().i32_add().local_set(hhi);
            i.local_get(hn)
                .local_get(hhi)
                .local_get(hhi)
                .local_get(hn)
                .i32_gt_u()
                .select()
                .local_set(hhi);
            i.local_get(hlo).local_set(hi_);
            i.local_get(hmid).local_set(hj);
            i.local_get(hlo).local_set(hk);
            // merge [lo,mid) + [mid,hi) from A into B
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hk).local_get(hhi).i32_ge_u().br_if(1);
            // take_left = j >= hi || (i < mid && a[i] <= a[j])
            i.local_get(hj).local_get(hhi).i32_ge_u();
            i.if_(BlockType::Result(ValType::I32));
            i.i32_const(1);
            i.else_();
            i.local_get(hi_).local_get(hmid).i32_ge_u();
            i.if_(BlockType::Result(ValType::I32));
            i.i32_const(0);
            i.else_();
        }
        self.emit_le_flag(elem, ha, hi_, hj)?;
        {
            let mut i = self.f.instructions();
            i.end();
            i.end();
            // src = take_left ? i++ : j++
            i.if_(BlockType::Result(ValType::I32));
            i.local_get(hi_);
            i.local_get(hi_).i32_const(1).i32_add().local_set(hi_);
            i.else_();
            i.local_get(hj);
            i.local_get(hj).i32_const(1).i32_add().local_set(hj);
            i.end();
            i.local_set(hsrc);
            // B[k] = A[src]
            i.local_get(hb2).local_get(hk).i32_const(stride).i32_mul().i32_add();
            i.local_get(ha).local_get(hsrc).i32_const(stride).i32_mul().i32_add();
            if stride == 8 {
                i.i64_load(slot_memarg(0)).i64_store(slot_memarg(0));
            } else {
                i.i32_load(slot_memarg(0)).i32_store(slot_memarg(0));
            }
            i.local_get(hk).i32_const(1).i32_add().local_set(hk);
            i.br(0);
            i.end();
            i.end();
            i.local_get(hlo).local_get(hw).i32_const(1).i32_shl().i32_add().local_set(hlo);
            i.br(0);
            i.end();
            i.end();
            // swap A <-> B; w *= 2
            i.local_get(ha).local_set(htmp);
            i.local_get(hb2).local_set(ha);
            i.local_get(htmp).local_set(hb2);
            i.local_get(hw).i32_const(1).i32_shl().local_set(hw);
            i.br(0);
            i.end();
            i.end();
            i.local_get(ha);
        }
        for _ in 0..10 {
            self.release_i32();
        }
        Ok(())
    }

    /// Push `A[i] <= A[j]` as an i32 flag.
    fn emit_le_flag(&mut self, elem: SliceTy, ha: u32, hi_: u32, hj: u32) -> Result<(), EmitError> {
        let stride = elem.slot_size() as i32;
        let mut i = self.f.instructions();
        let addr = |i: &mut wasm_encoder::InstructionSink<'_>, idx: u32| {
            i.local_get(ha).local_get(idx).i32_const(stride).i32_mul().i32_add();
        };
        match elem {
            FLOAT => {
                let t = self.scr_i64_local;
                for idx in [hi_, hj] {
                    addr(&mut i, idx);
                    i.i64_load(slot_memarg(0)).local_set(t);
                    i.local_get(t);
                    i.local_get(t).i64_const(63).i64_shr_s().i64_const(1).i64_shr_u();
                    i.i64_xor();
                }
                i.i64_le_s();
            }
            INT => {
                addr(&mut i, hi_);
                i.i64_load(slot_memarg(0));
                addr(&mut i, hj);
                i.i64_load(slot_memarg(0));
                i.i64_le_s();
            }
            BOOL => {
                addr(&mut i, hi_);
                i.i32_load(slot_memarg(0));
                addr(&mut i, hj);
                i.i32_load(slot_memarg(0));
                i.i32_le_u();
            }
            _ => {
                addr(&mut i, hi_);
                i.i32_load(slot_memarg(0));
                addr(&mut i, hj);
                i.i32_load(slot_memarg(0));
                i.call(F_STR_CMP).i32_const(0).i32_le_s();
            }
        }
        Ok(())
    }
}
