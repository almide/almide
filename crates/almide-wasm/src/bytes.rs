//! Bytes — the byte-packed buffer surface (String's layout twin). The
//! oracle allows in-place `set_*` (a `mut`/buffer API); under the
//! bind-deep-copy doctrine a local's block is uniquely its own, so the
//! stores are unobservable through aliases. `bytes.new` relies on the
//! bump allocator's zero guarantee (fresh pages are zero and the bump
//! head never reuses).

use almide_ir::{IrExpr, IrExprKind};
use wasm_encoder::{BlockType, MemArg, ValType};

use crate::emitter::Emitter;
use crate::*;

pub(crate) const BYTES: SliceTy = SliceTy::Scalar(Scalar::Bytes);

pub(crate) fn byte_at() -> MemArg {
    MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 }
}

/// Payload-relative byte address: byte k of the current window.
pub(crate) fn byte_k(k: u8) -> MemArg {
    MemArg {
        offset: u64::from(almide_layout::PAYLOAD) + u64::from(k),
        align: 0,
        memory_index: 0,
    }
}

impl Emitter<'_> {

    fn lower_bytes_new(&mut self, n: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(n, Some(INT))?;
        // native `len.max(0)` — a NEGATIVE size is the empty buffer,
        // never a wrapped 4 GiB ask.
        let h = self.hold_i64()?;
        let oom = self.pool.intern("Error: out of memory");
        let mut i = self.f.instructions();
        i.local_set(h);
        i.i64_const(0).local_get(h).local_get(h).i64_const(0).i64_lt_s().select().local_set(h);
        // Judged in i64 BEFORE the i32 wrap (the bytes_repeat bound):
        // past the structural limit is the C-197 die. The old die was
        // ACCIDENTAL — the bind's deep copy re-read the wrapped length
        // and ITS alloc failed; RC-5's share-at-bind removed that copy
        // and exposed the naked wrap (len -1, 4294967295 printed).
        i.local_get(h).i64_const(0x7FFF_0000).i64_gt_s().if_(BlockType::Empty);
        i.i32_const(oom as i32).call(F_EPRINTLN_BLOCK);
        i.i32_const(1).call(F_EXIT_IMPORT).unreachable();
        i.end();
        i.local_get(h).i32_wrap_i64().call(F_ALLOC);
        let _ = i;
        self.release_i64();
        Ok(Some(BYTES))
    }

    fn lower_bytes_get(&mut self, b: &IrExpr, idx: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        self.lower(idx, Some(INT))?;
        let ih = self.hold_i64()?;
        let hr = self.hold_i32()?;
        self.f.instructions().local_set(ih);
        self.bytes_room(bh, ih, 1);
        let mut i = self.f.instructions();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(8).call(F_ALLOC).local_tee(hr);
        i.local_get(bh).local_get(ih).i32_wrap_i64().i32_add();
        i.i64_load8_u(byte_k(0));
        i.i64_store(slot_memarg(almide_layout::OPTION_FIELD));
        i.local_get(hr);
        i.else_();
        i.i32_const(0);
        i.end();
        let _ = i;
        self.release_i32();
        self.release_i64();
        self.release_i32();
        Ok(Some(SliceTy::Option(self.types.intern(INT))))
    }

    fn lower_bytes_set_arm(&mut self, b: &IrExpr, idx: &IrExpr, v: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        self.f.instructions().call(F_BLOCK_COPY);
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        self.lower(idx, Some(INT))?;
        let ih = self.hold_i64()?;
        self.f.instructions().local_set(ih);
        self.lower(v, Some(INT))?;
        let hv = self.hold_i64()?;
        self.f.instructions().local_set(hv);
        self.bytes_room(bh, ih, 1);
        let mut i = self.f.instructions();
        i.if_(BlockType::Empty);
        i.local_get(bh).local_get(ih).i32_wrap_i64().i32_add();
        i.local_get(hv).i64_store8(byte_k(0));
        i.end();
        i.local_get(bh);
        let _ = i;
        self.release_i64();
        self.release_i64();
        self.release_i32();
        Ok(Some(BYTES))
    }

    fn lower_bytes_slice(&mut self, b: &IrExpr, s: &IrExpr, e: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(s, Some(INT))?;
        let hs = self.hold_i64()?;
        self.f.instructions().local_set(hs);
        self.lower(e, Some(INT))?;
        let he = self.hold_i64()?;
        let ho = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(he);
        for h in [hs, he] {
            i.local_get(h);
            i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u();
            i.local_get(h)
                .local_get(hb)
                .i32_load(len_memarg())
                .i64_extend_i32_u()
                .i64_lt_u();
            i.select().local_set(h);
        }
        i.local_get(hs).local_get(he).i64_ge_u().if_(BlockType::Result(ValType::I32));
        i.i32_const(0).call(F_ALLOC);
        i.else_();
        i.local_get(he).local_get(hs).i64_sub().i32_wrap_i64().call(F_ALLOC).local_set(ho);
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hs)
            .i32_wrap_i64()
            .i32_add();
        i.local_get(he).local_get(hs).i64_sub().i32_wrap_i64();
        i.memory_copy(0, 0);
        i.local_get(ho);
        i.end();
        let _ = i;
        self.release_i32();
        self.release_i64();
        self.release_i64();
        self.release_i32();
        Ok(Some(BYTES))
    }

    fn lower_bytes_fill(&mut self, b: &IrExpr, v: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let IrExprKind::Var { id } = &b.kind else {
            return unsup("bytes-fill-nonvar");
        };
        let Some((var_idx, var_ty, vglob)) = self.mut_var(id) else {
            return unsup("var:unmapped");
        };
        self.lower(b, Some(BYTES))?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(v, Some(INT))?;
        let hv = self.hold_i64()?;
        let ho = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hv);
        i.local_get(hb).i32_load(len_memarg()).call(F_ALLOC).local_set(ho);
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hv).i32_wrap_i64();
        i.local_get(hb).i32_load(len_memarg());
        i.memory_fill(0);
        i.local_get(ho);
        let _ = i;
        self.emit_store_mut_var(*id, var_idx, var_ty, vglob)?;
        self.release_i32();
        self.release_i64();
        self.release_i32();
        Ok(None)
    }

    fn lower_bytes_concat(&mut self, a: &IrExpr, b: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(a, Some(BYTES))?;
        self.lower(b, Some(BYTES))?;
        self.f.instructions().call(F_CONCAT);
        Ok(Some(BYTES))
    }

    fn lower_bytes_to_string(&mut self, b: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let inv_pre = self.pool.intern("invalid UTF-8: invalid utf-8 sequence of ");
        let inv_mid = self.pool.intern(" bytes from index ");
        let inc_pre = self.pool.intern("invalid UTF-8: incomplete utf-8 byte sequence from index ");
        let h = self.work.helper(Helper::BytesToString { inv_pre, inv_mid, inc_pre });
        self.lower(b, Some(BYTES))?;
        self.f.instructions().call(h);
        Ok(Some(SliceTy::Result(self.types.intern(STR), self.types.intern(STR))))
    }

    fn lower_bytes_to_list(&mut self, b: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        let bh = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(bh);
        i.local_get(bh).i32_load(len_memarg()).i32_const(8).i32_mul();
        i.call(F_ALLOC).local_set(ho);
        i.i32_const(0).local_set(hc);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hc).local_get(bh).i32_load(len_memarg()).i32_ge_u().br_if(1);
        i.local_get(ho).local_get(hc).i32_const(8).i32_mul().i32_add();
        i.local_get(bh).local_get(hc).i32_add().i64_load8_u(byte_k(0));
        i.i64_store(slot_memarg(0));
        i.local_get(hc).i32_const(1).i32_add().local_set(hc);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(self.types.intern(INT))))
    }

    fn lower_bytes_repeat(&mut self, b: &IrExpr, n: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        self.lower(n, Some(INT))?;
        let hn = self.hold_i64()?;
        let ho = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let oom = self.pool.intern("Error: out of memory");
        let mut i = self.f.instructions();
        i.local_set(hn);
        // n = max(n, 0)  (select: v1 first)
        i.local_get(hn).i64_const(0);
        i.local_get(hn).i64_const(0).i64_gt_s();
        i.select().local_set(hn);
        // total = len * n, judged in i64 BEFORE the i32 wrap —
        // past the structural bound is the C-197 die.
        i.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
        i.local_get(hn).i64_mul();
        i.i64_const(0x7FFF_0000).i64_gt_s().if_(BlockType::Empty);
        i.i32_const(oom as i32).call(F_EPRINTLN_BLOCK);
        i.i32_const(1).call(F_EXIT_IMPORT).unreachable();
        i.end();
        i.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
        i.local_get(hn).i64_mul().i32_wrap_i64();
        i.call(F_ALLOC).local_set(ho);
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hw);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hn).i64_eqz().br_if(1);
        i.local_get(hw);
        i.local_get(bh).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(bh).i32_load(len_memarg());
        i.memory_copy(0, 0);
        i.local_get(hw).local_get(bh).i32_load(len_memarg()).i32_add().local_set(hw);
        i.local_get(hn).i64_const(1).i64_sub().local_set(hn);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        self.release_i32();
        self.release_i32();
        self.release_i64();
        self.release_i32();
        Ok(Some(BYTES))
    }

    fn lower_bytes_lossy(&mut self, b: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        let lossy = self.work.helper(Helper::Utf8Lossy);
        self.f.instructions().call(lossy);
        Ok(Some(STR))
    }

    fn lower_bytes_from_string(&mut self, s: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        self.f.instructions().call(F_BLOCK_COPY);
        Ok(Some(BYTES))
    }

    fn lower_bytes_len(&mut self, b: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        self.f.instructions().i32_load(len_memarg()).i64_extend_i32_u();
        Ok(Some(INT))
    }

    fn lower_bytes_from_list(&mut self, xs: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        match self.lower(xs, None)? {
            SliceTy::List(h) if self.types.el(h) == INT => {}
            other => return unsup(&format!("bytes-from-of:{other:?}")),
        }
        let bh = self.hold_i32()?;
        let ch = self.hold_i32()?;
        let ih = self.hold_i32()?;
        let rh = self.hold_i32()?;
        self.f.instructions().local_tee(bh);
        self.f
            .instructions()
            .i32_load(len_memarg())
            .i32_const(8)
            .i32_div_u()
            .local_tee(ch)
            .call(F_ALLOC)
            .local_set(rh)
            .i32_const(0)
            .local_set(ih);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.f.instructions().local_get(ih).local_get(ch).i32_ge_u().br_if(1);
        self.f.instructions().local_get(rh).local_get(ih).i32_add();
        self.f
            .instructions()
            .local_get(bh)
            .local_get(ih)
            .i32_const(8)
            .i32_mul()
            .i32_add()
            .i64_load(slot_memarg(0))
            .i32_wrap_i64()
            .i32_store8(byte_at());
        self.f
            .instructions()
            .local_get(ih)
            .i32_const(1)
            .i32_add()
            .local_set(ih)
            .br(0)
            .end()
            .end();
        self.f.instructions().local_get(rh);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(BYTES))
    }

    fn lower_bytes_get_or(&mut self, b: &IrExpr, i: &IrExpr, d: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        self.lower(i, Some(INT))?;
        let ih = self.hold_i64()?;
        self.f.instructions().local_set(ih);
        // the default ALWAYS evaluates (native argument order) —
        // in-branch lowering would skip its effects in-bounds
        self.lower(d, Some(INT))?;
        let hd = self.hold_i64()?;
        self.f.instructions().local_set(hd);
        self.bytes_room(bh, ih, 1);
        let mut ins = self.f.instructions();
        ins.if_(BlockType::Result(ValType::I64));
        ins.local_get(bh)
            .local_get(ih)
            .i32_wrap_i64()
            .i32_add()
            .i32_load8_u(byte_at())
            .i64_extend_i32_u();
        ins.else_().local_get(hd).end();
        let _ = ins;
        self.release_i64();
        self.release_i64();
        self.release_i32();
        Ok(Some(INT))
    }

    /// One-arg names.
    fn lower_bytes_unary(&mut self, func: &str, x: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        match func {
            "to_string" => self.lower_bytes_to_string(x),
            "to_list" => self.lower_bytes_to_list(x),
            "to_string_lossy" => self.lower_bytes_lossy(x),
            "len" => self.lower_bytes_len(x),
            "from_string" => self.lower_bytes_from_string(x),
            "from_list" => self.lower_bytes_from_list(x),
            _ => self.lower_bytes_new(x),
        }
    }

    /// Two-arg names.
    fn lower_bytes_pair(&mut self, func: &str, a: &IrExpr, b: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        match func {
            "concat" => self.lower_bytes_concat(a, b),
            "chunks" => self.lower_bytes_chunks(a, b),
            "repeat" => self.lower_bytes_repeat(a, b),
            _ => self.lower_bytes_get(a, b),
        }
    }

    /// Three-arg names.
    fn lower_bytes_triple(&mut self, func: &str, a: &IrExpr, b: &IrExpr, c: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        match func {
            "set" => self.lower_bytes_set_arm(a, b, c),
            "slice" => self.lower_bytes_slice(a, b, c),
            _ => self.lower_bytes_get_or(a, b, c),
        }
    }

    pub(crate) fn lower_bytes_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        if let Some(out) = self.lower_bytes_rw(func, args)? {
            return Ok(out);
        }
        match (func, args) {
            // Shape-grouped dispatch (pattern-count complexity; the
            // name split lives in the per-shape sub-dispatchers).
            ("to_string" | "to_list" | "to_string_lossy" | "len" | "from_string"
            | "from_list" | "new", [x]) => self.lower_bytes_unary(func, x),
            ("concat" | "chunks" | "repeat" | "get", [a, b]) => {
                self.lower_bytes_pair(func, a, b)
            }
            ("set" | "slice" | "get_or", [a, b, c]) => self.lower_bytes_triple(func, a, b, c),
            ("read_length_prefixed_strings_le", [b, p, c]) => {
                self.lower_bytes_lenprefix(b, p, c)
            }
            ("fill", [b, v]) => self.lower_bytes_fill(b, v),
            // some(byte) / none (native b.get — usize-wrap: negative i
            // is huge and misses). Its default is NONE, not 0, so it
            // takes its own guard instead of the bits path.
            // Functional set: a fresh copy, one in-range byte replaced
            // (native clone + guarded store).
            // MUT push (native b.push): copy-grow 1, store, write back.
            ("push", [b, v]) | ("append_u8", [b, v]) => self.lower_bytes_push(b, v),
            // pad to target with `val` on the chosen side; target <= len
            // (negative INCLUDED — the signed read, both legs) is a copy.
            ("pad_left" | "pad_right", [b, target, v]) => {
                let left = func == "pad_left";
                self.lower(b, Some(BYTES))?;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(target, Some(INT))?;
                let ht = self.hold_i64()?;
                self.f.instructions().local_set(ht);
                self.lower(v, Some(INT))?;
                let hv = self.hold_i64()?;
                let ho = self.hold_i32()?;
                let hp = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hv);
                i.local_get(ht);
                i.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
                i.i64_le_s().if_(BlockType::Result(ValType::I32));
                i.local_get(bh).call(F_BLOCK_COPY);
                i.else_();
                i.local_get(ht).i32_wrap_i64().call(F_ALLOC).local_set(ho);
                // pad = target - len bytes of val
                i.local_get(ht)
                    .i32_wrap_i64()
                    .local_get(bh)
                    .i32_load(len_memarg())
                    .i32_sub()
                    .local_set(hp);
                // fill zone start: left → payload; right → payload + len
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                if !left {
                    i.local_get(bh).i32_load(len_memarg()).i32_add();
                }
                i.local_get(hv).i32_wrap_i64();
                i.local_get(hp);
                i.memory_fill(0);
                // the source bytes: left → after the pad; right → front
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                if left {
                    i.local_get(hp).i32_add();
                }
                i.local_get(bh).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(bh).i32_load(len_memarg());
                i.memory_copy(0, 0);
                i.local_get(ho);
                i.end();
                let _ = i;
                self.release_i32();
                self.release_i32();
                self.release_i64();
                self.release_i64();
                self.release_i32();
                Ok(Some(BYTES))
            }
            // MUT window copy (native copy_from): either offset past its
            // buffer is a no-op; len clamps to both remainders.
            ("copy_from", [dst, src, doff, soff, n]) => {
                let IrExprKind::Var { id } = &dst.kind else {
                    return unsup("bytes-copy-from-nonvar");
                };
                let Some((var_idx, var_ty, vglob)) = self.mut_var(id) else {
                    return unsup("var:unmapped");
                };
                self.emit_read_mut_var_cow(id, var_idx, var_ty, vglob)?;
                self.f.instructions().call(F_BLOCK_COPY);
                let dh = self.hold_i32()?;
                self.f.instructions().local_set(dh);
                self.lower(src, Some(BYTES))?;
                let sh = self.hold_i32()?;
                self.f.instructions().local_set(sh);
                self.lower(doff, Some(INT))?;
                let hdo = self.hold_i64()?;
                self.f.instructions().local_set(hdo);
                self.lower(soff, Some(INT))?;
                let hso = self.hold_i64()?;
                self.f.instructions().local_set(hso);
                self.lower(n, Some(INT))?;
                let hn = self.hold_i64()?;
                let hl = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hn);
                // in-range offsets? (usize-wrap: negative = huge = miss)
                i.local_get(hdo).i64_const(0).i64_ge_s();
                i.local_get(hdo);
                i.local_get(dh).i32_load(len_memarg()).i64_extend_i32_u();
                i.i64_lt_s().i32_and();
                i.local_get(hso).i64_const(0).i64_ge_s().i32_and();
                i.local_get(hso);
                i.local_get(sh).i32_load(len_memarg()).i64_extend_i32_u();
                i.i64_lt_s().i32_and();
                i.local_get(hn).i64_const(0).i64_ge_s().i32_and();
                i.if_(BlockType::Empty);
                // len = min(n, dst_rem, src_rem) — select(v1,v2,cond) =
                // cond ? v1 : v2, so the REMAINDER is v1 under n > rem.
                i.local_get(dh)
                    .i32_load(len_memarg())
                    .local_get(hdo)
                    .i32_wrap_i64()
                    .i32_sub();
                i.local_get(hn).i32_wrap_i64();
                i.local_get(hn).i32_wrap_i64();
                i.local_get(dh)
                    .i32_load(len_memarg())
                    .local_get(hdo)
                    .i32_wrap_i64()
                    .i32_sub();
                i.i32_gt_u().select();
                i.local_set(hl);
                i.local_get(sh)
                    .i32_load(len_memarg())
                    .local_get(hso)
                    .i32_wrap_i64()
                    .i32_sub();
                i.local_get(hl);
                i.local_get(hl);
                i.local_get(sh)
                    .i32_load(len_memarg())
                    .local_get(hso)
                    .i32_wrap_i64()
                    .i32_sub();
                i.i32_gt_u().select();
                i.local_set(hl);
                i.local_get(dh)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hdo)
                    .i32_wrap_i64()
                    .i32_add();
                i.local_get(sh)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hso)
                    .i32_wrap_i64()
                    .i32_add();
                i.local_get(hl);
                i.memory_copy(0, 0);
                i.end();
                i.local_get(dh);
                let _ = i;
                self.emit_store_mut_var(*id, var_idx, var_ty, vglob)?;
                self.release_i32();
                self.release_i64();
                self.release_i64();
                self.release_i64();
                self.release_i32();
                self.release_i32();
                Ok(None)
            }
            // slice: native usize-min clamps — a NEGATIVE bound casts
            // huge and saturates to len (s >= e is the empty buffer).
            // The BE serialization cursor (#1099 intrinsics): append k
            // big-endian bytes of the value, mut on the native surface —
            // the push convention (var write-back, no value).
            (
                "write_u8" | "write_u16_be" | "write_u32_be" | "write_i64_be" | "write_f64_be",
                [b, v],
            ) => {
                let IrExprKind::Var { id } = &b.kind else {
                    return unsup("bytes-write-nonvar");
                };
                let Some((var_idx, var_ty, vglob)) = self.mut_var(id) else {
                    return unsup("var:unmapped");
                };
                let (k, float) = match func {
                    "write_u8" => (1, false),
                    "write_u16_be" => (2, false),
                    "write_u32_be" => (4, false),
                    "write_i64_be" => (8, false),
                    _ => (8, true),
                };
                self.lower_bytes_write_be(b, v, k, float)?;
                self.emit_store_mut_var(*id, var_idx, var_ty, vglob)?;
                Ok(None)
            }
            // copy_within: memmove inside the buffer, NO-OP when the
            // range is empty or the destination does not fit (native
            // guard verbatim — the adds wrap in i64 exactly like the
            // release-mode usize arithmetic they mirror).
            ("copy_within", [b, s, e, d]) => {
                let IrExprKind::Var { id } = &b.kind else {
                    return unsup("bytes-copy-within-nonvar");
                };
                let Some((var_idx, var_ty, vglob)) = self.mut_var(id) else {
                    return unsup("var:unmapped");
                };
                self.lower(b, Some(BYTES))?;
                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                self.lower(s, Some(INT))?;
                let hs = self.hold_i64()?;
                self.f.instructions().local_set(hs);
                self.lower(e, Some(INT))?;
                let he = self.hold_i64()?;
                self.f.instructions().local_set(he);
                self.lower(d, Some(INT))?;
                let hd = self.hold_i64()?;
                let ho = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hd);
                i.local_get(hb).i32_load(len_memarg()).call(F_ALLOC).local_set(ho);
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hb).i32_load(len_memarg());
                i.memory_copy(0, 0);
                // e = min_u(e, len)
                i.local_get(he);
                i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u();
                i.local_get(he)
                    .local_get(hb)
                    .i32_load(len_memarg())
                    .i64_extend_i32_u()
                    .i64_lt_u();
                i.select().local_set(he);
                // s < e  &&  d + (e - s) <= len
                i.local_get(hs).local_get(he).i64_lt_u();
                i.local_get(hd).local_get(he).i64_add().local_get(hs).i64_sub();
                i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u();
                i.i64_le_u();
                i.i32_and().if_(BlockType::Empty);
                i.local_get(ho)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hd)
                    .i32_wrap_i64()
                    .i32_add();
                i.local_get(ho)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hs)
                    .i32_wrap_i64()
                    .i32_add();
                i.local_get(he).local_get(hs).i64_sub().i32_wrap_i64();
                i.memory_copy(0, 0);
                i.end();
                i.local_get(ho);
                let _ = i;
                self.emit_store_mut_var(*id, var_idx, var_ty, vglob)?;
                self.release_i32();
                self.release_i64();
                self.release_i64();
                self.release_i64();
                self.release_i32();
                Ok(None)
            }
            // fill: every byte becomes `val as u8` — mut on the native
            // surface (same convention).
            // Concatenation: blocks share the string layout (len = bytes),
            // so the string concat helper applies verbatim.
            // chunks: size <= 0 is the empty list; the last chunk may be
            // short. size clamps through i64 BEFORE any i32 narrowing so
            // a huge size is ONE chunk, never a wrapped count.
            // to_string: std::str::from_utf8 verbatim — ok shares the
            // block; err carries the Utf8Error Display line.
            // One i64 slot per byte (native to_list).
            // n copies (native n.max(0); the C-197 structural bound dies
            // as OOM — no chosen ceiling, ratified A 2026-08-17).
            // Native from_utf8_lossy (the WHATWG helper) — the self-host
            // impl is a raw copy and must not shadow this.
            // The linked append/write family is FUNCTIONAL in the
            // self-host but MUT on the native surface — a statement call
            // on a var writes the fresh result back (the list.push
            // convention).
            (f, [b, ..]) if f.starts_with("append_") || f.starts_with("write_") => {
                let IrExprKind::Var { id } = &b.kind else {
                    return unsup("bytes-append-nonvar");
                };
                let Some((var_idx, var_ty, vglob)) = self.mut_var(id) else {
                    return unsup("var:unmapped");
                };
                match self.lower_linked_call("bytes", func, args, false)? {
                    Some(SliceTy::Scalar(Scalar::Bytes)) => {}
                    other => return unsup(&format!("bytes-append-ret:{other:?}")),
                }
                self.emit_store_mut_var(*id, var_idx, var_ty, vglob)?;
                Ok(None)
            }
            // Not a native arm: the audited linked path before the wall.
            _ => self.lower_linked_call("bytes", func, args, false),
        }
    }
}

impl Emitter<'_> {
    /// MUT push (native b.push): the `$bytes_push` helper — cap fast
    /// path, geometric growth, outgrown block freed at rc==1 (#1689) —
    /// then write back, exactly the `list.push` convention.
    fn lower_bytes_push(&mut self, b: &IrExpr, v: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
                let IrExprKind::Var { id } = &b.kind else {
                    return unsup("bytes-push-nonvar");
                };
                let Some((var_idx, var_ty, vglob)) = self.mut_var(id) else {
                    return unsup("var:unmapped");
                };
                self.emit_read_mut_var_cow(id, var_idx, var_ty, vglob)?;
                self.lower(v, Some(INT))?;
                self.f.instructions().call(F_BYTES_PUSH);
                self.emit_store_mut_var(*id, var_idx, var_ty, vglob)?;
                Ok(None)
    }
}
