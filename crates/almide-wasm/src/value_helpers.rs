//! Synthesized Value/string helper FUNCTION BODIES (the `$value_eq`,
//! `$vjson*`, `$vfield`, `$vkeys`, `$split` forms) —
//! split from value.rs for the file budget. The Value layout contract
//! lives in value.rs's module doc.

use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::value::{value_tags, VT_ARRAY, VT_FLOAT, VT_INT, VT_OBJECT, VT_STR};
use crate::*;

pub(crate) fn raw8() -> MemArg {
    MemArg { offset: 0, align: 0, memory_index: 0 }
}

/// `$value_eq(a, b) -> i32` — deep structural Value equality over THIS
/// backend's layout (tag @SUM_TAG, payload @SUM_FIELD): tags must match,
/// Bool/Int by i64 payload, Float IEEE == (NaN never equal, ±0 equal),
/// Str by bytes, Array element-wise and Object pair-wise IN ORDER (the
/// oracle value_eq's exact walk).
pub(crate) fn emit_value_eq_helper(self_idx: u32, key_off: u32, val_off: u32) -> Function {
    let (a, b, ta, pa, pb, la, k) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32);
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(a).i32_load(slot_memarg(almide_layout::SUM_TAG)).local_set(ta);
    i.local_get(ta);
    i.local_get(b).i32_load(slot_memarg(almide_layout::SUM_TAG));
    i.i32_ne().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    // Null
    i.local_get(ta).i32_eqz().if_(BlockType::Empty);
    i.i32_const(1).return_();
    i.end();
    // Bool / Int: the i64 payload
    i.local_get(ta).i32_const(VT_INT).i32_le_u().if_(BlockType::Empty);
    i.local_get(a).i64_load(slot_memarg(almide_layout::SUM_FIELD));
    i.local_get(b).i64_load(slot_memarg(almide_layout::SUM_FIELD));
    i.i64_eq().return_();
    i.end();
    // Float: IEEE ==
    i.local_get(ta).i32_const(VT_FLOAT).i32_eq().if_(BlockType::Empty);
    i.local_get(a).f64_load(slot_memarg(almide_layout::SUM_FIELD));
    i.local_get(b).f64_load(slot_memarg(almide_layout::SUM_FIELD));
    i.f64_eq().return_();
    i.end();
    // Str: byte equality of the payload blocks
    i.local_get(a).i32_load(slot_memarg(almide_layout::SUM_FIELD)).local_set(pa);
    i.local_get(b).i32_load(slot_memarg(almide_layout::SUM_FIELD)).local_set(pb);
    i.local_get(ta).i32_const(VT_STR).i32_eq().if_(BlockType::Empty);
    i.local_get(pa).local_get(pb).call(F_STR_EQ).return_();
    i.end();
    // Array / Object: the payload lists' byte lengths must agree
    i.local_get(pa).i32_load(len_memarg()).local_set(la);
    i.local_get(la);
    i.local_get(pb).i32_load(len_memarg());
    i.i32_ne().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(la).i32_ge_u().br_if(1);
    i.local_get(ta).i32_const(VT_ARRAY).i32_eq().if_(BlockType::Empty);
    // Array element: recurse on the two Value addresses
    i.local_get(pa).local_get(k).i32_add().i32_load(slot_memarg(0));
    i.local_get(pb).local_get(k).i32_add().i32_load(slot_memarg(0));
    i.call(self_idx).i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.else_();
    // Object pair: key strings, then the values
    i.local_get(pa)
        .local_get(k)
        .i32_add()
        .i32_load(slot_memarg(0))
        .i32_load(slot_memarg(key_off));
    i.local_get(pb)
        .local_get(k)
        .i32_add()
        .i32_load(slot_memarg(0))
        .i32_load(slot_memarg(key_off));
    i.call(F_STR_EQ).i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.local_get(pa)
        .local_get(k)
        .i32_add()
        .i32_load(slot_memarg(0))
        .i32_load(slot_memarg(val_off));
    i.local_get(pb)
        .local_get(k)
        .i32_add()
        .i32_load(slot_memarg(0))
        .i32_load(slot_memarg(val_off));
    i.call(self_idx).i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.end();
    i.local_get(k).i32_const(4).i32_add().local_set(k);
    i.br(0).end().end();
    i.i32_const(1).end();
    f
}

/// `$vfield(v, key) -> i32`: 0 not-object / 1 missing / value address.
pub(crate) fn emit_value_field_helper() -> Function {
    let (v, key, p, end, pair) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let m_tag = slot_memarg(almide_layout::SUM_TAG);
    let m_pay = slot_memarg(almide_layout::SUM_FIELD);
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).i32_load(m_tag).i32_const(value_tags::VT_OBJECT).i32_ne();
    i.if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.local_get(v).i32_load(m_pay).local_set(pair); // pairs list (reuse local)
    i.local_get(pair).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(p);
    i.local_get(p).local_get(pair).i32_load(len_memarg()).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p).i32_load(raw8()).local_set(pair);
    i.local_get(pair).i32_load(slot_memarg(0)).local_get(key).call(F_STR_EQ);
    i.if_(BlockType::Empty);
    i.local_get(pair).i32_load(slot_memarg(4)).return_();
    i.end();
    i.local_get(p).i32_const(4).i32_add().local_set(p);
    i.br(0);
    i.end();
    i.end();
    i.i32_const(1);
    i.end();
    f
}

/// `$vkeys(v) -> i32`: the keys as a List[String] (fresh block; key
/// addresses shared — strings are immutable).
///
/// The payload slot is a pairs-list handle ONLY under tag Object, so the
/// walk is guarded by the tag (#2395). Every other tag answers `[]`, the
/// oracle's answer: `value_core`'s `value_keys` reads the pair count out
/// of the slot-count word, which is 0 for every non-object. Unguarded,
/// this helper read the slot whatever the tag said — an Int's payload as
/// a block address, a Str's payload as a pairs list, and, worst, Null's
/// slot, which `emit_value_box(_, None)` never writes at all, so it held
/// whatever the reused block last contained.
pub(crate) fn emit_value_keys_helper() -> Function {
    let (v, p, end, dst, cur) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let m_pay = slot_memarg(almide_layout::SUM_FIELD);
    let mut f = Function::new([(4, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v)
        .i32_load(slot_memarg(almide_layout::SUM_TAG))
        .i32_const(VT_OBJECT)
        .i32_ne()
        .if_(BlockType::Empty);
    i.i32_const(0).call(F_ALLOC).return_();
    i.end();
    i.local_get(v).i32_load(m_pay).local_set(v); // pairs list
    i.local_get(v).i32_load(len_memarg()).call(F_ALLOC).local_set(dst);
    i.local_get(v).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(p);
    i.local_get(p).local_get(v).i32_load(len_memarg()).i32_add().local_set(end);
    i.local_get(dst).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(cur);
    i.local_get(p).i32_load(raw8()).i32_load(slot_memarg(0));
    i.i32_store(raw8());
    i.local_get(p).i32_const(4).i32_add().local_set(p);
    i.local_get(cur).i32_const(4).i32_add().local_set(cur);
    i.br(0);
    i.end();
    i.end();
    i.local_get(dst);
    i.end();
    f
}

/// `$split(s, sep) -> List[String]` — two passes: count pieces, then
/// alloc + fill (each piece a fresh owned string), both over KMP searches.
pub(crate) fn emit_string_split_helper() -> Function {
    let (sb, sep) = (0u32, 1u32);
    let (slen, seplen, p, j, cnt, dst, slot, start, piece) =
        (2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32);
    // the KMP registers (j doubles as its cursor)
    let (tbl, k, c, res) = (11u32, 12u32, 13u32, 14u32);
    let pay = almide_layout::PAYLOAD as i32;
    let mut f = Function::new([(13, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(sb).i32_load(len_memarg()).local_set(slen);
    i.local_get(sep).i32_load(len_memarg()).local_set(seplen);
    // C-100: the EMPTY separator is Rust's char-boundary split — a
    // leading "", each CHAR (multibyte whole), a trailing "".
    i.local_get(seplen).i32_eqz().if_(BlockType::Empty);
    i.local_get(sb).call(F_STR_LEN_CHARS).i32_wrap_i64().i32_const(2).i32_add().local_set(cnt);
    i.local_get(cnt).i32_const(4).i32_mul().call(F_ALLOC).local_set(dst);
    i.local_get(dst).i32_const(pay).i32_add().local_set(slot);
    // leading ""
    i.i32_const(0).call(F_ALLOC).local_set(piece);
    i.local_get(slot).local_get(piece).i32_store(raw8());
    i.local_get(slot).i32_const(4).i32_add().local_set(slot);
    i.i32_const(0).local_set(p);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(slen).i32_ge_u().br_if(1);
    // char byte length by lead-byte class
    i.local_get(sb).i32_const(pay).i32_add().local_get(p).i32_add().i32_load8_u(raw8()).local_set(j);
    i.local_get(j).i32_const(0x80).i32_lt_u().if_(BlockType::Result(ValType::I32));
    i.i32_const(1);
    i.else_();
    i.local_get(j).i32_const(0xE0).i32_lt_u().if_(BlockType::Result(ValType::I32));
    i.i32_const(2);
    i.else_();
    i.local_get(j).i32_const(0xF0).i32_lt_u().if_(BlockType::Result(ValType::I32));
    i.i32_const(3);
    i.else_();
    i.i32_const(4);
    i.end();
    i.end();
    i.end();
    i.local_set(j); // j = char byte length
    i.local_get(j).call(F_ALLOC).local_set(piece);
    i.local_get(piece).i32_const(pay).i32_add();
    i.local_get(sb).i32_const(pay).i32_add().local_get(p).i32_add();
    i.local_get(j);
    i.memory_copy(0, 0);
    i.local_get(slot).local_get(piece).i32_store(raw8());
    i.local_get(slot).i32_const(4).i32_add().local_set(slot);
    i.local_get(p).local_get(j).i32_add().local_set(p);
    i.br(0);
    i.end();
    i.end();
    // trailing ""
    i.i32_const(0).call(F_ALLOC).local_set(piece);
    i.local_get(slot).local_get(piece).i32_store(raw8());
    i.local_get(dst).return_();
    i.end();
    // Both passes are forward walks of KMP searches, each resuming past the
    // last separator with one table (crate::kmp, #4206).
    let kmp = crate::kmp::Kmp { hay: sb, n: slen, pat: sep, m: seplen, tbl, k, c, q: j };
    kmp.emit_table(&mut i);
    // ── pass 1: count = 1 + matches ──
    i.i32_const(1).local_set(cnt);
    i.i32_const(0).local_set(p);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_set(j);
    kmp.emit_find(&mut i, res);
    i.local_get(res).i32_const(0).i32_lt_s().br_if(1);
    i.local_get(cnt).i32_const(1).i32_add().local_set(cnt);
    i.local_get(res).local_get(seplen).i32_add().local_set(p);
    i.br(0);
    i.end();
    i.end();
    // ── alloc the result list (4-byte string slots) ──
    i.local_get(cnt).i32_const(4).i32_mul().call(F_ALLOC).local_set(dst);
    i.local_get(dst).i32_const(pay).i32_add().local_set(slot);
    // ── pass 2: fill ──
    i.i32_const(0).local_set(start);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(start).local_set(j);
    kmp.emit_find(&mut i, res);
    i.local_get(res).i32_const(0).i32_lt_s().br_if(1);
    // piece = s[start, res)
    i.local_get(res).local_get(start).i32_sub().call(F_ALLOC).local_set(piece);
    i.local_get(piece).i32_const(pay).i32_add();
    i.local_get(sb).i32_const(pay).i32_add().local_get(start).i32_add();
    i.local_get(res).local_get(start).i32_sub();
    i.memory_copy(0, 0);
    i.local_get(slot).local_get(piece).i32_store(raw8());
    i.local_get(slot).i32_const(4).i32_add().local_set(slot);
    i.local_get(res).local_get(seplen).i32_add().local_set(start);
    i.br(0);
    i.end();
    i.end();
    kmp.emit_free(&mut i);
    // final piece [start, slen)
    i.local_get(slen).local_get(start).i32_sub().call(F_ALLOC).local_set(piece);
    i.local_get(piece).i32_const(pay).i32_add();
    i.local_get(sb).i32_const(pay).i32_add().local_get(start).i32_add();
    i.local_get(slen).local_get(start).i32_sub();
    i.memory_copy(0, 0);
    i.local_get(slot).local_get(piece).i32_store(raw8());
    i.local_get(dst);
    i.end();
    f
}
