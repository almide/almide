//! Structural list edits (remove_at/reverse) — split from list_order.rs
//! for the complexity budget; list_order's dispatcher forwards here.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// Native `if (i as usize) < len { xs.remove(i) }`: OOB — negative
    /// wraps huge — is a NO-OP (C-034), never a trap. Two spans copied
    /// around the removed slot; OOB degenerates to span1 = whole,
    /// span2 = empty.
    pub(crate) fn lower_list_remove_at(
        &mut self,
        xs: &IrExpr,
        idx: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-remove-at-of:{other:?}")),
        };
        let stride = self.types.el(h).slot_size() as i32;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(idx, Some(INT))?;
        let hn = self.hold_i64()?;
        let hl = self.hold_i32()?;
        let hin = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hn);
        i.local_get(hb).i32_load(len_memarg()).local_set(hl);
        // in_bounds = 0 <= idx < len/stride (index domain — the
        // byte product would wrap for a huge idx)
        i.local_get(hn).i64_const(0).i64_ge_s();
        i.local_get(hn);
        i.local_get(hl).i32_const(stride).i32_div_u().i64_extend_i32_u();
        i.i64_lt_s().i32_and().local_set(hin);
        // k = in_bounds ? idx*stride : len  (select: v1 first)
        i.local_get(hn).i64_const(i64::from(stride)).i64_mul().i32_wrap_i64();
        i.local_get(hl);
        i.local_get(hin).select().local_set(hk);
        // skip = in_bounds ? stride : 0; out = alloc(len - skip)
        i.local_get(hl);
        i.i32_const(stride).i32_const(0).local_get(hin).select();
        i.i32_sub().call(F_ALLOC).local_set(ho);
        // span 1: [0, k)
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hk);
        i.memory_copy(0, 0);
        // span 2: [k+skip, len) lands at k
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hk).i32_add();
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hk).i32_add();
        i.i32_const(stride).i32_const(0).local_get(hin).select().i32_add();
        i.local_get(hl).local_get(hk).i32_sub();
        i.i32_const(stride).i32_const(0).local_get(hin).select().i32_sub();
        i.memory_copy(0, 0);
        i.local_get(ho);
        let _ = i;
        for _ in 0..5 {
            self.release_i32();
        }
        self.release_i64();
        Ok(Some(SliceTy::List(h)))
    }

    /// Fresh block, elements copied back-to-front (native `iter().rev()`);
    /// slot-width raw moves carry f64 bits and heap handles alike.
    pub(crate) fn lower_list_reverse(&mut self, xs: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-reverse-of:{other:?}")),
        };
        let stride = self.types.el(h).slot_size() as i32;
        let hb = self.hold_i32()?;
        let hl = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hb);
        i.local_get(hb).i32_load(len_memarg()).local_set(hl);
        i.local_get(hl).call(F_ALLOC).local_set(ho);
        i.i32_const(0).local_set(hc);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hc).local_get(hl).i32_ge_u().br_if(1);
        // dst = out + (len - stride - c); src elem at c —
        // slot_memarg supplies the PAYLOAD offset on both sides.
        i.local_get(ho)
            .local_get(hl)
            .i32_add()
            .i32_const(stride)
            .i32_sub()
            .local_get(hc)
            .i32_sub();
        i.local_get(hb).local_get(hc).i32_add();
        if stride == 8 {
            i.i64_load(slot_memarg(0)).i64_store(slot_memarg(0));
        } else {
            i.i32_load(slot_memarg(0)).i32_store(slot_memarg(0));
        }
        i.local_get(hc).i32_const(stride).i32_add().local_set(hc);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(h)))
    }
}
