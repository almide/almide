//! The split_once kernel cut in three (split_match.rs): the first byte
//! occurrence of the separator, and the two pieces around it — the
//! self-hosted `string_split_once` body (stdlib/string_search.almd),
//! result for result, minus the option cell and the tuple it would wrap them in.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
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
        let (hn, hm, hr) = (self.hold_i32()?, self.hold_i32()?, self.hold_i32()?);
        let (ht, hk, hc, hq) = (self.hold_i32()?, self.hold_i32()?, self.hold_i32()?, self.hold_i32()?);
        let mut i = self.f.instructions();
        i.local_set(hp);
        i.local_get(hs).i32_load(len_memarg()).local_set(hn);
        i.local_get(hp).i32_load(len_memarg()).local_set(hm);
        // An empty separator hits at 0; otherwise one KMP search (crate::kmp,
        // #4206), whose result is -1 when the separator is absent.
        i.i32_const(0).local_set(hr);
        i.local_get(hm).if_(BlockType::Empty);
        let kmp = crate::kmp::Kmp { hay: hs, n: hn, pat: hp, m: hm, tbl: ht, k: hk, c: hc, q: hq };
        kmp.emit_table(&mut i);
        i.i32_const(0).local_set(hq);
        kmp.emit_find(&mut i, hr);
        kmp.emit_free(&mut i);
        i.end();
        i.local_get(hr).i64_extend_i32_s();
        for _ in 0..9 {
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
