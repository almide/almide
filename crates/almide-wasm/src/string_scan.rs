//! Byte-scanner string surfaces (pad pair / lines / chars / codepoint)
//! — split from string_ext.rs for the complexity budget. All scanners
//! walk UTF-8 by lead-byte class; continuation bytes are never a cursor
//! position on valid UTF-8.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, MemArg, ValType};

use crate::emitter::Emitter;
use crate::*;

/// Payload-relative byte address (align 0 — a byte load may not carry
/// the 4-byte hint slot_memarg advertises).
pub(crate) fn str_byte() -> MemArg {
    MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 }
}

impl Emitter<'_> {
    /// stdlib/string_pad.almd verbatim: pad with copies of the FIRST
    /// codepoint of `pad` (a space if empty) until the CODEPOINT count
    /// reaches width; already wide → a copy of s.
    pub(crate) fn lower_string_pad(
        &mut self,
        s: &IrExpr,
        w: &IrExpr,
        pad: &IrExpr,
        at_start: bool,
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        self.f.instructions().local_set(hs);
        self.lower(w, Some(INT))?;
        let hw = self.hold_i64()?;
        self.f.instructions().local_set(hw);
        self.lower(pad, Some(STR))?;
        let hp = self.hold_i32()?;
        let hl = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hsc = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hp);
        i.local_get(hs).i32_load(len_memarg()).local_set(hl);
        // scount: bytes that are NOT continuations (b>>6 != 2)
        i.i32_const(0).local_set(hsc);
        i.i32_const(0).local_set(hk);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hk).local_get(hl).i32_ge_u().br_if(1);
        i.local_get(hsc);
        i.local_get(hs).local_get(hk).i32_add().i32_load8_u(str_byte());
        i.i32_const(6).i32_shr_u().i32_const(2).i32_ne().i32_add();
        i.local_set(hsc);
        i.local_get(hk).i32_const(1).i32_add().local_set(hk);
        i.br(0).end().end();
        // wide enough → a fresh copy of s
        i.local_get(hw).local_get(hsc).i64_extend_i32_u().i64_le_s();
        i.if_(BlockType::Result(ValType::I32));
        i.local_get(hl).call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hl);
        i.memory_copy(0, 0);
        i.local_get(hb);
        i.else_();
        i.local_get(hw).i32_wrap_i64().local_get(hsc).i32_sub().local_set(hn);
        // pclen into hk: empty pad → 1 (a space)
        i.local_get(hp).i32_load(len_memarg()).i32_eqz();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(1);
        i.else_();
        i.local_get(hp).i32_load8_u(str_byte()).local_set(hk);
        i.i32_const(1);
        i.local_get(hk).i32_const(0xC0).i32_ge_u().i32_add();
        i.local_get(hk).i32_const(0xE0).i32_ge_u().i32_add();
        i.local_get(hk).i32_const(0xF0).i32_ge_u().i32_add();
        i.end();
        i.local_set(hk);
        i.local_get(hn).local_get(hk).i32_mul().local_get(hl).i32_add();
        i.call(F_ALLOC).local_set(hb);
        // s goes after the pads (start) or first (end); hsc
        // becomes the write cursor
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        if !at_start {
            i.local_tee(hsc);
            i.local_get(hs).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hl);
            i.memory_copy(0, 0);
            i.local_get(hsc).local_get(hl).i32_add();
        }
        i.local_set(hsc);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hn).i32_const(0).i32_le_s().br_if(1);
        i.local_get(hp).i32_load(len_memarg()).i32_eqz();
        i.if_(BlockType::Empty);
        i.local_get(hsc).i32_const(32).i32_store8(MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        });
        i.else_();
        i.local_get(hsc);
        i.local_get(hp).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hk);
        i.memory_copy(0, 0);
        i.end();
        i.local_get(hsc).local_get(hk).i32_add().local_set(hsc);
        i.local_get(hn).i32_const(1).i32_sub().local_set(hn);
        i.br(0).end().end();
        if at_start {
            i.local_get(hsc);
            i.local_get(hs).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hl);
            i.memory_copy(0, 0);
        }
        i.local_get(hb);
        i.end();
        let _ = i;
        for _ in 0..6 {
            self.release_i32();
        }
        self.release_i64();
        self.release_i32();
        Ok(Some(STR))
    }

    /// Rust str::lines verbatim: split at '\n', strip the '\r' of a
    /// "\r\n" pair, no entry for a trailing newline; a lone trailing
    /// '\r' stays in its line.
    pub(crate) fn lower_string_lines(&mut self, s: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        let hl = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hst = self.hold_i32()?;
        let he = self.hold_i32()?;
        let hb = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hs);
        i.local_get(hs).i32_load(len_memarg()).local_set(hl);
        i.i32_const(0).call(F_ALLOC).local_set(hacc);
        i.i32_const(0).local_set(hk);
        i.i32_const(0).local_set(hst);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hk).local_get(hl).i32_ge_u().br_if(1);
        i.local_get(hs).local_get(hk).i32_add().i32_load8_u(str_byte());
        i.i32_const(10).i32_eq().if_(BlockType::Empty);
        i.local_get(hk).local_set(he);
        // strip the '\r' of a CRLF pair
        i.local_get(he).local_get(hst).i32_gt_u();
        i.if_(BlockType::Empty);
        i.local_get(hs)
            .local_get(he)
            .i32_add()
            .i32_const(1)
            .i32_sub()
            .i32_load8_u(str_byte());
        i.i32_const(13).i32_eq().if_(BlockType::Empty);
        i.local_get(he).i32_const(1).i32_sub().local_set(he);
        i.end();
        i.end();
        i.local_get(he).local_get(hst).i32_sub().call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hst)
            .i32_add();
        i.local_get(he).local_get(hst).i32_sub();
        i.memory_copy(0, 0);
        i.local_get(hacc).local_get(hb).call(F_LIST_PUSH_4).local_set(hacc);
        i.local_get(hk).i32_const(1).i32_add().local_set(hst);
        i.end();
        i.local_get(hk).i32_const(1).i32_add().local_set(hk);
        i.br(0).end().end();
        // the final segment (no trailing newline)
        i.local_get(hst).local_get(hl).i32_lt_u().if_(BlockType::Empty);
        i.local_get(hl).local_get(hst).i32_sub().call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hst)
            .i32_add();
        i.local_get(hl).local_get(hst).i32_sub();
        i.memory_copy(0, 0);
        i.local_get(hacc).local_get(hb).call(F_LIST_PUSH_4).local_set(hacc);
        i.end();
        i.local_get(hacc);
        let _ = i;
        for _ in 0..7 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(self.types.intern(STR))))
    }

    /// One-char strings in order (native s.chars()).
    pub(crate) fn lower_string_chars(&mut self, s: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        let hl = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hb = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hs);
        i.local_get(hs).i32_load(len_memarg()).local_set(hl);
        i.i32_const(0).call(F_ALLOC).local_set(hacc);
        i.i32_const(0).local_set(hk);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hk).local_get(hl).i32_ge_u().br_if(1);
        // w = 1 + (b>=0xC0) + (b>=0xE0) + (b>=0xF0)
        i.local_get(hs).local_get(hk).i32_add().i32_load8_u(str_byte()).local_set(hw);
        i.i32_const(1);
        i.local_get(hw).i32_const(0xC0).i32_ge_u().i32_add();
        i.local_get(hw).i32_const(0xE0).i32_ge_u().i32_add();
        i.local_get(hw).i32_const(0xF0).i32_ge_u().i32_add();
        i.local_set(hw);
        i.local_get(hw).call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hk)
            .i32_add();
        i.local_get(hw);
        i.memory_copy(0, 0);
        i.local_get(hacc).local_get(hb).call(F_LIST_PUSH_4).local_set(hacc);
        i.local_get(hk).local_get(hw).i32_add().local_set(hk);
        i.br(0).end().end();
        i.local_get(hacc);
        let _ = i;
        for _ in 0..6 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(self.types.intern(STR))))
    }

    /// First char's codepoint (native `chars().next()`): "" → none.
    /// The lead byte classes the width; continuations add 6 bits.
    pub(crate) fn lower_string_codepoint(
        &mut self,
        s: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hcp = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hs);
        i.local_get(hs).i32_load(len_memarg()).i32_eqz();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(0);
        i.else_();
        i.local_get(hs).i32_load8_u(str_byte()).local_set(hcp);
        i.i32_const(1);
        i.local_get(hcp).i32_const(0xC0).i32_ge_u().i32_add();
        i.local_get(hcp).i32_const(0xE0).i32_ge_u().i32_add();
        i.local_get(hcp).i32_const(0xF0).i32_ge_u().i32_add();
        i.local_set(hw);
        // lead mask: w=1 keeps all 7 bits; w>1 masks 0xFF >> (w+1)
        i.local_get(hcp);
        i.i32_const(0xFF);
        i.local_get(hw).i32_const(1).i32_add();
        i.local_get(hw).i32_const(1).i32_gt_u().i32_mul();
        i.i32_shr_u();
        i.i32_and().local_set(hcp);
        i.i32_const(1).local_set(hk);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hk).local_get(hw).i32_ge_u().br_if(1);
        i.local_get(hcp).i32_const(6).i32_shl();
        i.local_get(hs).local_get(hk).i32_add().i32_load8_u(str_byte());
        i.i32_const(0x3F).i32_and().i32_or().local_set(hcp);
        i.local_get(hk).i32_const(1).i32_add().local_set(hk);
        i.br(0).end().end();
        // some(cp): an 8-byte option cell holding the i64
        i.i32_const(8).call(F_ALLOC).local_tee(hk);
        i.local_get(hcp).i64_extend_i32_u();
        i.i64_store(slot_memarg(almide_layout::OPTION_FIELD));
        i.local_get(hk);
        i.end();
        let _ = i;
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Option(self.types.intern(INT))))
    }
}
