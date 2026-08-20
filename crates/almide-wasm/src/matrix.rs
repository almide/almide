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
    pub(crate) fn lower_matrix_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let out = match (func, args) {
            ("zeros", [r, c]) | ("ones", [r, c]) => {
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
            }
            ("shape", [m]) => {
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
            }
            ("rows", [m]) | ("cols", [m]) => {
                let off = if func == "rows" { 0 } else { 4 };
                self.lower(m, Some(SliceTy::Matrix))?;
                self.f
                    .instructions()
                    .i32_load(slot_memarg(off))
                    .i64_extend_i32_u();
                Some(INT)
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }
}
