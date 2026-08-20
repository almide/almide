//! The dynamic `Value` model (Codec/json's data carrier), NATIVE to this
//! backend's ratified layout (2026-08-20 ○: rebuild, do not adopt the
//! incumbent's len-as-tag convention). A Value is a 16-byte block:
//! `[rc][len][cap][tag:i32 @SUM_TAG][pad][payload:8B @SUM_FIELD]` — the
//! SAME offsets Result blocks use, so the machinery reads familiarly.
//! Tags: 0=Null, 1=Bool, 2=Int, 3=Float, 4=Str, 5=Array, 6=Object.
//! Str/Array payloads hold BLOCK ADDRESSES (our 4-byte-slot lists);
//! sharing is unobservable because the Value API never mutates in place.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::emitter::Emitter;
use crate::*;

fn raw8() -> MemArg {
    MemArg { offset: 0, align: 0, memory_index: 0 }
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

pub(crate) mod value_tags {
    pub(crate) const VT_OBJECT: i32 = 6;
}
pub(crate) const VT_NULL: i32 = 0;
pub(crate) const VT_BOOL: i32 = 1;
pub(crate) const VT_INT: i32 = 2;
pub(crate) const VT_FLOAT: i32 = 3;
pub(crate) const VT_STR: i32 = 4;
pub(crate) const VT_ARRAY: i32 = 5;
pub(crate) const VT_OBJECT: i32 = 6;

impl Emitter<'_> {
    /// `value.*` module calls. Returns Ok(None) for unhandled names so the
    /// caller can fall through to the qualified table / whitelist.
    pub(crate) fn lower_value_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let out = match (func, args) {
            ("null", []) => {
                self.emit_value_box(VT_NULL, None)?;
                Some(SliceTy::Value)
            }
            ("int", [n]) => {
                self.lower(n, Some(INT))?;
                self.emit_value_box(VT_INT, Some(INT))?;
                Some(SliceTy::Value)
            }
            ("bool", [b]) => {
                self.lower(b, Some(BOOL))?;
                self.f.instructions().i64_extend_i32_u();
                self.emit_value_box(VT_BOOL, Some(INT))?;
                Some(SliceTy::Value)
            }
            ("float", [x]) => {
                self.lower(x, Some(FLOAT))?;
                self.emit_value_box(VT_FLOAT, Some(FLOAT))?;
                Some(SliceTy::Value)
            }
            ("str", [s]) => {
                self.lower(s, Some(STR))?;
                self.emit_value_box(VT_STR, Some(STR))?;
                Some(SliceTy::Value)
            }
            // Object: tag 6, payload = the (String, Value) pairs list —
            // insertion order IS the block, exactly the interp's ordered
            // object model.
            ("object", [pairs]) => {
                let got = self.lower(pairs, None)?;
                let SliceTy::List(h) = got else {
                    return Err(EmitError::Unsupported(format!("value.object-of:{got:?}")));
                };
                let ok = match self.types.el(h) {
                    SliceTy::Tuple(ti) => {
                        let def = self.types.tuple_def(ti);
                        def.fields.len() == 2
                            && def.fields[0].0 == STR
                            && def.fields[1].0 == SliceTy::Value
                    }
                    _ => false,
                };
                if !ok {
                    return Err(EmitError::Unsupported("value.object-el".into()));
                }
                self.emit_value_box(VT_OBJECT, Some(STR))?;
                Some(SliceTy::Value)
            }
            ("array", [xs]) => {
                let got = self.lower(xs, None)?;
                let SliceTy::List(h) = got else {
                    return Err(EmitError::Unsupported(format!("value.array-of:{got:?}")));
                };
                if self.types.el(h) != SliceTy::Value {
                    return Err(EmitError::Unsupported("value.array-el".into()));
                }
                self.emit_value_box(VT_ARRAY, Some(STR))?; // addr slot (i32 class)
                Some(SliceTy::Value)
            }
            ("as_int", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                self.emit_value_unbox(VT_INT, INT, "expected Int")?;
                Some(SliceTy::Result(self.types.intern(INT), self.types.intern(STR)))
            }
            ("as_bool", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                self.emit_value_unbox(VT_BOOL, BOOL, "expected Bool")?;
                Some(SliceTy::Result(self.types.intern(BOOL), self.types.intern(STR)))
            }
            ("as_string", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                self.emit_value_unbox(VT_STR, STR, "expected Str")?;
                Some(SliceTy::Result(self.types.intern(STR), self.types.intern(STR)))
            }
            ("as_array", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                let lv = SliceTy::List(self.types.intern(SliceTy::Value));
                self.emit_value_unbox(VT_ARRAY, lv, "expected Array")?;
                Some(SliceTy::Result(self.types.intern(lv), self.types.intern(STR)))
            }
            // #658: a JSON number has no int/float split — an Int Value
            // widens to a valid Float.
            ("as_float", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                self.emit_value_as_float()?;
                Some(SliceTy::Result(self.types.intern(FLOAT), self.types.intern(STR)))
            }
            // The Codec-derive field accessor: tag check, first-match
            // scan, the incumbent's exact err lines.
            ("field", [v, key]) => {
                self.lower(v, Some(SliceTy::Value))?;
                let hv = self.hold_i32()?;
                self.f.instructions().local_set(hv);
                self.lower(key, Some(STR))?;
                let hk = self.hold_i32()?;
                self.f.instructions().local_set(hk);
                let vf = self.work.helper(Helper::ValueField);
                let hr = self.hold_i32()?;
                self.f.instructions().i32_const(16).call(F_ALLOC).local_set(hr);
                let m_tag = slot_memarg(almide_layout::SUM_TAG);
                let m_pay = slot_memarg(almide_layout::SUM_FIELD);
                let not_obj = self.pool.intern("expected Object");
                let miss_pre = self.pool.intern("missing field '");
                let miss_post = self.pool.intern("'");
                let mut i = self.f.instructions();
                i.local_get(hv).local_get(hk).call(vf).local_set(hv);
                i.local_get(hv).i32_eqz().if_(BlockType::Empty);
                i.local_get(hr).i32_const(1).i32_store(m_tag);
                i.local_get(hr).i32_const(not_obj as i32).i32_store(m_pay);
                i.else_();
                i.local_get(hv).i32_const(1).i32_eq().if_(BlockType::Empty);
                i.local_get(hr).i32_const(1).i32_store(m_tag);
                i.local_get(hr);
                i.i32_const(miss_pre as i32).local_get(hk).call(F_CONCAT);
                i.i32_const(miss_post as i32).call(F_CONCAT);
                i.i32_store(m_pay);
                i.else_();
                i.local_get(hr).i32_const(0).i32_store(m_tag);
                i.local_get(hr).local_get(hv).i32_store(m_pay);
                i.end();
                i.end();
                i.local_get(hr);
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Some(SliceTy::Result(
                    self.types.intern(SliceTy::Value),
                    self.types.intern(STR),
                ))
            }
            ("keys", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                let vk = self.work.helper(Helper::ValueKeys);
                self.f.instructions().call(vk);
                Some(SliceTy::List(self.types.intern(STR)))
            }
            ("stringify", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                self.emit_value_stringify()?;
                Some(STR)
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }

    /// The pooled JSON/display fragment set.
    pub(crate) fn json_frags(&mut self) -> JsonFrags {
        JsonFrags {
            null_: self.pool.intern("null"),
            true_: self.pool.intern("true"),
            false_: self.pool.intern("false"),
            esc_backslash: self.pool.intern("\\\\"),
            esc_quote: self.pool.intern("\\\""),
            esc_n: self.pool.intern("\\n"),
            esc_r: self.pool.intern("\\r"),
            esc_t: self.pool.intern("\\t"),
            quote: self.pool.intern("\""),
            comma: self.pool.intern(","),
            colon: self.pool.intern(":"),
            lbrack: self.pool.intern("["),
            rbrack: self.pool.intern("]"),
            lbrace: self.pool.intern("{"),
            rbrace: self.pool.intern("}"),
        }
    }

    /// `[value]` -> `[String]`: run the JSON serializer helpers over the
    /// line buffer and capture the region as a real block.
    pub(crate) fn emit_value_stringify(&mut self) -> Result<(), EmitError> {
        let Some(fi) = self.resolve_qualified("float.to_string") else {
            return Err(EmitError::Unsupported("stringify:float-unlinked".into()));
        };
        let info = &self.table.infos[fi];
        if info.refuse.is_some() || info.ret != Some(STR) {
            return Err(EmitError::Unsupported("stringify:float-impl".into()));
        }
        let float_idx = info.wasm_index;
        self.calls.insert(fi);
        let frags = self.json_frags();
        let _ = self.work.helper(Helper::JsonQuote { frags });
        let vj = self.work.helper(Helper::JsonValue { float_to_string: float_idx, frags });
        let hv = self.hold_i32()?;
        let start = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.f.instructions().global_get(G_LINE_CURSOR).local_set(start);
        self.f
            .instructions()
            .local_get(start)
            .local_get(hv)
            .call(vj)
            .local_set(self.tmp_i32_local);
        self.f
            .instructions()
            .local_get(start)
            .local_get(self.tmp_i32_local)
            .call(F_BUF_TO_BLOCK);
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    /// `[payload?]` -> `[value block]`: alloc, tag, store the 8-byte slot.
    /// `payload_kind` picks the store width (None = tag-only Null).
    fn emit_value_box(
        &mut self,
        tag: i32,
        payload_kind: Option<SliceTy>,
    ) -> Result<(), EmitError> {
        let hv = payload_kind.map(|k| self.hold_val(k)).transpose()?;
        let hb = self.hold_i32()?;
        if let (Some(h), Some(_)) = (hv, payload_kind) {
            self.f.instructions().local_set(h);
        }
        self.f
            .instructions()
            .i32_const(16)
            .call(F_ALLOC)
            .local_tee(hb)
            .i32_const(tag)
            .i32_store(slot_memarg(almide_layout::SUM_TAG));
        if let (Some(h), Some(k)) = (hv, payload_kind) {
            self.f.instructions().local_get(hb).local_get(h);
            self.store_ty_slot(k, almide_layout::SUM_FIELD);
        }
        self.f.instructions().local_get(hb);
        self.release_i32();
        if let Some(k) = payload_kind {
            self.release_val(k);
        }
        Ok(())
    }

    /// `[value block]` -> `[Result block]`: tag match yields ok(payload),
    /// anything else the exact incumbent err line.
    fn emit_value_unbox(
        &mut self,
        want_tag: i32,
        payload: SliceTy,
        err_msg: &str,
    ) -> Result<(), EmitError> {
        let msg = self.pool.intern(err_msg);
        let hv = self.hold_i32()?;
        let hr = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.f
            .instructions()
            .i32_const(16)
            .call(F_ALLOC)
            .local_set(hr);
        let mut i = self.f.instructions();
        i.local_get(hv)
            .i32_load(slot_memarg(almide_layout::SUM_TAG))
            .i32_const(want_tag)
            .i32_eq()
            .if_(BlockType::Empty);
        i.local_get(hr).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hr).local_get(hv);
        let _ = i;
        self.load_ty_slot(payload, almide_layout::SUM_FIELD);
        self.store_ty_slot(payload, almide_layout::SUM_FIELD);
        let mut i = self.f.instructions();
        i.else_();
        i.local_get(hr).i32_const(1).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hr)
            .i32_const(msg as i32)
            .i32_store(slot_memarg(almide_layout::SUM_FIELD));
        i.end();
        i.local_get(hr);
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    /// `as_float` with the #658 widening: Float passes through, Int
    /// converts, anything else errs "expected Float".
    fn emit_value_as_float(&mut self) -> Result<(), EmitError> {
        let msg = self.pool.intern("expected Float");
        let hv = self.hold_i32()?;
        let hr = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.f.instructions().i32_const(16).call(F_ALLOC).local_set(hr);
        let m_tag = slot_memarg(almide_layout::SUM_TAG);
        let m_pay = slot_memarg(almide_layout::SUM_FIELD);
        let mut i = self.f.instructions();
        i.local_get(hv).i32_load(m_tag).i32_const(VT_FLOAT).i32_eq();
        i.if_(BlockType::Empty);
        i.local_get(hr).i32_const(0).i32_store(m_tag);
        i.local_get(hr).local_get(hv).f64_load(m_pay).f64_store(m_pay);
        i.else_();
        i.local_get(hv).i32_load(m_tag).i32_const(VT_INT).i32_eq();
        i.if_(BlockType::Empty);
        i.local_get(hr).i32_const(0).i32_store(m_tag);
        i.local_get(hr).local_get(hv).i64_load(m_pay).f64_convert_i64_s().f64_store(m_pay);
        i.else_();
        i.local_get(hr).i32_const(1).i32_store(m_tag);
        i.local_get(hr).i32_const(msg as i32).i32_store(m_pay);
        i.end();
        i.end();
        i.local_get(hr);
        self.release_i32();
        self.release_i32();
        Ok(())
    }
}
