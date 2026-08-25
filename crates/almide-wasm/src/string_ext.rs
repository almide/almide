//! String surface extensions (push/take/drop/get/replace/join and the
//! prefix/emptiness tests) — split from the module-call chain for the
//! complexity budget; consulted FIRST for module calls (Ok(None) falls
//! through to the chain). The byte scanners (pad/lines/chars/codepoint)
//! live in string_scan.rs.

use almide_ir::{CallTarget, IrExpr, IrExprKind};
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::string_scan::str_byte;
use crate::*;

impl Emitter<'_> {
    pub(crate) fn lower_string_ext(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let CallTarget::Module { module, func, .. } = target else {
            return Ok(None);
        };
        if module.as_str() != "string" {
            return Ok(None);
        }
        match (func.as_str(), args) {
            // mut append (native s.push_str): var write-back of concat.
            ("push", [v, x]) => self.lower_string_push(v, x),
            ("from_bytes", [xs]) => self.lower_string_from_bytes(xs),
            ("is_empty", [s]) => {
                self.lower(s, Some(STR))?;
                self.f.instructions().i32_load(len_memarg()).i32_eqz();
                Ok(Some(BOOL))
            }
            ("pad_start" | "pad_end", [s, w, p]) => {
                self.lower_string_pad(s, w, p, func.as_str() == "pad_start")
            }
            ("lines", [s]) => self.lower_string_lines(s),
            ("chars", [s]) => self.lower_string_chars(s),
            ("capitalize", [s]) => self.lower_string_capitalize(s),
            ("run_length_encode", [s]) => self.lower_string_rle(s),
            ("codepoint", [s]) => self.lower_string_codepoint(s),
            ("get", [s, i]) => self.lower_string_get(s, i),
            ("drop", [s, n]) => self.lower_string_drop(s, n),
            ("take", [s, n]) => self.lower_string_take(s, n),
            ("starts_with", [s, p]) => self.lower_string_starts_with(s, p),
            // ends_with = the strip_suffix compare with a Bool verdict.
            ("ends_with", [s, p]) => {
                let got = self.lower_string_strip(s, p, false)?;
                let _ = got;
                self.f.instructions().i32_const(0).i32_ne();
                Ok(Some(BOOL))
            }
            ("strip_prefix" | "strip_suffix", [s, p]) => {
                self.lower_string_strip(s, p, func.as_str() == "strip_prefix")
            }
            // Rust str::replace / replace_first byte-for-byte via the
            // shared helper (the `first` flag selects the form). The
            // empty-pattern char-boundary rule (C-100) lives in the helper.
            ("replace" | "replace_first", [s, from, to]) => {
                let first = func.as_str() == "replace_first";
                self.lower(s, Some(STR))?;
                self.lower(from, Some(STR))?;
                self.lower(to, Some(STR))?;
                self.f.instructions().i32_const(i32::from(first)).call(F_STR_REPLACE);
                Ok(Some(STR))
            }
            // string.join(xs, sep) is list.join with the module spelled
            // the other way — same F_LIST_JOIN, same List[String] demand.
            ("join", [xs, sep]) => {
                match self.lower(xs, None)? {
                    SliceTy::List(h) if self.types.el(h) == STR => {}
                    other => return unsup(&format!("string-join-of:{other:?}")),
                }
                self.lower(sep, Some(STR))?;
                self.f.instructions().call(F_LIST_JOIN);
                Ok(Some(STR))
            }
            _ => return Ok(None),
        }
        .map(Some)
    }

    /// First n CHARS (native `s.chars().take(n as usize)`): a NEGATIVE n
    /// reinterprets huge and takes the WHOLE string — deliberately not
    /// the C-054 clamp; cp_off clamps past-end.
    fn lower_string_take(&mut self, s: &IrExpr, n: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        self.f.instructions().local_set(hs);
        self.lower(n, Some(INT))?;
        let hn = self.hold_i64()?;
        let hoff = self.hold_i32()?;
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hn);
        i.local_get(hn).i64_const(0).i64_lt_s();
        i.if_(BlockType::Result(ValType::I32));
        i.local_get(hs).i32_load(len_memarg());
        i.else_();
        i.local_get(hs).local_get(hn).call(F_CP_OFF);
        i.end();
        i.local_set(hoff);
        i.local_get(hoff).call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hoff);
        i.memory_copy(0, 0);
        i.local_get(hb);
        let _ = i;
        self.release_i32();
        self.release_i32();
        self.release_i64();
        self.release_i32();
        Ok(Some(STR))
    }

    /// Skip n CHARS (native `s.chars().skip(n as usize)`): a NEGATIVE n
    /// reinterprets huge and skips EVERYTHING — the deliberate
    /// mirror-asymmetry of take (whole vs empty).
    fn lower_string_drop(&mut self, s: &IrExpr, n: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        self.f.instructions().local_set(hs);
        self.lower(n, Some(INT))?;
        let hn = self.hold_i64()?;
        let hoff = self.hold_i32()?;
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hn);
        i.local_get(hn).i64_const(0).i64_lt_s();
        i.if_(BlockType::Result(ValType::I32));
        i.local_get(hs).i32_load(len_memarg());
        i.else_();
        i.local_get(hs).local_get(hn).call(F_CP_OFF);
        i.end();
        i.local_set(hoff);
        i.local_get(hs).i32_load(len_memarg()).local_get(hoff).i32_sub();
        i.call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hoff)
            .i32_add();
        i.local_get(hs).i32_load(len_memarg()).local_get(hoff).i32_sub();
        i.memory_copy(0, 0);
        i.local_get(hb);
        let _ = i;
        self.release_i32();
        self.release_i32();
        self.release_i64();
        self.release_i32();
        Ok(Some(STR))
    }

    /// Char i as a one-char string (native char_at): negative or
    /// past-end → none; cp_off clamps, so past-end IS off == len.
    fn lower_string_get(&mut self, s: &IrExpr, idx: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        self.f.instructions().local_set(hs);
        self.lower(idx, Some(INT))?;
        let hn = self.hold_i64()?;
        let hoff = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hn);
        i.local_get(hn).i64_const(0).i64_lt_s();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(0);
        i.else_();
        i.local_get(hs).local_get(hn).call(F_CP_OFF).local_set(hoff);
        i.local_get(hoff).local_get(hs).i32_load(len_memarg()).i32_ge_u();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(0);
        i.else_();
        i.local_get(hs).local_get(hoff).i32_add().i32_load8_u(str_byte()).local_set(hw);
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
            .local_get(hoff)
            .i32_add();
        i.local_get(hw);
        i.memory_copy(0, 0);
        // some(str): a 4-byte option cell holding the handle
        i.i32_const(4).call(F_ALLOC).local_tee(hoff).local_get(hb);
        i.i32_store(slot_memarg(almide_layout::OPTION_FIELD));
        i.local_get(hoff);
        i.end();
        i.end();
        let _ = i;
        for _ in 0..3 {
            self.release_i32();
        }
        self.release_i64();
        self.release_i32();
        Ok(Some(SliceTy::Option(self.types.intern(STR))))
    }

    /// mut append (native s.push_str): var write-back of concat.
    fn lower_string_push(&mut self, v: &IrExpr, x: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let IrExprKind::Var { id } = &v.kind else {
            return unsup("string-push-nonvar");
        };
        let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
            return unsup("var:unmapped");
        };
        if var_ty != STR {
            return unsup(&format!("string-push-of:{var_ty:?}"));
        }
        self.f.instructions().local_get(var_idx);
        self.lower(x, Some(STR))?;
        self.f.instructions().call(F_CONCAT).local_set(var_idx);
        Ok(None)
    }

    /// strip_prefix/strip_suffix (native Option-returning): the byte
    /// affix matches → some(rest), else none.
    fn lower_string_strip(
        &mut self,
        s: &IrExpr,
        p: &IrExpr,
        prefix: bool,
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        self.f.instructions().local_set(hs);
        self.lower(p, Some(STR))?;
        let hp = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hr = self.hold_i32()?;
        let hoff = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hp);
        i.local_get(hp).i32_load(len_memarg()).local_set(hn);
        // the affix's start within s: 0 (prefix) or slen - plen (suffix)
        if prefix {
            i.i32_const(0).local_set(hoff);
        } else {
            i.local_get(hs).i32_load(len_memarg()).local_get(hn).i32_sub().local_set(hoff);
        }
        i.local_get(hn).local_get(hs).i32_load(len_memarg()).i32_gt_u();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(0);
        i.else_();
        i.i32_const(1).local_set(hr);
        i.i32_const(0).local_set(hk);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hk).local_get(hn).i32_ge_u().br_if(1);
        i.local_get(hs).local_get(hoff).i32_add().local_get(hk).i32_add();
        i.i32_load8_u(str_byte());
        i.local_get(hp).local_get(hk).i32_add().i32_load8_u(str_byte());
        i.i32_ne().if_(BlockType::Empty);
        i.i32_const(0).local_set(hr);
        i.br(2);
        i.end();
        i.local_get(hk).i32_const(1).i32_add().local_set(hk);
        i.br(0).end().end();
        i.local_get(hr).if_(BlockType::Result(ValType::I32));
        // some(rest): the bytes OUTSIDE the affix
        i.local_get(hs).i32_load(len_memarg()).local_get(hn).i32_sub();
        i.call(F_ALLOC).local_set(hk);
        i.local_get(hk).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hs).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        if prefix {
            i.local_get(hn).i32_add();
        }
        i.local_get(hs).i32_load(len_memarg()).local_get(hn).i32_sub();
        i.memory_copy(0, 0);
        i.i32_const(4).call(F_ALLOC).local_tee(hr).local_get(hk);
        i.i32_store(slot_memarg(almide_layout::OPTION_FIELD));
        i.local_get(hr);
        i.else_();
        i.i32_const(0);
        i.end();
        i.end();
        let _ = i;
        for _ in 0..6 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Option(self.types.intern(STR))))
    }

    /// Byte-prefix compare (native str::starts_with): for valid UTF-8
    /// the byte test IS the char test.
    fn lower_string_starts_with(
        &mut self,
        s: &IrExpr,
        prefix: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower(s, Some(STR))?;
        let hs = self.hold_i32()?;
        self.f.instructions().local_set(hs);
        self.lower(prefix, Some(STR))?;
        let hp = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hr = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hp);
        i.local_get(hp).i32_load(len_memarg()).local_set(hn);
        i.local_get(hn).local_get(hs).i32_load(len_memarg()).i32_gt_u();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(0);
        i.else_();
        i.i32_const(1).local_set(hr);
        i.i32_const(0).local_set(hk);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hk).local_get(hn).i32_ge_u().br_if(1);
        i.local_get(hs).local_get(hk).i32_add().i32_load8_u(str_byte());
        i.local_get(hp).local_get(hk).i32_add().i32_load8_u(str_byte());
        i.i32_ne().if_(BlockType::Empty);
        i.i32_const(0).local_set(hr);
        i.br(2);
        i.end();
        i.local_get(hk).i32_const(1).i32_add().local_set(hk);
        i.br(0).end().end();
        i.local_get(hr);
        i.end();
        let _ = i;
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(BOOL))
    }

    /// from_bytes = from_list ∘ the NATIVE WHATWG lossy helper
    /// (String::from_utf8_lossy verbatim). The self-host
    /// string_from_bytes reads the list len header raw and the
    /// self-host bytes_to_string_lossy is a RAW COPY (not lossy) —
    /// both unlinkable; the helper is the one true decoder.
    fn lower_string_from_bytes(&mut self, xs: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        match self.lower_bytes_call("from_list", std::slice::from_ref(xs))? {
            Some(SliceTy::Scalar(Scalar::Bytes)) => {}
            other => return unsup(&format!("from-bytes-of:{other:?}")),
        }
        let lossy = self.work.helper(Helper::Utf8Lossy);
        self.f.instructions().call(lossy);
        Ok(Some(STR))
    }
}
