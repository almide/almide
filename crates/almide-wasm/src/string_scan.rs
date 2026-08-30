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

/// The rle scanner's register file (holds + tuple layout), so the
/// flush helper takes one parameter.
struct RleRegs {
    hs: u32,
    hps: u32,
    hpw: u32,
    hcnt: u32,
    hacc: u32,
    hb: u32,
    tuple_size: u32,
    str_off: u32,
    cnt_off: u32,
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

    /// First char to_uppercase + rest verbatim (native capitalize).
    /// The 1:N SpecialCasing (ß→SS) rides the LINKED to_upper over the
    /// one-char prefix — one mapping table, two surfaces.
    pub(crate) fn lower_string_capitalize(
        &mut self,
        s: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let Some(fi) = self.resolve_qualified("string.to_upper") else {
            return unsup("capitalize:to-upper-unlinked");
        };
        let info = &self.table.infos[fi];
        if info.refuse.is_some() || info.ret != Some(STR) {
            return unsup("capitalize:to-upper-impl");
        }
        let upper_idx = info.wasm_index;
        self.calls.insert(fi);
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hs);
        i.local_get(hs).i32_load(len_memarg()).i32_eqz();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(0).call(F_ALLOC);
        i.else_();
        i.local_get(hs).i32_load8_u(str_byte()).local_set(hw);
        i.i32_const(1);
        i.local_get(hw).i32_const(0xC0).i32_ge_u().i32_add();
        i.local_get(hw).i32_const(0xE0).i32_ge_u().i32_add();
        i.local_get(hw).i32_const(0xF0).i32_ge_u().i32_add();
        i.local_set(hw);
        // first char as its own string → linked to_upper
        i.local_get(hw).call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hw);
        i.memory_copy(0, 0);
        i.local_get(hb).call(upper_idx);
        // rest verbatim
        i.local_get(hs).i32_load(len_memarg()).local_get(hw).i32_sub();
        i.call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hw)
            .i32_add();
        i.local_get(hs).i32_load(len_memarg()).local_get(hw).i32_sub();
        i.memory_copy(0, 0);
        i.local_get(hb).call(F_CONCAT);
        i.end();
        let _ = i;
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(STR))
    }

    /// One (char, count) tuple pushed onto the accumulator: the run's
    /// char is copied out of the source at [ps, ps+pw).
    fn emit_rle_flush(&mut self, r: &RleRegs) {
        let RleRegs { hs, hps, hpw, hcnt, hacc, hb, tuple_size, str_off, cnt_off } = *r;
        let mut i = self.f.instructions();
        i.local_get(hpw).call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hps)
            .i32_add();
        i.local_get(hpw);
        i.memory_copy(0, 0);
        // the pair block: (String, Int)
        i.i32_const(tuple_size as i32).call(F_ALLOC);
        i.local_tee(hps); // hps is dead for this run — reuse as the pair
        i.local_get(hb).i32_store(slot_memarg(str_off));
        i.local_get(hps).local_get(hcnt).i64_extend_i32_u().i64_store(slot_memarg(cnt_off));
        i.local_get(hacc).local_get(hps).call(F_LIST_PUSH_4).local_set(hacc);
    }

    /// CHAR-level runs (native: equal adjacent chars fold into a
    /// (char, count) pair, in order).
    pub(crate) fn lower_string_rle(&mut self, s: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let ti = self.types.tuple(vec![STR, INT]);
        let def = self.types.tuple_def(ti);
        let (str_off, cnt_off) = (def.fields[0].1, def.fields[1].1);
        let tuple_size = def.size;
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        let hl = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hps = self.hold_i32()?;
        let hpw = self.hold_i32()?;
        let hcnt = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let hb = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let hf = self.hold_i32()?;
        let regs = RleRegs { hs, hps, hpw, hcnt, hacc, hb, tuple_size, str_off, cnt_off };
        {
            let mut i = self.f.instructions();
            i.local_set(hs);
            i.local_get(hs).i32_load(len_memarg()).local_set(hl);
            i.i32_const(0).call(F_ALLOC).local_set(hacc);
            i.local_get(hl).if_(BlockType::Empty);
            // first run opens at 0
            i.local_get(hs).i32_load8_u(str_byte()).local_set(hpw);
            i.i32_const(1);
            i.local_get(hpw).i32_const(0xC0).i32_ge_u().i32_add();
            i.local_get(hpw).i32_const(0xE0).i32_ge_u().i32_add();
            i.local_get(hpw).i32_const(0xF0).i32_ge_u().i32_add();
            i.local_set(hpw);
            i.i32_const(0).local_set(hps);
            i.i32_const(1).local_set(hcnt);
            i.local_get(hpw).local_set(hk);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hk).local_get(hl).i32_ge_u().br_if(1);
            i.local_get(hs).local_get(hk).i32_add().i32_load8_u(str_byte()).local_set(hw);
            i.i32_const(1);
            i.local_get(hw).i32_const(0xC0).i32_ge_u().i32_add();
            i.local_get(hw).i32_const(0xE0).i32_ge_u().i32_add();
            i.local_get(hw).i32_const(0xF0).i32_ge_u().i32_add();
            i.local_set(hw);
            // same char? width equal AND every byte equal
            i.local_get(hw).local_get(hpw).i32_eq().local_set(hf);
            i.local_get(hf).if_(BlockType::Empty);
            i.i32_const(0).local_set(hj);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hj).local_get(hw).i32_ge_u().br_if(1);
            i.local_get(hs).local_get(hps).i32_add().local_get(hj).i32_add();
            i.i32_load8_u(str_byte());
            i.local_get(hs).local_get(hk).i32_add().local_get(hj).i32_add();
            i.i32_load8_u(str_byte());
            i.i32_ne().if_(BlockType::Empty);
            i.i32_const(0).local_set(hf);
            i.br(2);
            i.end();
            i.local_get(hj).i32_const(1).i32_add().local_set(hj);
            i.br(0).end().end();
            i.end();
            i.local_get(hf).if_(BlockType::Empty);
            i.local_get(hcnt).i32_const(1).i32_add().local_set(hcnt);
            i.else_();
        }
        self.emit_rle_flush(&regs);
        {
            let mut i = self.f.instructions();
            i.local_get(hk).local_set(hps);
            i.local_get(hw).local_set(hpw);
            i.i32_const(1).local_set(hcnt);
            i.end();
            i.local_get(hk).local_get(hw).i32_add().local_set(hk);
            i.br(0).end().end();
        }
        // the final run always exists (len > 0 guard)
        self.emit_rle_flush(&regs);
        self.f.instructions().end();
        self.f.instructions().local_get(hacc);
        for _ in 0..11 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(self.types.intern(SliceTy::Tuple(ti)))))
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

    /// First/last CHAR as some(String), none on empty (native
    /// `chars().next()/last()`). Last walks back over continuation
    /// bytes (0b10xxxxxx) to its lead.
    pub(crate) fn lower_string_first_last(
        &mut self,
        last: bool,
        s: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        let hp = self.hold_i32()?;
        let hl = self.hold_i32()?;
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hs);
        i.local_get(hs).i32_load(len_memarg()).i32_eqz();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(almide_layout::NULL_ADDR as i32);
        i.else_();
        if last {
            // p = len-1, back over continuations; the char spans p..len
            i.local_get(hs).i32_load(len_memarg()).i32_const(1).i32_sub().local_set(hp);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hp).i32_eqz().br_if(1);
            i.local_get(hs).local_get(hp).i32_add().i32_load8_u(str_byte());
            i.i32_const(0xC0).i32_and().i32_const(0x80).i32_ne().br_if(1);
            i.local_get(hp).i32_const(1).i32_sub().local_set(hp);
            i.br(0).end().end();
            i.local_get(hs).i32_load(len_memarg()).local_get(hp).i32_sub().local_set(hl);
        } else {
            i.i32_const(0).local_set(hp);
            i.local_get(hs).i32_load8_u(str_byte()).local_set(hl);
            i.i32_const(1);
            i.local_get(hl).i32_const(0xC0).i32_ge_u().i32_add();
            i.local_get(hl).i32_const(0xE0).i32_ge_u().i32_add();
            i.local_get(hl).i32_const(0xF0).i32_ge_u().i32_add();
            i.local_set(hl);
        }
        i.local_get(hl).call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_get(hp).i32_add();
        i.local_get(hl);
        i.memory_copy(0, 0);
        // some(str): a 4-byte option cell holding the handle
        i.i32_const(4).call(F_ALLOC).local_tee(hp).local_get(hb);
        i.i32_store(slot_memarg(almide_layout::OPTION_FIELD));
        i.local_get(hp);
        i.end();
        let _ = i;
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Option(self.types.intern(STR))))
    }

    /// Codepoint-wise reverse (native `chars().rev()`): each UTF-8
    /// sequence keeps its internal byte order, sequences swap ends.
    pub(crate) fn lower_string_reverse(&mut self, s: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let hl = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hs);
        i.local_get(hs).i32_load(len_memarg()).local_set(hn);
        i.local_get(hn).call(F_ALLOC).local_set(ho);
        i.i32_const(0).local_set(hc);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hc).local_get(hn).i32_ge_u().br_if(1);
        // seq length from the lead byte: 1 + (b≥C0) + (b≥E0) + (b≥F0)
        i.local_get(hs).local_get(hc).i32_add().i32_load8_u(str_byte()).local_set(hl);
        i.i32_const(1);
        i.local_get(hl).i32_const(0xC0).i32_ge_u().i32_add();
        i.local_get(hl).i32_const(0xE0).i32_ge_u().i32_add();
        i.local_get(hl).i32_const(0xF0).i32_ge_u().i32_add();
        i.local_set(hl);
        // dst = out.payload + (n − pos − len); src = in.payload + pos
        i.local_get(ho)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hn)
            .i32_add()
            .local_get(hc)
            .i32_sub()
            .local_get(hl)
            .i32_sub();
        i.local_get(hs).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_get(hc).i32_add();
        i.local_get(hl);
        i.memory_copy(0, 0);
        i.local_get(hc).local_get(hl).i32_add().local_set(hc);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(STR))
    }
}
