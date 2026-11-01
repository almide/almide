//! `String::from_utf8_lossy` as emitted wasm, for the shims that hand host
//! bytes to the guest as Strings. A String is valid UTF-8 (the owner's
//! trusted invariant, gated by the producer ledger), so every shim producer
//! that copies host bytes either validates them (`bad`) or decodes them
//! lossily, the way native decodes the same bytes (#4390, #4391).

use wasm_encoder::{BlockType, InstructionSink};

use crate::mem8;

/// The walk's scratch locals.
#[derive(Clone, Copy)]
pub struct LossyLocals {
    pub k: u32,
    pub b0: u32,
    pub extra: u32,
    pub lo: u32,
    pub hi: u32,
    pub vlen: u32,
    pub ok: u32,
    pub j: u32,
    pub c: u32,
    pub chars: u32,
}

/// Decode `[ptr, ptr + len)` as `String::from_utf8_lossy` does — the
/// Table 3-7 well-formed sequences kept, one U+FFFD per maximal invalid
/// subpart — leaving the decoded text's chars in `l.chars` and adding its
/// byte length to `bytes`. With `dst = Some((buf, pos))` the decoded bytes
/// are also written at `buf + pos`, advancing `pos`. At most 3 bytes out
/// per byte in. With `bad = Some(local)`, that local is set to 1 when any
/// U+FFFD was produced — the bytes were not UTF-8 — and left alone otherwise.
pub fn emit_lossy(i: &mut InstructionSink<'_>, (ptr, len): (u32, u32), bytes: u32, dst: Option<(u32, u32)>, bad: Option<u32>, l: LossyLocals) {
    // One decoded unit of `n` bytes: count it, and write it when writing.
    let unit = |i: &mut InstructionSink<'_>, fffd: bool, n: u32| {
        if fffd {
            if let Some(b) = bad {
                i.i32_const(1).local_set(b);
            }
        }
        i.local_get(l.chars).i32_const(1).i32_add().local_set(l.chars);
        if fffd {
            i.local_get(bytes).i32_const(3).i32_add().local_set(bytes);
        } else {
            i.local_get(bytes).local_get(n).i32_add().local_set(bytes);
        }
        if let Some((buf, pos)) = dst {
            if fffd {
                for (off, b) in [(0u64, 0xEF), (1, 0xBF), (2, 0xBD)] {
                    i.local_get(buf).local_get(pos).i32_add().i32_const(b).i32_store8(mem8(off));
                }
                i.local_get(pos).i32_const(3).i32_add().local_set(pos);
            } else {
                i.local_get(buf).local_get(pos).i32_add();
                i.local_get(ptr).local_get(l.k).i32_add();
                i.local_get(n).memory_copy(0, 0);
                i.local_get(pos).local_get(n).i32_add().local_set(pos);
            }
        }
    };
    i.i32_const(0).local_set(l.chars);
    i.i32_const(0).local_set(l.k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.k).local_get(len).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(l.k).i32_add().i32_load8_u(mem8(0)).local_set(l.b0);
    // vlen = the bytes this unit consumes (lead included).
    i.i32_const(1).local_set(l.vlen);
    i.local_get(l.b0).i32_const(0x80).i32_lt_u().if_(BlockType::Empty);
    unit(i, false, l.vlen);
    i.else_();
    i.i32_const(0).local_set(l.extra);
    i.i32_const(0x80).local_set(l.lo);
    i.i32_const(0xBF).local_set(l.hi);
    // (first lead, last lead, continuation count, second-byte lo, hi)
    for (a, b, extra, lo, hi) in [
        (0xC2, 0xDF, 1, 0x80, 0xBF),
        (0xE0, 0xE0, 2, 0xA0, 0xBF),
        (0xE1, 0xEC, 2, 0x80, 0xBF),
        (0xED, 0xED, 2, 0x80, 0x9F),
        (0xEE, 0xEF, 2, 0x80, 0xBF),
        (0xF0, 0xF0, 3, 0x90, 0xBF),
        (0xF1, 0xF3, 3, 0x80, 0xBF),
        (0xF4, 0xF4, 3, 0x80, 0x8F),
    ] {
        i.local_get(l.b0).i32_const(a).i32_ge_u().local_get(l.b0).i32_const(b).i32_le_u().i32_and();
        i.if_(BlockType::Empty);
        i.i32_const(extra).local_set(l.extra);
        i.i32_const(lo).local_set(l.lo);
        i.i32_const(hi).local_set(l.hi);
        i.end();
    }
    i.local_get(l.extra).i32_eqz().if_(BlockType::Empty);
    unit(i, true, l.vlen);
    i.else_();
    // Walk the continuations j = 1..=extra; vlen counts the valid ones.
    i.i32_const(1).local_set(l.ok);
    i.i32_const(1).local_set(l.j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.j).local_get(l.extra).i32_gt_u().br_if(1);
    i.local_get(l.k).local_get(l.j).i32_add().local_get(len).i32_ge_u().if_(BlockType::Empty);
    i.i32_const(0).local_set(l.ok);
    i.br(2);
    i.end();
    i.local_get(ptr).local_get(l.k).i32_add().local_get(l.j).i32_add().i32_load8_u(mem8(0)).local_set(l.c);
    i.local_get(l.c);
    i.local_get(l.lo).i32_const(0x80).local_get(l.j).i32_const(1).i32_eq().select();
    i.i32_lt_u();
    i.local_get(l.c);
    i.local_get(l.hi).i32_const(0xBF).local_get(l.j).i32_const(1).i32_eq().select();
    i.i32_gt_u();
    i.i32_or().if_(BlockType::Empty);
    i.i32_const(0).local_set(l.ok);
    i.br(2);
    i.end();
    i.local_get(l.vlen).i32_const(1).i32_add().local_set(l.vlen);
    i.local_get(l.j).i32_const(1).i32_add().local_set(l.j);
    i.br(0).end().end();
    i.local_get(l.ok).if_(BlockType::Empty);
    unit(i, false, l.vlen);
    i.else_();
    unit(i, true, l.vlen);
    i.end();
    i.end();
    i.end();
    i.local_get(l.k).local_get(l.vlen).i32_add().local_set(l.k);
    i.br(0).end().end();
}
