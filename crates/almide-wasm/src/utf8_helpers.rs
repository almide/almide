//! UTF-8 byte-walk helpers — the WHATWG Table 3-7 classification
//! shared by from_utf8 validation (bytes.to_string) and the lossy
//! replacement walk (to_string_lossy / string.from_bytes). Split
//! from value_helpers.rs for the file budget.

use wasm_encoder::{BlockType, Function, MemArg, ValType};
use crate::*;

/// `$utf8_lossy(bytes) -> str` — String::from_utf8_lossy VERBATIM:
/// the Table 3-7 well-formed ranges (C2..DF +1; E0 A0..BF; E1..EC /
/// EE..EF 80..BF; ED 80..9F; F0 90..BF; F1..F3 80..BF; F4 80..8F, then
/// 80..BF continuations), one U+FFFD (EF BF BD) per MAXIMAL invalid
/// subpart: an invalid sequence consumes its lead plus every VALID
/// continuation it managed, and the failing byte re-examines. The out
/// block over-allocates 3n and patches its len to the written bytes.
/// `$bytes_to_string(b) -> i32` — std::str::from_utf8 verbatim (the same
/// Table 3-7 classification as the lossy walker): ok shares the block,
/// err carries the Utf8Error Display line ("invalid UTF-8: " prefixed).
/// error_len = lead + valid continuations (the maximal invalid subpart);
/// running off the end mid-sequence is the "incomplete" Display form.
pub(crate) fn emit_bytes_to_string_helper(inv_pre: u32, inv_mid: u32, inc_pre: u32) -> Function {
    let (b, n, k, b0, extra, lo, hi, vlen, j, c, r) =
        (0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32);
    let pay = almide_layout::PAYLOAD as i32;
    let byte = MemArg { offset: 0, align: 0, memory_index: 0 };
    let m_tag = slot_memarg(almide_layout::SUM_TAG);
    let m_pay = slot_memarg(almide_layout::SUM_FIELD);
    let mut f = Function::new([(10, ValType::I32)]);
    let mut i = f.instructions();
    // err epilogues are open-coded twice (invalid / incomplete); each
    // builds the message then RETURNS the err block. `$concat` BORROWS both
    // operands and returns a fresh block, so every `int.to_string` digit
    // block and every intermediate prefix is parked in a local and released
    // once the next concat has copied it (#3194: the four temporaries
    // outlived the program). The parking locals (b0/lo/hi/j) are dead past
    // an err epilogue — it returns.
    let emit_err_invalid = |i: &mut wasm_encoder::InstructionSink| {
        i.local_get(vlen).i64_extend_i32_u().call(F_INT_TO_STRING).local_set(lo);
        i.i32_const(inv_pre as i32).local_get(lo).call(F_CONCAT).local_set(hi);
        i.local_get(lo).call(F_DEC_FLAT);
        i.local_get(hi).i32_const(inv_mid as i32).call(F_CONCAT).local_set(b0);
        i.local_get(hi).call(F_DEC_FLAT);
        i.local_get(k).i64_extend_i32_u().call(F_INT_TO_STRING).local_set(j);
        i.local_get(b0).local_get(j).call(F_CONCAT).local_set(c);
        i.local_get(b0).call(F_DEC_FLAT);
        i.local_get(j).call(F_DEC_FLAT);
        i.i32_const(16).call(F_ALLOC).local_tee(r);
        i.i32_const(1).i32_store(m_tag);
        i.local_get(r).local_get(c).i32_store(m_pay);
        i.local_get(r).return_();
    };
    i.local_get(b).i32_load(len_memarg()).local_set(n);
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(n).i32_ge_u().br_if(1);
    i.local_get(b).i32_const(pay).i32_add().local_get(k).i32_add().i32_load8_u(byte);
    i.local_set(b0);
    i.local_get(b0).i32_const(0x80).i32_lt_u().if_(BlockType::Empty);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.else_();
    i.i32_const(0).local_set(extra);
    i.i32_const(0x80).local_set(lo);
    i.i32_const(0xBF).local_set(hi);
    i.local_get(b0).i32_const(0xC2).i32_ge_u();
    i.local_get(b0).i32_const(0xDF).i32_le_u();
    i.i32_and().if_(BlockType::Empty);
    i.i32_const(1).local_set(extra);
    i.end();
    i.local_get(b0).i32_const(0xE0).i32_eq().if_(BlockType::Empty);
    i.i32_const(2).local_set(extra);
    i.i32_const(0xA0).local_set(lo);
    i.end();
    i.local_get(b0).i32_const(0xE1).i32_ge_u();
    i.local_get(b0).i32_const(0xEC).i32_le_u();
    i.i32_and().if_(BlockType::Empty);
    i.i32_const(2).local_set(extra);
    i.end();
    i.local_get(b0).i32_const(0xED).i32_eq().if_(BlockType::Empty);
    i.i32_const(2).local_set(extra);
    i.i32_const(0x9F).local_set(hi);
    i.end();
    i.local_get(b0).i32_const(0xEE).i32_ge_u();
    i.local_get(b0).i32_const(0xEF).i32_le_u();
    i.i32_and().if_(BlockType::Empty);
    i.i32_const(2).local_set(extra);
    i.end();
    i.local_get(b0).i32_const(0xF0).i32_eq().if_(BlockType::Empty);
    i.i32_const(3).local_set(extra);
    i.i32_const(0x90).local_set(lo);
    i.end();
    i.local_get(b0).i32_const(0xF1).i32_ge_u();
    i.local_get(b0).i32_const(0xF3).i32_le_u();
    i.i32_and().if_(BlockType::Empty);
    i.i32_const(3).local_set(extra);
    i.end();
    i.local_get(b0).i32_const(0xF4).i32_eq().if_(BlockType::Empty);
    i.i32_const(3).local_set(extra);
    i.i32_const(0x8F).local_set(hi);
    i.end();
    i.i32_const(1).local_set(vlen);
    i.local_get(extra).i32_eqz().if_(BlockType::Empty);
    emit_err_invalid(&mut i);
    i.end();
    i.i32_const(1).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(extra).i32_gt_u().br_if(1);
    i.local_get(k).local_get(j).i32_add().local_get(n).i32_ge_u().if_(BlockType::Empty);
    // ran off the end mid-sequence: the "incomplete" Display form
    i.local_get(k).i64_extend_i32_u().call(F_INT_TO_STRING).local_set(lo);
    i.i32_const(inc_pre as i32).local_get(lo).call(F_CONCAT).local_set(c);
    i.local_get(lo).call(F_DEC_FLAT);
    i.i32_const(16).call(F_ALLOC).local_tee(r);
    i.i32_const(1).i32_store(m_tag);
    i.local_get(r).local_get(c).i32_store(m_pay);
    i.local_get(r).return_();
    i.end();
    i.local_get(b).i32_const(pay).i32_add().local_get(k).i32_add().local_get(j).i32_add();
    i.i32_load8_u(byte);
    i.local_set(c);
    i.local_get(c);
    i.local_get(lo).i32_const(0x80).local_get(j).i32_const(1).i32_eq().select();
    i.i32_lt_u();
    i.local_get(c);
    i.local_get(hi).i32_const(0xBF).local_get(j).i32_const(1).i32_eq().select();
    i.i32_gt_u();
    i.i32_or().if_(BlockType::Empty);
    emit_err_invalid(&mut i);
    i.end();
    i.local_get(vlen).i32_const(1).i32_add().local_set(vlen);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.local_get(k).local_get(extra).i32_add().i32_const(1).i32_add().local_set(k);
    i.end();
    i.br(0).end().end();
    // valid: ok(b) — the block is immutable, sharing is unobservable
    i.i32_const(16).call(F_ALLOC).local_tee(r);
    i.i32_const(0).i32_store(m_tag);
    // The Result HOLDS the block: +1 (its typed drop releases it, and the
    // caller's own release of the bytes is independent — #2010 stage 2c).
    i.local_get(b).call(F_INC);
    i.local_get(r).local_get(b).i32_store(m_pay);
    i.local_get(r);
    i.end();
    f
}

pub(crate) fn emit_utf8_lossy_helper() -> Function {
    let (b, n, out, k, w, b0, extra, lo, hi, vlen, ok, j, c) =
        (0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32);
    let pay = almide_layout::PAYLOAD as i32;
    let mut f = Function::new([(12, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(b).i32_load(len_memarg()).local_set(n);
    i.local_get(n).i32_const(3).i32_mul().call(F_ALLOC).local_set(out);
    i.local_get(out).i32_const(pay).i32_add().local_set(w);
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(n).i32_ge_u().br_if(1);
    i.local_get(b).i32_const(pay).i32_add().local_get(k).i32_add();
    i.i32_load8_u(MemArg { offset: 0, align: 0, memory_index: 0 });
    i.local_set(b0);
    i.local_get(b0).i32_const(0x80).i32_lt_u().if_(BlockType::Empty);
    // ASCII: one byte through
    i.local_get(w).local_get(b0).i32_store8(MemArg { offset: 0, align: 0, memory_index: 0 });
    i.local_get(w).i32_const(1).i32_add().local_set(w);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.else_();
    // classify the lead: extra = 0 marks invalid
    i.i32_const(0).local_set(extra);
    i.i32_const(0x80).local_set(lo);
    i.i32_const(0xBF).local_set(hi);
    i.local_get(b0).i32_const(0xC2).i32_ge_u();
    i.local_get(b0).i32_const(0xDF).i32_le_u();
    i.i32_and().if_(BlockType::Empty);
    i.i32_const(1).local_set(extra);
    i.end();
    i.local_get(b0).i32_const(0xE0).i32_eq().if_(BlockType::Empty);
    i.i32_const(2).local_set(extra);
    i.i32_const(0xA0).local_set(lo);
    i.end();
    i.local_get(b0).i32_const(0xE1).i32_ge_u();
    i.local_get(b0).i32_const(0xEC).i32_le_u();
    i.i32_and().if_(BlockType::Empty);
    i.i32_const(2).local_set(extra);
    i.end();
    i.local_get(b0).i32_const(0xED).i32_eq().if_(BlockType::Empty);
    i.i32_const(2).local_set(extra);
    i.i32_const(0x9F).local_set(hi);
    i.end();
    i.local_get(b0).i32_const(0xEE).i32_ge_u();
    i.local_get(b0).i32_const(0xEF).i32_le_u();
    i.i32_and().if_(BlockType::Empty);
    i.i32_const(2).local_set(extra);
    i.end();
    i.local_get(b0).i32_const(0xF0).i32_eq().if_(BlockType::Empty);
    i.i32_const(3).local_set(extra);
    i.i32_const(0x90).local_set(lo);
    i.end();
    i.local_get(b0).i32_const(0xF1).i32_ge_u();
    i.local_get(b0).i32_const(0xF3).i32_le_u();
    i.i32_and().if_(BlockType::Empty);
    i.i32_const(3).local_set(extra);
    i.end();
    i.local_get(b0).i32_const(0xF4).i32_eq().if_(BlockType::Empty);
    i.i32_const(3).local_set(extra);
    i.i32_const(0x8F).local_set(hi);
    i.end();
    i.local_get(extra).i32_eqz().if_(BlockType::Empty);
    // invalid lead: one FFFD, one byte consumed
    i.local_get(w).i32_const(0xEF).i32_store8(MemArg { offset: 0, align: 0, memory_index: 0 });
    i.local_get(w).i32_const(0xBF).i32_store8(MemArg { offset: 1, align: 0, memory_index: 0 });
    i.local_get(w).i32_const(0xBD).i32_store8(MemArg { offset: 2, align: 0, memory_index: 0 });
    i.local_get(w).i32_const(3).i32_add().local_set(w);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.else_();
    // walk the continuations: j = 1..=extra
    i.i32_const(1).local_set(vlen);
    i.i32_const(1).local_set(ok);
    i.i32_const(1).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(extra).i32_gt_u().br_if(1);
    i.local_get(k).local_get(j).i32_add().local_get(n).i32_ge_u().if_(BlockType::Empty);
    i.i32_const(0).local_set(ok);
    i.br(2);
    i.end();
    i.local_get(b).i32_const(pay).i32_add().local_get(k).i32_add().local_get(j).i32_add();
    i.i32_load8_u(MemArg { offset: 0, align: 0, memory_index: 0 });
    i.local_set(c);
    // range: j == 1 uses [lo, hi]; the rest 80..BF
    i.local_get(c);
    i.local_get(lo).i32_const(0x80).local_get(j).i32_const(1).i32_eq().select();
    i.i32_lt_u();
    i.local_get(c);
    i.local_get(hi).i32_const(0xBF).local_get(j).i32_const(1).i32_eq().select();
    i.i32_gt_u();
    i.i32_or().if_(BlockType::Empty);
    i.i32_const(0).local_set(ok);
    i.br(2);
    i.end();
    i.local_get(vlen).i32_const(1).i32_add().local_set(vlen);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.local_get(ok).if_(BlockType::Empty);
    // well-formed: copy the whole sequence
    i.local_get(w);
    i.local_get(b).i32_const(pay).i32_add().local_get(k).i32_add();
    i.local_get(extra).i32_const(1).i32_add();
    i.memory_copy(0, 0);
    i.local_get(w).local_get(extra).i32_add().i32_const(1).i32_add().local_set(w);
    i.local_get(k).local_get(extra).i32_add().i32_const(1).i32_add().local_set(k);
    i.else_();
    // maximal invalid subpart: FFFD, consume lead + valid continuations
    i.local_get(w).i32_const(0xEF).i32_store8(MemArg { offset: 0, align: 0, memory_index: 0 });
    i.local_get(w).i32_const(0xBF).i32_store8(MemArg { offset: 1, align: 0, memory_index: 0 });
    i.local_get(w).i32_const(0xBD).i32_store8(MemArg { offset: 2, align: 0, memory_index: 0 });
    i.local_get(w).i32_const(3).i32_add().local_set(w);
    i.local_get(k).local_get(vlen).i32_add().local_set(k);
    i.end();
    i.end();
    i.end();
    i.br(0).end().end();
    // len = written bytes
    i.local_get(out);
    i.local_get(w).local_get(out).i32_const(pay).i32_add().i32_sub();
    i.i32_store(len_memarg());
    i.local_get(out);
    i.end();
    f
}

/// `$err_text(str) -> str` — C-418's escaper (#4126), the wasm twin of
/// almide-base's err_text.rs for the error texts this emitter builds itself
/// (`value.field`'s key, `error.context`'s label): TAB, CR, LF and NUL read
/// `\t` `\r` `\n` `\0`; every other C0 control and DEL reads `\u{<hex>}`; the
/// multi-byte characters the rule escapes (C1 and the bidi controls) read
/// `\u{<hex>}` too — that set is the rule's own predicate run over the code
/// points at emit time, not a copy of it. Every other byte is copied. The
/// result is always a FRESH block (the caller releases it), over-allocated
/// 6n (the widest growth: one byte to `\u{1b}`) with its len patched.
pub(crate) fn emit_err_text_helper() -> Function {
    let (s, n, out, k, w, b) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
    let pay = almide_layout::PAYLOAD as i32;
    let byte = MemArg { offset: 0, align: 0, memory_index: 0 };
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    let put = |i: &mut wasm_encoder::InstructionSink<'_>, c: u8| {
        i.local_get(w).i32_const(c as i32).i32_store8(byte);
        i.local_get(w).i32_const(1).i32_add().local_set(w);
    };
    // One lowercase hex digit of `b >> shift`, written at `w`.
    let hex = |i: &mut wasm_encoder::InstructionSink<'_>, shift: i32| {
        i.local_get(w);
        i.local_get(b).i32_const(shift).i32_shr_u().i32_const(15).i32_and();
        i.local_get(b).i32_const(shift).i32_shr_u().i32_const(15).i32_and().i32_const(10).i32_lt_u();
        i.if_(BlockType::Result(ValType::I32)).i32_const('0' as i32).else_().i32_const('a' as i32 - 10).end();
        i.i32_add().i32_store8(byte);
        i.local_get(w).i32_const(1).i32_add().local_set(w);
    };
    i.local_get(s).i32_load(len_memarg()).local_set(n);
    i.local_get(n).i32_const(6).i32_mul().call(F_ALLOC).local_set(out);
    i.local_get(out).i32_const(pay).i32_add().local_set(w);
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(n).i32_ge_u().br_if(1);
    i.local_get(s).i32_const(pay).i32_add().local_get(k).i32_add().i32_load8_u(byte).local_set(b);
    i.block(BlockType::Empty);
    // Inside a case's `if`: if = 0, the per-byte block = 1, the loop = 2.
    for (c, esc) in [(b'\t', b't'), (b'\r', b'r'), (b'\n', b'n'), (0u8, b'0')] {
        i.local_get(b).i32_const(c as i32).i32_eq().if_(BlockType::Empty);
        put(&mut i, b'\\');
        put(&mut i, esc);
        i.local_get(k).i32_const(1).i32_add().local_set(k);
        i.br(2);
        i.end();
    }
    i.local_get(b).i32_const(0x20).i32_lt_u();
    i.local_get(b).i32_const(0x7f).i32_eq();
    i.i32_or().if_(BlockType::Empty);
    for c in *b"\\u{" {
        put(&mut i, c);
    }
    i.local_get(b).i32_const(16).i32_ge_u().if_(BlockType::Empty);
    hex(&mut i, 4);
    i.end();
    hex(&mut i, 0);
    put(&mut i, b'}');
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(2);
    i.end();
    // The multi-byte members of the set, matched as their UTF-8 bytes
    // (depth from inside the two ifs: 3 = the loop).
    for cp in 0x80u32..=0xffff {
        let Some(ch) = char::from_u32(cp) else { continue };
        if !almide_base::err_text::almide_rt_errtext_is_escaped(ch) {
            continue;
        }
        let mut buf = [0u8; 4];
        let seq = ch.encode_utf8(&mut buf).as_bytes().to_vec();
        let m = seq.len() as i32;
        i.local_get(k).i32_const(m).i32_add().local_get(n).i32_le_u().if_(BlockType::Empty);
        i.i32_const(1);
        for (j, sb) in seq.iter().enumerate() {
            i.local_get(s).i32_const(pay).i32_add().local_get(k).i32_add();
            i.i32_load8_u(MemArg { offset: j as u64, align: 0, memory_index: 0 });
            i.i32_const(*sb as i32).i32_eq().i32_and();
        }
        i.if_(BlockType::Empty);
        for c in format!("\\u{{{cp:x}}}").bytes() {
            put(&mut i, c);
        }
        i.local_get(k).i32_const(m).i32_add().local_set(k);
        i.br(3);
        i.end();
        i.end();
    }
    // Any other byte: copied.
    i.local_get(w).local_get(b).i32_store8(byte);
    i.local_get(w).i32_const(1).i32_add().local_set(w);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.end();
    i.br(0).end().end();
    i.local_get(out);
    i.local_get(w).local_get(out).i32_const(pay).i32_add().i32_sub();
    i.i32_store(len_memarg());
    i.local_get(out);
    i.end();
    f
}
