//! Synthesized JSON serializer helper FUNCTION BODIES (`$vjson`,
//! `$vjson_pretty`, `$vjson_quote`) — split from value_helpers.rs for the
//! file budget, the same way utf8_helpers.rs was. The Value layout contract
//! lives in value.rs's module doc; `raw8` is shared and stays there.

use wasm_encoder::{BlockType, Function, ValType};

use crate::value_helpers::raw8;
use crate::*;

/// cursor = append_copy(cursor, frag_payload, len) — helper-body form.
fn frag(i: &mut wasm_encoder::InstructionSink, cursor: u32, addr: u32, len: i32) {
    i.local_get(cursor)
        .i32_const(addr as i32 + almide_layout::PAYLOAD as i32)
        .i32_const(len)
        .call(F_APPEND_COPY)
        .local_set(cursor);
}

/// Pushes an i32 that is nonzero iff the f64 at `v`'s payload slot is NaN or
/// ±infinity — the values a JSON number cannot spell (#2499).
fn emit_nonfinite_test(i: &mut wasm_encoder::InstructionSink, v: u32, m_pay: wasm_encoder::MemArg) {
    i.local_get(v).f64_load(m_pay).local_get(v).f64_load(m_pay).f64_ne();
    i.local_get(v).f64_load(m_pay).f64_abs().f64_const(f64::INFINITY.into()).f64_eq();
    i.i32_or();
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
    // plain byte — a direct store at the PHYSICAL cursor, the room grown
    // first when the byte would leave it (#1826; the guard used to trap).
    i.local_get(cursor).global_get(G_LINE_ROOM).i32_ge_u().if_(BlockType::Empty);
    i.local_get(cursor).i32_const(1).call(F_LINE_GROW);
    i.end();
    i.local_get(cursor).global_get(G_LINE_DELTA).i32_add().local_get(b).i32_store8(raw8());
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
    // 3 float — LINKED float.to_string, minus a trailing ".0"; a NON-FINITE
    // float is the JSON `null` (#2499, C-356): `x != x` is NaN, `|x| == inf`
    // is either infinity, and JSON has no spelling for any of the three.
    i.local_get(t).i32_const(3).i32_eq().if_(BlockType::Empty);
    emit_nonfinite_test(&mut i, v, m_pay);
    i.if_(BlockType::Empty);
    frag(&mut i, cursor, frags.null_, 4);
    i.else_();
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
    i.end();
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
    // 3 float — LINKED float.to_string, minus a trailing ".0"; a NON-FINITE
    // float is the JSON `null` (#2499, C-356): `x != x` is NaN, `|x| == inf`
    // is either infinity, and JSON has no spelling for any of the three.
    i.local_get(t).i32_const(3).i32_eq().if_(BlockType::Empty);
    emit_nonfinite_test(&mut i, v, m_pay);
    i.if_(BlockType::Empty);
    frag(&mut i, cursor, frags.null_, 4);
    i.else_();
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
    i.end();
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
