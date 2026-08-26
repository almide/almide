//! String runtime helpers ($str_cmp / $str_replace) — split from
//! runtime.rs for the complexity budget.

use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::*;

/// `$str_cmp(a: i32, b: i32) -> i32`: -1/0/1 by BYTE lexicographic order
/// with length tiebreak — exactly Rust's `String: Ord` (what native
/// `list.sort` on `List[String]` compares by).
pub(crate) fn emit_str_cmp() -> Function {
    // params: 0=a, 1=b; locals: 2=p, 3=n, 4=la, 5=lb, 6=ca, 7=cb
    let (a, b, p, n, la, lb, ca, cb) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32);
    let payload = almide_layout::PAYLOAD as i32;
    let byte = |off: u32| MemArg { offset: u64::from(off), align: 0, memory_index: 0 };
    let mut f = Function::new([(6, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(a).i32_load(len_memarg()).local_set(la);
    i.local_get(b).i32_load(len_memarg()).local_set(lb);
    i.local_get(la).local_get(lb).local_get(la).local_get(lb).i32_lt_u().select().local_set(n);
    i.i32_const(0).local_set(p);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(n).i32_ge_u().br_if(1);
    i.local_get(a).i32_const(payload).i32_add().local_get(p).i32_add();
    i.i32_load8_u(byte(0)).local_set(ca);
    i.local_get(b).i32_const(payload).i32_add().local_get(p).i32_add();
    i.i32_load8_u(byte(0)).local_set(cb);
    i.local_get(ca).local_get(cb).i32_ne().if_(BlockType::Empty);
    i.i32_const(-1).i32_const(1).local_get(ca).local_get(cb).i32_lt_u().select().return_();
    i.end();
    i.local_get(p).i32_const(1).i32_add().local_set(p);
    i.br(0).end().end();
    // equal prefix: shorter sorts first
    i.local_get(la).local_get(lb).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.i32_const(-1).i32_const(1).local_get(la).local_get(lb).i32_lt_u().select();
    i.end();
    f
}

/// `$str_replace(s, from, to, first) -> i32`: Rust `str::replace` /
/// `replace_first` byte-for-byte. An EMPTY `from` inserts `to` at every
/// CHAR boundary (leading `to`, then each UTF-8 char ++ `to` — C-100);
/// `replace_first` with an empty `from` is `to ++ s` (find("") = 0).
pub(crate) fn emit_str_replace() -> Function {
    // params: 0=s, 1=from, 2=to, 3=first
    // locals: 4=slen, 5=flen, 6=tlen, 7=count, 8=p, 9=w, 10=out, 11=q, 12=eq
    let (s, from, to, first) = (0u32, 1u32, 2u32, 3u32);
    let (slen, flen, tlen, count, p, w, out, q, eq) =
        (4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32);
    let payload = almide_layout::PAYLOAD as i32;
    let byte = |off: u32| MemArg { offset: u64::from(off), align: 0, memory_index: 0 };
    let mut f = Function::new([(9, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(s).i32_load(len_memarg()).local_set(slen);
    i.local_get(from).i32_load(len_memarg()).local_set(flen);
    i.local_get(to).i32_load(len_memarg()).local_set(tlen);
    i.local_get(flen).i32_eqz().if_(BlockType::Empty);
    {
        // empty pattern
        i.local_get(first).if_(BlockType::Empty);
        i.local_get(to).local_get(s).call(F_CONCAT).return_();
        i.end();
        // chars = count of non-continuation bytes
        i.i32_const(0).local_set(count);
        i.i32_const(0).local_set(p);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(p).local_get(slen).i32_ge_u().br_if(1);
        i.local_get(s).i32_const(payload).i32_add().local_get(p).i32_add();
        i.i32_load8_u(byte(0)).i32_const(0xC0).i32_and().i32_const(0x80).i32_ne();
        i.if_(BlockType::Empty);
        i.local_get(count).i32_const(1).i32_add().local_set(count);
        i.end();
        i.local_get(p).i32_const(1).i32_add().local_set(p);
        i.br(0).end().end();
        // out = alloc(slen + (chars+1)*tlen); w walks its payload
        i.local_get(slen);
        i.local_get(count).i32_const(1).i32_add().local_get(tlen).i32_mul();
        i.i32_add().call(F_ALLOC).local_set(out);
        i.local_get(out).i32_const(payload).i32_add().local_set(w);
        i.local_get(w).local_get(to).i32_const(payload).i32_add().local_get(tlen);
        i.memory_copy(0, 0);
        i.local_get(w).local_get(tlen).i32_add().local_set(w);
        i.i32_const(0).local_set(p);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(p).local_get(slen).i32_ge_u().br_if(1);
        i.local_get(w);
        i.local_get(s).i32_const(payload).i32_add().local_get(p).i32_add();
        i.i32_load8_u(byte(0)).i32_store8(byte(0));
        i.local_get(w).i32_const(1).i32_add().local_set(w);
        i.local_get(p).i32_const(1).i32_add().local_set(p);
        // char boundary AFTER this byte → append `to`
        i.local_get(p).local_get(slen).i32_ge_u();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(1);
        i.else_();
        i.local_get(s).i32_const(payload).i32_add().local_get(p).i32_add();
        i.i32_load8_u(byte(0)).i32_const(0xC0).i32_and().i32_const(0x80).i32_ne();
        i.end();
        i.if_(BlockType::Empty);
        i.local_get(w).local_get(to).i32_const(payload).i32_add().local_get(tlen);
        i.memory_copy(0, 0);
        i.local_get(w).local_get(tlen).i32_add().local_set(w);
        i.end();
        i.br(0).end().end();
        i.local_get(out).return_();
    }
    i.end();
    // non-empty pattern: count pass, then fill pass
    i.i32_const(0).local_set(count);
    i.i32_const(0).local_set(p);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(flen).i32_add().local_get(slen).i32_gt_u().br_if(1);
    // eq = memeq(s+p, from, flen)
    i.i32_const(1).local_set(eq);
    i.i32_const(0).local_set(q);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(q).local_get(flen).i32_ge_u().br_if(1);
    i.local_get(s).i32_const(payload).i32_add().local_get(p).i32_add().local_get(q).i32_add();
    i.i32_load8_u(byte(0));
    i.local_get(from).i32_const(payload).i32_add().local_get(q).i32_add();
    i.i32_load8_u(byte(0));
    i.i32_ne().if_(BlockType::Empty);
    i.i32_const(0).local_set(eq);
    i.br(2);
    i.end();
    i.local_get(q).i32_const(1).i32_add().local_set(q);
    i.br(0).end().end();
    i.local_get(eq).if_(BlockType::Empty);
    i.local_get(count).i32_const(1).i32_add().local_set(count);
    i.local_get(p).local_get(flen).i32_add().local_set(p);
    i.local_get(first).br_if(2);
    i.else_();
    i.local_get(p).i32_const(1).i32_add().local_set(p);
    i.end();
    i.br(0).end().end();
    // out_len = slen + count*(tlen - flen)
    i.local_get(slen);
    i.local_get(count).local_get(tlen).local_get(flen).i32_sub().i32_mul();
    i.i32_add().call(F_ALLOC).local_set(out);
    i.local_get(out).i32_const(payload).i32_add().local_set(w);
    i.i32_const(0).local_set(p);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(slen).i32_ge_u().br_if(1);
    // try a match here while replacements remain
    i.i32_const(0).local_set(eq);
    i.local_get(count).i32_const(0).i32_gt_s();
    i.local_get(p).local_get(flen).i32_add().local_get(slen).i32_le_u();
    i.i32_and().if_(BlockType::Empty);
    i.i32_const(1).local_set(eq);
    i.i32_const(0).local_set(q);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(q).local_get(flen).i32_ge_u().br_if(1);
    i.local_get(s).i32_const(payload).i32_add().local_get(p).i32_add().local_get(q).i32_add();
    i.i32_load8_u(byte(0));
    i.local_get(from).i32_const(payload).i32_add().local_get(q).i32_add();
    i.i32_load8_u(byte(0));
    i.i32_ne().if_(BlockType::Empty);
    i.i32_const(0).local_set(eq);
    i.br(2);
    i.end();
    i.local_get(q).i32_const(1).i32_add().local_set(q);
    i.br(0).end().end();
    i.end();
    i.local_get(eq).if_(BlockType::Empty);
    i.local_get(w).local_get(to).i32_const(payload).i32_add().local_get(tlen);
    i.memory_copy(0, 0);
    i.local_get(w).local_get(tlen).i32_add().local_set(w);
    i.local_get(p).local_get(flen).i32_add().local_set(p);
    i.local_get(count).i32_const(1).i32_sub().local_set(count);
    i.else_();
    i.local_get(w);
    i.local_get(s).i32_const(payload).i32_add().local_get(p).i32_add();
    i.i32_load8_u(byte(0)).i32_store8(byte(0));
    i.local_get(w).i32_const(1).i32_add().local_set(w);
    i.local_get(p).i32_const(1).i32_add().local_set(p);
    i.end();
    i.br(0).end().end();
    i.local_get(out);
    i.end();
    f
}
