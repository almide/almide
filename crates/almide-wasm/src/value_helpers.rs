//! Synthesized Value/string helper FUNCTION BODIES (the `$value_eq`,
//! `$value_merge`, `$vjson*`, `$vfield`, `$vkeys`, `$split` forms) —
//! split from value.rs for the file budget. The Value layout contract
//! lives in value.rs's module doc.

use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::value::{value_tags, VT_ARRAY, VT_FLOAT, VT_INT, VT_OBJECT, VT_STR};
use crate::*;

fn raw8() -> MemArg {
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

/// `$value_merge(a, b) -> i32` — object merge (the oracle value_merge):
/// A's pairs in order (a shared key takes B's VALUE, keeping A's key
/// object), then B's pairs whose keys are new, in B order; a fresh pair
/// tuple only where overridden (immutable sharing elsewhere). Any
/// non-Object operand yields b itself.
pub(crate) fn emit_value_merge_helper(key_off: u32, val_off: u32) -> Function {
    let (a, b, pa, pb, la, lb, out, w, i, j, ka, fd) =
        (0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32);
    let fv = 12u32;
    let m_tag = slot_memarg(almide_layout::SUM_TAG);
    let m_pay = slot_memarg(almide_layout::SUM_FIELD);
    let mut f = Function::new([(11, ValType::I32)]);
    let mut ins = f.instructions();
    ins.local_get(a).i32_load(m_tag).i32_const(VT_OBJECT).i32_ne();
    ins.local_get(b).i32_load(m_tag).i32_const(VT_OBJECT).i32_ne();
    ins.i32_or().if_(BlockType::Empty);
    ins.local_get(b).return_();
    ins.end();
    ins.local_get(a).i32_load(m_pay).local_set(pa);
    ins.local_get(b).i32_load(m_pay).local_set(pb);
    ins.local_get(pa).i32_load(len_memarg()).local_set(la);
    ins.local_get(pb).i32_load(len_memarg()).local_set(lb);
    // count B's NEW keys (bytes) into w
    ins.i32_const(0).local_set(w);
    ins.i32_const(0).local_set(j);
    ins.block(BlockType::Empty).loop_(BlockType::Empty);
    ins.local_get(j).local_get(lb).i32_ge_u().br_if(1);
    ins.local_get(pb).local_get(j).i32_add().i32_load(slot_memarg(0));
    ins.i32_load(slot_memarg(key_off)).local_set(ka);
    ins.i32_const(0).local_set(fd);
    ins.i32_const(0).local_set(i);
    ins.block(BlockType::Empty).loop_(BlockType::Empty);
    ins.local_get(i).local_get(la).i32_ge_u().br_if(1);
    ins.local_get(pa).local_get(i).i32_add().i32_load(slot_memarg(0));
    ins.i32_load(slot_memarg(key_off));
    ins.local_get(ka).call(F_STR_EQ).if_(BlockType::Empty);
    ins.i32_const(1).local_set(fd);
    ins.br(2);
    ins.end();
    ins.local_get(i).i32_const(4).i32_add().local_set(i);
    ins.br(0).end().end();
    ins.local_get(fd).i32_eqz().if_(BlockType::Empty);
    ins.local_get(w).i32_const(4).i32_add().local_set(w);
    ins.end();
    ins.local_get(j).i32_const(4).i32_add().local_set(j);
    ins.br(0).end().end();
    ins.local_get(la).local_get(w).i32_add().call(F_ALLOC).local_set(out);
    // pass A: value overridden where B has the key
    ins.i32_const(0).local_set(i);
    ins.block(BlockType::Empty).loop_(BlockType::Empty);
    ins.local_get(i).local_get(la).i32_ge_u().br_if(1);
    ins.local_get(pa).local_get(i).i32_add().i32_load(slot_memarg(0)).local_set(fd);
    ins.local_get(fd).i32_load(slot_memarg(key_off)).local_set(ka);
    // scan B
    ins.i32_const(0).local_set(fv);
    ins.i32_const(0).local_set(j);
    ins.block(BlockType::Empty).loop_(BlockType::Empty);
    ins.local_get(j).local_get(lb).i32_ge_u().br_if(1);
    ins.local_get(pb).local_get(j).i32_add().i32_load(slot_memarg(0));
    ins.i32_load(slot_memarg(key_off));
    ins.local_get(ka).call(F_STR_EQ).if_(BlockType::Empty);
    ins.local_get(pb)
        .local_get(j)
        .i32_add()
        .i32_load(slot_memarg(0))
        .i32_load(slot_memarg(val_off))
        .local_set(fv);
    // a fresh (key, b-val) pair replaces fd
    ins.i32_const(8).call(F_ALLOC).local_tee(w);
    ins.local_get(ka).i32_store(slot_memarg(key_off));
    ins.local_get(w).local_get(fv).i32_store(slot_memarg(val_off));
    ins.local_get(w).local_set(fd);
    ins.br(2);
    ins.end();
    ins.local_get(j).i32_const(4).i32_add().local_set(j);
    ins.br(0).end().end();
    ins.local_get(out).local_get(i).i32_add().local_get(fd).i32_store(slot_memarg(0));
    ins.local_get(i).i32_const(4).i32_add().local_set(i);
    ins.br(0).end().end();
    // pass B: append the new keys (shared pair tuples), cursor after A
    ins.local_get(la).local_set(w);
    ins.i32_const(0).local_set(j);
    ins.block(BlockType::Empty).loop_(BlockType::Empty);
    ins.local_get(j).local_get(lb).i32_ge_u().br_if(1);
    ins.local_get(pb).local_get(j).i32_add().i32_load(slot_memarg(0));
    ins.i32_load(slot_memarg(key_off)).local_set(ka);
    ins.i32_const(0).local_set(fd);
    ins.i32_const(0).local_set(i);
    ins.block(BlockType::Empty).loop_(BlockType::Empty);
    ins.local_get(i).local_get(la).i32_ge_u().br_if(1);
    ins.local_get(pa).local_get(i).i32_add().i32_load(slot_memarg(0));
    ins.i32_load(slot_memarg(key_off));
    ins.local_get(ka).call(F_STR_EQ).if_(BlockType::Empty);
    ins.i32_const(1).local_set(fd);
    ins.br(2);
    ins.end();
    ins.local_get(i).i32_const(4).i32_add().local_set(i);
    ins.br(0).end().end();
    ins.local_get(fd).i32_eqz().if_(BlockType::Empty);
    ins.local_get(out).local_get(w).i32_add();
    ins.local_get(pb).local_get(j).i32_add().i32_load(slot_memarg(0));
    ins.i32_store(slot_memarg(0));
    ins.local_get(w).i32_const(4).i32_add().local_set(w);
    ins.end();
    ins.local_get(j).i32_const(4).i32_add().local_set(j);
    ins.br(0).end().end();
    // box: a fresh Object Value over the merged pairs
    ins.i32_const(16).call(F_ALLOC).local_tee(fd);
    ins.i32_const(VT_OBJECT).i32_store(m_tag);
    ins.local_get(fd).local_get(out).i32_store(m_pay);
    ins.local_get(fd);
    ins.end();
    f
}

/// cursor = append_copy(cursor, frag_payload, len) — helper-body form.
fn frag(i: &mut wasm_encoder::InstructionSink, cursor: u32, addr: u32, len: i32) {
    i.local_get(cursor)
        .i32_const(addr as i32 + almide_layout::PAYLOAD as i32)
        .i32_const(len)
        .call(F_APPEND_COPY)
        .local_set(cursor);
}

/// `$vjson_quote(cursor, str) -> cursor`: '"', the incumbent's exact
/// 5-escape set (\\ \" \n \r \t — no control-char \u escapes), '"'.
pub(crate) fn emit_json_quote_helper(frags: JsonFrags) -> Function {
    let (cursor, sb, p, end, b) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    frag(&mut i, cursor, frags.quote, 1);
    i.local_get(sb).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(p);
    i.local_get(p).local_get(sb).i32_load(len_memarg()).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p).i32_load8_u(raw8()).local_set(b);
    for (byte, fr) in [
        (92, frags.esc_backslash),
        (34, frags.esc_quote),
        (10, frags.esc_n),
        (13, frags.esc_r),
        (9, frags.esc_t),
    ] {
        i.local_get(b).i32_const(byte).i32_eq().if_(BlockType::Empty);
        frag(&mut i, cursor, fr, 2);
        i.else_();
    }
    // plain byte — bounds-guarded direct store
    i.local_get(cursor).global_get(G_LINE_END).i32_ge_u().if_(BlockType::Empty);
    i.unreachable();
    i.end();
    i.local_get(cursor).local_get(b).i32_store8(raw8());
    i.local_get(cursor).i32_const(1).i32_add().local_set(cursor);
    for _ in 0..5 {
        i.end();
    }
    i.local_get(p).i32_const(1).i32_add().local_set(p);
    i.br(0);
    i.end();
    i.end();
    frag(&mut i, cursor, frags.quote, 1);
    i.local_get(cursor);
    i.end();
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
pub(crate) fn emit_value_keys_helper() -> Function {
    let (v, p, end, dst, cur) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let m_pay = slot_memarg(almide_layout::SUM_FIELD);
    let mut f = Function::new([(4, ValType::I32)]);
    let mut i = f.instructions();
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
/// alloc + fill (each piece a fresh owned string).
pub(crate) fn emit_string_split_helper() -> Function {
    let (sb, sep) = (0u32, 1u32);
    let (slen, seplen, p, j, cnt, dst, slot, start, piece) =
        (2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32);
    let pay = almide_layout::PAYLOAD as i32;
    let mut f = Function::new([(9, ValType::I32)]);
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
    // Pass shared matcher: at position p, do seplen bytes match?
    // (emitted twice — once per pass — via this closure)
    let emit_match = |i: &mut wasm_encoder::InstructionSink, hit_then_else: &mut dyn FnMut(&mut wasm_encoder::InstructionSink)| {
        // j = 0; loop { j >= seplen -> HIT; bytes differ -> MISS }
        i.local_set(j); // expects 0 pushed by caller
        i.block(BlockType::Result(ValType::I32)); // yields 1 hit / 0 miss
        i.loop_(BlockType::Empty);
        i.local_get(j).local_get(seplen).i32_ge_u().if_(BlockType::Empty);
        i.i32_const(1).br(2);
        i.end();
        i.local_get(sb).i32_const(pay).i32_add().local_get(p).i32_add().local_get(j).i32_add().i32_load8_u(raw8());
        i.local_get(sep).i32_const(pay).i32_add().local_get(j).i32_add().i32_load8_u(raw8());
        i.i32_ne().if_(BlockType::Empty);
        i.i32_const(0).br(2);
        i.end();
        i.local_get(j).i32_const(1).i32_add().local_set(j);
        i.br(0);
        i.end();
        i.i32_const(0); // unreachable filler for the block result
        i.end();
        hit_then_else(i);
    };
    // ── pass 1: count = 1 + matches ──
    i.i32_const(1).local_set(cnt);
    i.i32_const(0).local_set(p);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(seplen).i32_add().local_get(slen).i32_gt_u().br_if(1);
    i.i32_const(0);
    emit_match(&mut i, &mut |i| {
        i.if_(BlockType::Empty);
        i.local_get(cnt).i32_const(1).i32_add().local_set(cnt);
        i.local_get(p).local_get(seplen).i32_add().local_set(p);
        i.else_();
        i.local_get(p).i32_const(1).i32_add().local_set(p);
        i.end();
    });
    i.br(0);
    i.end();
    i.end();
    // ── alloc the result list (4-byte string slots) ──
    i.local_get(cnt).i32_const(4).i32_mul().call(F_ALLOC).local_set(dst);
    i.local_get(dst).i32_const(pay).i32_add().local_set(slot);
    // ── pass 2: fill ──
    i.i32_const(0).local_set(p);
    i.i32_const(0).local_set(start);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(seplen).i32_add().local_get(slen).i32_gt_u().br_if(1);
    i.i32_const(0);
    emit_match(&mut i, &mut |i| {
        i.if_(BlockType::Empty);
        // piece = s[start, p)
        i.local_get(p).local_get(start).i32_sub().call(F_ALLOC).local_set(piece);
        i.local_get(piece).i32_const(pay).i32_add();
        i.local_get(sb).i32_const(pay).i32_add().local_get(start).i32_add();
        i.local_get(p).local_get(start).i32_sub();
        i.memory_copy(0, 0);
        i.local_get(slot).local_get(piece).i32_store(raw8());
        i.local_get(slot).i32_const(4).i32_add().local_set(slot);
        i.local_get(p).local_get(seplen).i32_add().local_set(p);
        i.local_get(p).local_set(start);
        i.else_();
        i.local_get(p).i32_const(1).i32_add().local_set(p);
        i.end();
    });
    i.br(0);
    i.end();
    i.end();
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

/// `$vjson(cursor, value) -> cursor` — the recursive serializer.
/// `$vjson_pretty(cursor, v, depth) -> cursor` — value_core's
/// json_stringify_pretty_at verbatim: identical leaves; an empty array/
/// object prints "[]"/"{}"; else "[\n" items "\n" indent(d) "]" with
/// per-item ",\n"-separated indent(d+1) pieces; two-space indents.
pub(crate) fn emit_json_value_pretty_helper(
    helper_base: u32,
    helpers: &[Helper],
    float_to_string: u32,
    frags: JsonFrags,
    pf: crate::work::PrettyFrags,
) -> Function {
    let self_idx = helper_base
        + helpers
            .iter()
            .position(|h| matches!(h, Helper::JsonValuePretty { .. }))
            .expect("registered") as u32;
    let quote_idx = helper_base
        + helpers
            .iter()
            .position(|h| matches!(h, Helper::JsonQuote { .. }))
            .expect("registered") as u32;
    let (cursor, v, depth, t, p, end, s32, l32, k) =
        (0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32);
    let m_tag = slot_memarg(almide_layout::SUM_TAG);
    let m_pay = slot_memarg(almide_layout::SUM_FIELD);
    let mut f = Function::new([(6, ValType::I32)]);
    let mut i = f.instructions();
    // indent(n): append "  " n times — inlined at each use via a macro-ish
    // closure over the instruction sink.
    let indent = |i: &mut wasm_encoder::InstructionSink, upto_depth_plus: i32| {
        // k = depth (+1 when upto_depth_plus == 1); loop appending "  "
        i.local_get(depth);
        if upto_depth_plus == 1 {
            i.i32_const(1).i32_add();
        }
        i.local_set(k);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(k).i32_const(0).i32_le_s().br_if(1);
        frag(i, cursor, pf.indent2, 2);
        i.local_get(k).i32_const(1).i32_sub().local_set(k);
        i.br(0).end().end();
    };
    i.local_get(v).i32_load(m_tag).local_set(t);
    // 0 null
    i.local_get(t).i32_eqz().if_(BlockType::Empty);
    frag(&mut i, cursor, frags.null_, 4);
    i.else_();
    // 1 bool
    i.local_get(t).i32_const(1).i32_eq().if_(BlockType::Empty);
    i.local_get(v).i64_load(m_pay).i64_eqz().if_(BlockType::Empty);
    frag(&mut i, cursor, frags.false_, 5);
    i.else_();
    frag(&mut i, cursor, frags.true_, 4);
    i.end();
    i.else_();
    // 2 int
    i.local_get(t).i32_const(2).i32_eq().if_(BlockType::Empty);
    i.local_get(cursor).local_get(v).i64_load(m_pay).call(F_APPEND_I64).local_set(cursor);
    i.else_();
    // 3 float — LINKED float.to_string, minus a trailing ".0"
    i.local_get(t).i32_const(3).i32_eq().if_(BlockType::Empty);
    i.local_get(cursor).global_set(G_LINE_CURSOR);
    i.local_get(v).f64_load(m_pay).call(float_to_string).local_set(s32);
    i.local_get(s32).i32_load(len_memarg()).local_set(l32);
    i.local_get(l32).i32_const(2).i32_ge_s();
    i.local_get(s32)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .local_get(l32)
        .i32_add()
        .i32_const(2)
        .i32_sub()
        .i32_load16_u(raw8())
        .i32_const(0x302e)
        .i32_eq();
    i.i32_and().if_(BlockType::Empty);
    i.local_get(l32).i32_const(2).i32_sub().local_set(l32);
    i.end();
    i.local_get(cursor)
        .local_get(s32)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .local_get(l32)
        .call(F_APPEND_COPY)
        .local_set(cursor);
    i.else_();
    // 4 str
    i.local_get(t).i32_const(4).i32_eq().if_(BlockType::Empty);
    i.local_get(cursor).local_get(v).i32_load(m_pay).call(quote_idx).local_set(cursor);
    i.else_();
    // 5 array
    i.local_get(t).i32_const(5).i32_eq().if_(BlockType::Empty);
    i.local_get(v).i32_load(m_pay).local_set(s32);
    i.local_get(s32).i32_load(len_memarg()).i32_eqz().if_(BlockType::Empty);
    frag(&mut i, cursor, pf.empty_arr, 2);
    i.else_();
    frag(&mut i, cursor, frags.lbrack, 1);
    frag(&mut i, cursor, pf.nl, 1);
    i.local_get(s32).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(p);
    i.local_get(p).local_get(s32).i32_load(len_memarg()).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p)
        .local_get(s32)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .i32_ne()
        .if_(BlockType::Empty);
    frag(&mut i, cursor, pf.comma_nl, 2);
    i.end();
    indent(&mut i, 1);
    i.local_get(cursor).local_get(p).i32_load(raw8());
    i.local_get(depth).i32_const(1).i32_add();
    i.call(self_idx).local_set(cursor);
    i.local_get(p).i32_const(4).i32_add().local_set(p);
    i.br(0);
    i.end();
    i.end();
    frag(&mut i, cursor, pf.nl, 1);
    indent(&mut i, 0);
    frag(&mut i, cursor, frags.rbrack, 1);
    i.end();
    i.else_();
    // 6 object
    i.local_get(t).i32_const(6).i32_eq().if_(BlockType::Empty);
    i.local_get(v).i32_load(m_pay).local_set(s32);
    i.local_get(s32).i32_load(len_memarg()).i32_eqz().if_(BlockType::Empty);
    frag(&mut i, cursor, pf.empty_obj, 2);
    i.else_();
    frag(&mut i, cursor, frags.lbrace, 1);
    frag(&mut i, cursor, pf.nl, 1);
    i.local_get(s32).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(p);
    i.local_get(p).local_get(s32).i32_load(len_memarg()).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p)
        .local_get(s32)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .i32_ne()
        .if_(BlockType::Empty);
    frag(&mut i, cursor, pf.comma_nl, 2);
    i.end();
    indent(&mut i, 1);
    i.local_get(p).i32_load(raw8()).local_set(l32);
    i.local_get(cursor)
        .local_get(l32)
        .i32_load(slot_memarg(0))
        .call(quote_idx)
        .local_set(cursor);
    frag(&mut i, cursor, pf.colon_sp, 2);
    i.local_get(cursor)
        .local_get(l32)
        .i32_load(slot_memarg(4))
        .local_get(depth)
        .i32_const(1)
        .i32_add()
        .call(self_idx)
        .local_set(cursor);
    i.local_get(p).i32_const(4).i32_add().local_set(p);
    i.br(0);
    i.end();
    i.end();
    frag(&mut i, cursor, pf.nl, 1);
    indent(&mut i, 0);
    frag(&mut i, cursor, frags.rbrace, 1);
    i.end();
    i.else_();
    frag(&mut i, cursor, frags.null_, 4);
    for _ in 0..7 {
        i.end();
    }
    i.local_get(cursor);
    i.end();
    f
}

pub(crate) fn emit_json_value_helper(
    helper_base: u32,
    helpers: &[Helper],
    float_to_string: u32,
    frags: JsonFrags,
) -> Function {
    let self_idx = helper_base
        + helpers
            .iter()
            .position(|h| matches!(h, Helper::JsonValue { .. }))
            .expect("registered") as u32;
    let quote_idx = helper_base
        + helpers
            .iter()
            .position(|h| matches!(h, Helper::JsonQuote { .. }))
            .expect("registered") as u32;
    let (cursor, v, t, p, end, s32, l32) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32);
    let m_tag = slot_memarg(almide_layout::SUM_TAG);
    let m_pay = slot_memarg(almide_layout::SUM_FIELD);
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).i32_load(m_tag).local_set(t);
    // 0 null
    i.local_get(t).i32_eqz().if_(BlockType::Empty);
    frag(&mut i, cursor, frags.null_, 4);
    i.else_();
    // 1 bool
    i.local_get(t).i32_const(1).i32_eq().if_(BlockType::Empty);
    i.local_get(v).i64_load(m_pay).i64_eqz().if_(BlockType::Empty);
    frag(&mut i, cursor, frags.false_, 5);
    i.else_();
    frag(&mut i, cursor, frags.true_, 4);
    i.end();
    i.else_();
    // 2 int
    i.local_get(t).i32_const(2).i32_eq().if_(BlockType::Empty);
    i.local_get(cursor).local_get(v).i64_load(m_pay).call(F_APPEND_I64).local_set(cursor);
    i.else_();
    // 3 float — LINKED float.to_string, minus a trailing ".0"
    i.local_get(t).i32_const(3).i32_eq().if_(BlockType::Empty);
    i.local_get(cursor).global_set(G_LINE_CURSOR);
    i.local_get(v).f64_load(m_pay).call(float_to_string).local_set(s32);
    i.local_get(s32).i32_load(len_memarg()).local_set(l32);
    i.local_get(l32).i32_const(2).i32_ge_s();
    i.local_get(s32)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .local_get(l32)
        .i32_add()
        .i32_const(2)
        .i32_sub()
        .i32_load16_u(raw8())
        .i32_const(0x302e) // ".0" little-endian
        .i32_eq();
    i.i32_and().if_(BlockType::Empty);
    i.local_get(l32).i32_const(2).i32_sub().local_set(l32);
    i.end();
    i.local_get(cursor)
        .local_get(s32)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .local_get(l32)
        .call(F_APPEND_COPY)
        .local_set(cursor);
    i.else_();
    // 4 str
    i.local_get(t).i32_const(4).i32_eq().if_(BlockType::Empty);
    i.local_get(cursor).local_get(v).i32_load(m_pay).call(quote_idx).local_set(cursor);
    i.else_();
    // 5 array: [ e , e ]
    i.local_get(t).i32_const(5).i32_eq().if_(BlockType::Empty);
    frag(&mut i, cursor, frags.lbrack, 1);
    i.local_get(v).i32_load(m_pay).local_set(s32);
    i.local_get(s32).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(p);
    i.local_get(p).local_get(s32).i32_load(len_memarg()).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p)
        .local_get(s32)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .i32_ne()
        .if_(BlockType::Empty);
    frag(&mut i, cursor, frags.comma, 1);
    i.end();
    i.local_get(cursor).local_get(p).i32_load(raw8()).call(self_idx).local_set(cursor);
    i.local_get(p).i32_const(4).i32_add().local_set(p);
    i.br(0);
    i.end();
    i.end();
    frag(&mut i, cursor, frags.rbrack, 1);
    i.else_();
    // 6 object: { "k" : v , ... } over the (Str, Value) pairs list
    i.local_get(t).i32_const(6).i32_eq().if_(BlockType::Empty);
    frag(&mut i, cursor, frags.lbrace, 1);
    i.local_get(v).i32_load(m_pay).local_set(s32);
    i.local_get(s32).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(p);
    i.local_get(p).local_get(s32).i32_load(len_memarg()).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p)
        .local_get(s32)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .i32_ne()
        .if_(BlockType::Empty);
    frag(&mut i, cursor, frags.comma, 1);
    i.end();
    // pair block: key @ payload+0, value @ payload+4
    i.local_get(p).i32_load(raw8()).local_set(l32);
    i.local_get(cursor)
        .local_get(l32)
        .i32_load(slot_memarg(0))
        .call(quote_idx)
        .local_set(cursor);
    frag(&mut i, cursor, frags.colon, 1);
    i.local_get(cursor)
        .local_get(l32)
        .i32_load(slot_memarg(4))
        .call(self_idx)
        .local_set(cursor);
    i.local_get(p).i32_const(4).i32_add().local_set(p);
    i.br(0);
    i.end();
    i.end();
    frag(&mut i, cursor, frags.rbrace, 1);
    i.else_();
    // unknown tag — the incumbent renders "null"
    frag(&mut i, cursor, frags.null_, 4);
    for _ in 0..7 {
        i.end();
    }
    i.local_get(cursor);
    i.end();
    f
}


