//! The matrix floor, stage 1: constructors + shape reads on THIS
//! backend's layout. A Matrix block is FLAT — payload
//! `[rows:i32 @0][cols:i32 @4][f64 elements @8, row-major]` — no
//! row-pointer array (the native Vec-of-rows shape is an implementation
//! detail; the 2026-08-10 fuzz night showed row headers alone can OOM a
//! leg, which is exactly why `almide_rt_matrix_dims` bounds the ROW
//! count alone and not just the product).
//!
//! Dims rule (runtime/rs matrix.rs::almide_rt_matrix_dims, verbatim):
//! negative dimensions clamp to 0 (C-034 signed-clamp / C-161), then
//! `r > 2^28 || saturating(r*c) > 2^28` aborts in the T6 form —
//! `Error: matrix dimensions too large` + exit 1 — before any allocator
//! is asked for anything.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

const MAX_ELEMS: i64 = 1 << 28;

impl Emitter<'_> {
    /// `matrix.*` module calls (stage 1 set). Ok(None) = not handled here.
    fn lower_matrix_fill_ctor(&mut self, func: &str, r: &IrExpr, c: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        Ok({
            let ones = func == "ones";
            self.lower(r, Some(INT))?;
            let hr = self.hold_i64()?;
            self.f.instructions().local_set(hr);
            self.lower(c, Some(INT))?;
            let hc = self.hold_i64()?;
            let hb = self.hold_i32()?;
            let msg = self.pool.intern("matrix dimensions too large");
            {
                let mut i = self.f.instructions();
                i.local_set(hc);
                // clamp negatives to 0
                for h in [hr, hc] {
                    // select(v1, v2, cond) = cond ? v1 : v2 —
                    // negative clamps to 0 (C-034/C-161).
                    i.i64_const(0)
                        .local_get(h)
                        .local_get(h)
                        .i64_const(0)
                        .i64_lt_s()
                        .select()
                        .local_set(h);
                }
                // r > MAX  ||  r*c > MAX (r,c are in [0, 2^63); the
                // r-bound check makes the product overflow-free:
                // past it we abort before multiplying)
                i.local_get(hr).i64_const(MAX_ELEMS).i64_gt_s();
                i.if_(BlockType::Empty);
                i.i32_const(msg as i32);
            }
            self.emit_error_frame_abort();
            {
                let mut i = self.f.instructions();
                i.end();
                i.local_get(hr).local_get(hc).i64_mul().i64_const(MAX_ELEMS).i64_gt_s();
                i.if_(BlockType::Empty);
                i.i32_const(msg as i32);
            }
            self.emit_error_frame_abort();
            {
                let mut i = self.f.instructions();
                i.end();
                // rows == 0 forces cols to 0: native's `from_iter`
                // derives cols from the FIRST ROW, so a rowless
                // matrix has no cols — `cols(zeros(0, 9))` is 0.
                i.i64_const(0)
                    .local_get(hc)
                    .local_get(hr)
                    .i64_eqz()
                    .select()
                    .local_set(hc);
                // alloc 8 + r*c*8
                i.local_get(hr)
                    .local_get(hc)
                    .i64_mul()
                    .i64_const(8)
                    .i64_mul()
                    .i64_const(8)
                    .i64_add()
                    .i32_wrap_i64()
                    .call(F_ALLOC)
                    .local_set(hb);
                i.local_get(hb).local_get(hr).i32_wrap_i64().i32_store(slot_memarg(0));
                i.local_get(hb).local_get(hc).i32_wrap_i64().i32_store(slot_memarg(4));
            }
            if ones {
                // fill r*c f64 ones (fresh bump pages are zero, so
                // zeros needs no loop; ones walks the payload).
                let cur = self.hold_i32()?;
                let end = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_get(hb)
                    .i32_const(almide_layout::PAYLOAD as i32 + 8)
                    .i32_add()
                    .local_set(cur);
                i.local_get(cur)
                    .local_get(hr)
                    .local_get(hc)
                    .i64_mul()
                    .i64_const(8)
                    .i64_mul()
                    .i32_wrap_i64()
                    .i32_add()
                    .local_set(end);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
                i.local_get(cur).f64_const(1.0.into()).f64_store(
                    wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 },
                );
                i.local_get(cur).i32_const(8).i32_add().local_set(cur);
                i.br(0);
                i.end();
                i.end();
                let _ = i;
                self.release_i32();
                self.release_i32();
            }
            self.f.instructions().local_get(hb);
            self.release_i32();
            self.release_i64();
            self.release_i64();
            Some(SliceTy::Matrix)
        })
    }

    fn lower_matrix_shape(&mut self, m: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        Ok({
            self.lower(m, Some(SliceTy::Matrix))?;
            let hm = self.hold_i32()?;
            let hb = self.hold_i32()?;
            let pair = self.types.tuple(vec![INT, INT]);
            let def = self.types.tuple_def(pair);
            let (roff, coff) = (def.fields[0].1, def.fields[1].1);
            let size = def.size;
            let mut i = self.f.instructions();
            i.local_set(hm);
            i.i32_const(size as i32).call(F_ALLOC).local_set(hb);
            i.local_get(hb);
            i.local_get(hm).i32_load(slot_memarg(0)).i64_extend_i32_u();
            i.i64_store(slot_memarg(roff));
            i.local_get(hb);
            i.local_get(hm).i32_load(slot_memarg(4)).i64_extend_i32_u();
            i.i64_store(slot_memarg(coff));
            i.local_get(hb);
            let _ = i;
            self.release_i32();
            self.release_i32();
            Some(SliceTy::Tuple(pair))
        })
    }

    fn lower_matrix_dims_read(&mut self, func: &str, m: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        Ok({
            let off = if func == "rows" { 0 } else { 4 };
            self.lower(m, Some(SliceTy::Matrix))?;
            self.f
                .instructions()
                .i32_load(slot_memarg(off))
                .i64_extend_i32_u();
            Some(INT)
        })
    }

    fn lower_matrix_get(&mut self, m: &IrExpr, r: &IrExpr, c: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        Ok({
            self.lower(m, Some(SliceTy::Matrix))?;
            let hm = self.hold_i32()?;
            self.f.instructions().local_set(hm);
            self.lower(r, Some(INT))?;
            let hr = self.hold_i64()?;
            self.f.instructions().local_set(hr);
            self.lower(c, Some(INT))?;
            let hc = self.hold_i64()?;
            self.f.instructions().local_set(hc);
            // The index-domain rule (C-282): an accessor with no
            // identity value ABORTS out of range, in the same unified
            // form as `xs[i]`. Compared as i64 BEFORE any cast, so a
            // negative can never wrap past the test.
            let msg = self.pool.intern("matrix index out of bounds");
            for (idx, ext_off) in [(hr, 0), (hc, 4)] {
                {
                    let mut i = self.f.instructions();
                    i.local_get(idx).i64_const(0).i64_lt_s();
                    i.local_get(idx);
                    i.local_get(hm).i32_load(slot_memarg(ext_off)).i64_extend_i32_u();
                    i.i64_ge_s();
                    i.i32_or();
                    i.if_(BlockType::Empty);
                    i.i32_const(msg as i32);
                }
                self.emit_error_frame_abort();
                self.f.instructions().end();
            }
            {
                let mut i = self.f.instructions();
                i.local_get(hm);
                i.local_get(hr);
                i.local_get(hm).i32_load(slot_memarg(4)).i64_extend_i32_u();
                i.i64_mul().local_get(hc).i64_add().i32_wrap_i64();
                i.i32_const(3).i32_shl().i32_add();
                i.f64_load(slot_memarg(8));
            }
            self.release_i64();
            self.release_i64();
            self.release_i32();
            Some(FLOAT)
        })
    }

    fn lower_matrix_from_lists(&mut self, rows: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        Ok({
            let fh = self.types.intern(FLOAT);
            let inner = self.types.intern(SliceTy::List(fh));
            self.lower(rows, Some(SliceTy::List(inner)))?;
            let hl = self.hold_i32()?;
            let hr = self.hold_i32()?;
            let hc = self.hold_i32()?;
            let hb = self.hold_i32()?;
            let hi = self.hold_i32()?;
            let hsrc = self.hold_i32()?;
            let hn = self.hold_i32()?;
            let hdst = self.hold_i32()?;
            let hj = self.hold_i32()?;
            let mut i = self.f.instructions();
            i.local_tee(hl);
            // r = list count; c = the FIRST row's width if any (native
            // from_iter), else 0.
            i.i32_load(len_memarg()).i32_const(2).i32_shr_u().local_set(hr);
            i.i32_const(0).local_set(hc);
            i.local_get(hr).if_(BlockType::Empty);
            i.local_get(hl)
                .i32_load(slot_memarg(0))
                .i32_load(len_memarg())
                .i32_const(3)
                .i32_shr_u()
                .local_set(hc);
            i.end();
            // alloc 8 + r*c*8 (i64 math: the ragged-degenerate product
            // can exceed i32 even though well-formed inputs cannot)
            i.local_get(hr)
                .i64_extend_i32_u()
                .local_get(hc)
                .i64_extend_i32_u()
                .i64_mul()
                .i64_const(8)
                .i64_mul()
                .i64_const(8)
                .i64_add()
                .i32_wrap_i64()
                .call(F_ALLOC)
                .local_set(hb);
            i.local_get(hb).local_get(hr).i32_store(slot_memarg(0));
            i.local_get(hb).local_get(hc).i32_store(slot_memarg(4));
            // Per row, copy min(width, c) elements; a SHORT row's tail
            // stays zero from the fresh pages. (Native flattens ragged
            // rows into misaligned data — self-inconsistent and pinned
            // by no fixture; zero-fill/truncate is the deterministic
            // reading of "cols comes from the first row".)
            i.local_get(hb)
                .i32_const(almide_layout::PAYLOAD as i32 + 8)
                .i32_add()
                .local_set(hdst);
            i.i32_const(0).local_set(hi);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hi).local_get(hr).i32_ge_u().br_if(1);
            i.local_get(hl)
                .local_get(hi)
                .i32_const(2)
                .i32_shl()
                .i32_add()
                .i32_load(slot_memarg(0))
                .local_set(hsrc);
            i.local_get(hsrc).i32_load(len_memarg()).i32_const(3).i32_shr_u().local_set(hn);
            i.local_get(hn)
                .local_get(hc)
                .local_get(hn)
                .local_get(hc)
                .i32_lt_u()
                .select()
                .local_set(hn);
            i.i32_const(0).local_set(hj);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hj).local_get(hn).i32_ge_u().br_if(1);
            i.local_get(hdst).local_get(hj).i32_const(3).i32_shl().i32_add();
            i.local_get(hsrc).local_get(hj).i32_const(3).i32_shl().i32_add();
            i.f64_load(slot_memarg(0));
            i.f64_store(raw8());
            i.local_get(hj).i32_const(1).i32_add().local_set(hj);
            i.br(0);
            i.end();
            i.end();
            i.local_get(hdst).local_get(hc).i32_const(3).i32_shl().i32_add().local_set(hdst);
            i.local_get(hi).i32_const(1).i32_add().local_set(hi);
            i.br(0);
            i.end();
            i.end();
            i.local_get(hb);
            let _ = i;
            for _ in 0..9 {
                self.release_i32();
            }
            Some(SliceTy::Matrix)
        })
    }

    fn lower_matrix_to_lists(&mut self, m: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        Ok({
            self.lower(m, Some(SliceTy::Matrix))?;
            let hm = self.hold_i32()?;
            let hr = self.hold_i32()?;
            let hc8 = self.hold_i32()?;
            let ho = self.hold_i32()?;
            let hi = self.hold_i32()?;
            let hrow = self.hold_i32()?;
            let hsrc = self.hold_i32()?;
            let hj = self.hold_i32()?;
            let mut i = self.f.instructions();
            i.local_tee(hm);
            i.i32_load(slot_memarg(0)).local_set(hr);
            i.local_get(hm).i32_load(slot_memarg(4)).i32_const(3).i32_shl().local_set(hc8);
            i.local_get(hr).i32_const(2).i32_shl().call(F_ALLOC).local_set(ho);
            i.local_get(hm)
                .i32_const(almide_layout::PAYLOAD as i32 + 8)
                .i32_add()
                .local_set(hsrc);
            i.i32_const(0).local_set(hi);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hi).local_get(hr).i32_ge_u().br_if(1);
            // every row is a FRESH List[Float] block (native to_vec
            // copies; sharing would let a later matrix op alias in)
            i.local_get(hc8).call(F_ALLOC).local_set(hrow);
            i.i32_const(0).local_set(hj);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hj).local_get(hc8).i32_ge_u().br_if(1);
            i.local_get(hrow).local_get(hj).i32_add();
            i.local_get(hsrc).local_get(hj).i32_add();
            i.f64_load(raw8());
            i.f64_store(slot_memarg(0));
            i.local_get(hj).i32_const(8).i32_add().local_set(hj);
            i.br(0);
            i.end();
            i.end();
            i.local_get(ho)
                .local_get(hi)
                .i32_const(2)
                .i32_shl()
                .i32_add()
                .local_get(hrow)
                .i32_store(slot_memarg(0));
            i.local_get(hsrc).local_get(hc8).i32_add().local_set(hsrc);
            i.local_get(hi).i32_const(1).i32_add().local_set(hi);
            i.br(0);
            i.end();
            i.end();
            i.local_get(ho);
            let _ = i;
            for _ in 0..8 {
                self.release_i32();
            }
            let fh = self.types.intern(FLOAT);
            let inner = self.types.intern(SliceTy::List(fh));
            Some(SliceTy::List(inner))
        })
    }

    fn lower_matrix_transpose(&mut self, m: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        Ok({
            self.lower(m, Some(SliceTy::Matrix))?;
            let hm = self.hold_i32()?;
            let hr = self.hold_i32()?;
            let hc = self.hold_i32()?;
            let hb = self.hold_i32()?;
            let hi = self.hold_i32()?;
            let hj = self.hold_i32()?;
            let hsrc = self.hold_i32()?;
            let mut i = self.f.instructions();
            i.local_tee(hm);
            i.i32_load(slot_memarg(0)).local_set(hr);
            i.local_get(hm).i32_load(slot_memarg(4)).local_set(hc);
            // either dim 0 → the (0, 0) matrix (native mk(0, 0));
            // zeroing BOTH makes the header write and the loops uniform
            i.local_get(hr).i32_eqz().local_get(hc).i32_eqz().i32_or();
            i.if_(BlockType::Empty);
            i.i32_const(0).local_set(hr);
            i.i32_const(0).local_set(hc);
            i.end();
            // r*c is constructor-ceiling-bounded, so i32 math holds
            i.local_get(hr)
                .local_get(hc)
                .i32_mul()
                .i32_const(3)
                .i32_shl()
                .i32_const(8)
                .i32_add()
                .call(F_ALLOC)
                .local_set(hb);
            i.local_get(hb).local_get(hc).i32_store(slot_memarg(0));
            i.local_get(hb).local_get(hr).i32_store(slot_memarg(4));
            // src walks the input row-major once; out[j*r + i] =
            // in[i*c + j]. A pure permutation — bit-exact against the
            // kernel by construction, no arithmetic to reassociate.
            i.local_get(hm)
                .i32_const(almide_layout::PAYLOAD as i32 + 8)
                .i32_add()
                .local_set(hsrc);
            i.i32_const(0).local_set(hi);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hi).local_get(hr).i32_ge_u().br_if(1);
            i.i32_const(0).local_set(hj);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hj).local_get(hc).i32_ge_u().br_if(1);
            i.local_get(hb);
            i.local_get(hj)
                .local_get(hr)
                .i32_mul()
                .local_get(hi)
                .i32_add()
                .i32_const(3)
                .i32_shl()
                .i32_add();
            i.local_get(hsrc).f64_load(raw8());
            i.f64_store(slot_memarg(8));
            i.local_get(hsrc).i32_const(8).i32_add().local_set(hsrc);
            i.local_get(hj).i32_const(1).i32_add().local_set(hj);
            i.br(0);
            i.end();
            i.end();
            i.local_get(hi).i32_const(1).i32_add().local_set(hi);
            i.br(0);
            i.end();
            i.end();
            i.local_get(hb);
            let _ = i;
            for _ in 0..7 {
                self.release_i32();
            }
            Some(SliceTy::Matrix)
        })
    }

    fn lower_matrix_row_dot(&mut self, m: &IrExpr, r: &IrExpr, v: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        Ok({
            self.lower(m, Some(SliceTy::Matrix))?;
            let hm = self.hold_i32()?;
            self.f.instructions().local_set(hm);
            self.lower(r, Some(INT))?;
            let hr = self.hold_i64()?;
            self.f.instructions().local_set(hr);
            let fh = self.types.intern(FLOAT);
            self.lower(v, Some(SliceTy::List(fh)))?;
            let hv = self.hold_i32()?;
            let hs = self.hold_f64()?;
            let hsrc = self.hold_i32()?;
            let hvp = self.hold_i32()?;
            let hn = self.hold_i32()?;
            let hk = self.hold_i32()?;
            let mut i = self.f.instructions();
            i.local_set(hv);
            // A REDUCTION over a row: its empty-sum identity is 0.0,
            // so an out-of-range (or negative) row ANSWERS it — never
            // aborts (C-282, the accessor/reduction split).
            i.f64_const(0.0.into()).local_set(hs);
            i.local_get(hr).i64_const(0).i64_ge_s();
            i.local_get(hr);
            i.local_get(hm).i32_load(slot_memarg(0)).i64_extend_i32_u();
            i.i64_lt_s();
            i.i32_and();
            i.if_(BlockType::Empty);
            // n = min(cols, vec count)
            i.local_get(hm).i32_load(slot_memarg(4)).local_set(hn);
            i.local_get(hv).i32_load(len_memarg()).i32_const(3).i32_shr_u().local_set(hk);
            i.local_get(hn)
                .local_get(hk)
                .local_get(hn)
                .local_get(hk)
                .i32_lt_u()
                .select()
                .local_set(hn);
            i.local_get(hm).i32_const(almide_layout::PAYLOAD as i32 + 8).i32_add();
            i.local_get(hr).i32_wrap_i64();
            i.local_get(hm).i32_load(slot_memarg(4)).i32_mul().i32_const(3).i32_shl();
            i.i32_add().local_set(hsrc);
            i.local_get(hv).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hvp);
            i.i32_const(0).local_set(hk);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hk).local_get(hn).i32_ge_u().br_if(1);
            // s += row[k] * vec[k]: mul-then-add, k ascending —
            // exactly the native scalar loop (no fma, no reassociation)
            i.local_get(hs);
            i.local_get(hsrc).local_get(hk).i32_const(3).i32_shl().i32_add();
            i.f64_load(raw8());
            i.local_get(hvp).local_get(hk).i32_const(3).i32_shl().i32_add();
            i.f64_load(raw8());
            i.f64_mul().f64_add().local_set(hs);
            i.local_get(hk).i32_const(1).i32_add().local_set(hk);
            i.br(0);
            i.end();
            i.end();
            i.end();
            i.local_get(hs);
            let _ = i;
            self.release_i32();
            self.release_i32();
            self.release_i32();
            self.release_i32();
            self.release_f64();
            self.release_i32();
            self.release_i64();
            self.release_i32();
            Some(FLOAT)
        })
    }

    pub(crate) fn lower_matrix_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let out = match (func, args) {
            ("zeros" | "ones", [r, c]) => self.lower_matrix_fill_ctor(func, r, c)?,
            ("shape", [m]) => self.lower_matrix_shape(m)?,
            ("rows" | "cols", [m]) => self.lower_matrix_dims_read(func, m)?,
            ("get", [m, r, c]) => self.lower_matrix_get(m, r, c)?,
            ("from_lists", [rows]) => self.lower_matrix_from_lists(rows)?,
            ("to_lists", [m]) => self.lower_matrix_to_lists(m)?,
            ("transpose", [m]) => self.lower_matrix_transpose(m)?,
            ("row_dot" | "dot_row", [m, r, v]) => self.lower_matrix_row_dot(m, r, v)?,
            // Kernel families, shape-grouped (name split in the subs).
            ("gelu" | "softmax_rows", [m]) => {
                return if func == "gelu" {
                    self.lower_matrix_elementwise("gelu", m, None).map(Some)
                } else {
                    self.lower_matrix_softmax(m).map(Some)
                }
            }
            ("pow", [m, e]) => {
                return self.lower_matrix_elementwise("pow", m, Some(e)).map(Some)
            }
            ("rms_norm_rows", [m, g, eps]) => {
                return self.lower_matrix_rms_norm(m, g, eps).map(Some)
            }
            ("select_rows", [m, ids]) => {
                return self.lower_matrix_select_rows(m, ids).map(Some)
            }
            (
                "from_bytes_f32_le" | "from_bytes_f16_le" | "select_rows_f32"
                | "from_q1_0_bytes" | "select_rows_q1_0" | "select_rows_q8_0_dq",
                [a, b, c, d],
            ) => return self.lower_matrix_loader(func, a, b, c, d).map(Some),
            ("rope_rotate", [x, nh, hd, th]) => {
                return self.lower_matrix_rope(false, x, nh, hd, th, None).map(Some)
            }
            ("rope_rotate_at" | "rope_rotate_neox_at", [x, nh, hd, th, sp]) => {
                let neox = func == "rope_rotate_neox_at";
                return self.lower_matrix_rope(neox, x, nh, hd, th, Some(sp)).map(Some)
            }
            ("multi_head_attention" | "masked_multi_head_attention", [q, k, v, nh]) => {
                let causal = func == "masked_multi_head_attention";
                return self.lower_matrix_mha(causal, q, k, v, nh).map(Some)
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }
}

/// Raw absolute-address f64 access (block bases are 4-aligned, hint 2).
fn raw8() -> wasm_encoder::MemArg {
    wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 }
}
