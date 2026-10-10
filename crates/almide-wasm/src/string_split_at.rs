//! The split_once kernel cut in three (split_match.rs): the first byte
//! occurrence of the separator, and the two pieces around it — the
//! self-hosted `string_split_once` body (stdlib/string_split_once.almd),
//! byte for byte, minus the option cell and the tuple it would wrap them in.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::string_scan::str_byte;
use crate::*;

impl Emitter<'_> {
    /// `split_once#at(s, sep)`: the byte index of the first occurrence of
    /// `sep` in `s`, or -1. An empty `sep` hits at 0 (Rust semantics).
    pub(crate) fn lower_split_at(&mut self, s: &IrExpr, sep: &IrExpr) -> ArmResult {
        self.lower_arg(s, Some(STR), ArgMode::Borrow)?;
        let hs = self.hold_i32()?;
        self.f.instructions().local_set(hs);
        self.lower_arg(sep, Some(STR), ArgMode::Borrow)?;
        let hp = self.hold_i32()?;
        let (hpos, hj, hr) = (self.hold_i32()?, self.hold_i32()?, self.hold_i32()?);
        let mut i = self.f.instructions();
        i.local_set(hp);
        i.i32_const(-1).local_set(hr).i32_const(0).local_set(hpos);
        // depths inside the inner loop: 0 inner, 1 miss, 2 outer, 3 done
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        // the separator no longer fits: absent
        i.local_get(hpos).local_get(hp).i32_load(len_memarg()).i32_add();
        i.local_get(hs).i32_load(len_memarg()).i32_gt_u().br_if(1);
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hp).i32_load(len_memarg()).i32_ge_u();
        i.if_(BlockType::Empty).local_get(hpos).local_set(hr).br(4).end();
        i.local_get(hs).local_get(hpos).i32_add().local_get(hj).i32_add().i32_load8_u(str_byte());
        i.local_get(hp).local_get(hj).i32_add().i32_load8_u(str_byte());
        i.i32_ne().br_if(1);
        i.local_get(hj).i32_const(1).i32_add().local_set(hj).br(0);
        i.end().end();
        i.local_get(hpos).i32_const(1).i32_add().local_set(hpos).br(0);
        i.end().end();
        i.local_get(hr).i64_extend_i32_s();
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(Lowered::scalar(INT)))
    }

    /// `split_once#head(s, at)` = a fresh `s[0 .. at]`, and
    /// `split_once#tail(s, sep, at)` = a fresh `s[at + len(sep) ..]` (`sep`
    /// `None`: the head).
    pub(crate) fn lower_split_piece(&mut self, s: &IrExpr, sep: Option<&IrExpr>, at: &IrExpr) -> ArmResult {
        self.lower_arg(s, Some(STR), ArgMode::Borrow)?;
        let hs = self.hold_i32()?;
        self.f.instructions().local_set(hs);
        // the piece's first byte
        let hstart = self.hold_i32()?;
        match sep {
            Some(sep) => {
                self.lower_arg(sep, Some(STR), ArgMode::Borrow)?;
                self.f.instructions().i32_load(len_memarg());
                self.lower_arg(at, Some(INT), ArgMode::Borrow)?;
                self.f.instructions().i32_wrap_i64().i32_add().local_set(hstart);
            }
            None => {
                self.f.instructions().i32_const(0).local_set(hstart);
            }
        }
        let hn = self.hold_i32()?;
        if sep.is_none() {
            self.lower_arg(at, Some(INT), ArgMode::Borrow)?;
            self.f.instructions().i32_wrap_i64().local_set(hn);
        } else {
            self.f.instructions().local_get(hs).i32_load(len_memarg()).local_get(hstart).i32_sub().local_set(hn);
        }
        let hk = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_get(hn).call(F_ALLOC).local_set(hk);
        i.local_get(hk).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_get(hstart).i32_add();
        i.local_get(hn);
        i.memory_copy(0, 0);
        i.local_get(hk);
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(STR)))
    }
}
