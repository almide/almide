//! json.set_path / json.remove_path — runtime-recursive builders over
//! THIS backend's Value layout (tag @SUM_TAG, payload @SUM_FIELD; an
//! Object's payload is a (String, Value)-pair LIST, pairs are 8-byte
//! tuple blocks key@0/val@4). Semantics verbatim from
//! stdlib/json_path.almd (which reads the INCUMBENT layout raw and is
//! therefore unlinkable here): a path is a List[String] of segments —
//! "f<name>" a field step, "i<int>" an index step; set upserts (a
//! missing field APPENDS a freshly-built chain of empty objects), a
//! non-object/array node passes through (index) or is REPLACED by the
//! object chain (field); remove of the LAST segment drops the entry,
//! deeper segments recurse; a miss passes the node through unchanged.

use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::work::Helper;
use crate::*;

fn raw8() -> MemArg {
    MemArg { offset: 0, align: 0, memory_index: 0 }
}

const VT_OBJECT: i32 = 6;
const VT_ARRAY: i32 = 5;

/// Shared per-segment prologue: seg = path[k]; rest = fresh block of
/// seg[1..]; idx = atoi(rest) with '-' (any non-digit → 0, the
/// `int.parse(rest) ?? 0` mirror). Locals are caller-chosen.
#[allow(clippy::too_many_arguments)]
fn seg_prologue(
    i: &mut wasm_encoder::InstructionSink,
    path: u32,
    k: u32,
    seg: u32,
    rest: u32,
    idx: u32,
    scr: u32,
    scr2: u32,
) {
    // seg = path[k]
    i.local_get(path).local_get(k).i32_const(4).i32_mul().i32_add();
    i.i32_load(slot_memarg(0)).local_set(seg);
    // rest = fresh(seg[1..])
    i.local_get(seg).i32_load(len_memarg()).i32_const(1).i32_sub().call(F_ALLOC);
    i.local_set(rest);
    i.local_get(rest).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(seg).i32_const(almide_layout::PAYLOAD as i32).i32_add().i32_const(1).i32_add();
    i.local_get(rest).i32_load(len_memarg());
    i.memory_copy(0, 0);
    // idx = atoi(rest): scr = cursor, scr2 = sign flag; `seg`'s job is
    // done, so it doubles as the digit temp. Any non-digit → 0 (the
    // `int.parse(rest) ?? 0` mirror).
    i.i32_const(0).local_set(idx);
    i.i32_const(0).local_set(scr);
    i.i32_const(0).local_set(scr2);
    i.local_get(rest).i32_load(len_memarg()).i32_eqz().i32_eqz().if_(BlockType::Empty);
    i.local_get(rest)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .i32_load8_u(raw8())
        .i32_const(45)
        .i32_eq()
        .if_(BlockType::Empty);
    i.i32_const(1).local_set(scr2);
    i.i32_const(1).local_set(scr);
    i.end();
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(scr).local_get(rest).i32_load(len_memarg()).i32_ge_u().br_if(1);
    i.local_get(rest)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .local_get(scr)
        .i32_add()
        .i32_load8_u(raw8())
        .i32_const(48)
        .i32_sub()
        .local_set(seg);
    i.local_get(seg).i32_const(0).i32_lt_s();
    i.local_get(seg).i32_const(9).i32_gt_s();
    i.i32_or().if_(BlockType::Empty);
    i.i32_const(0).local_set(idx);
    i.br(2);
    i.end();
    i.local_get(idx).i32_const(10).i32_mul().local_get(seg).i32_add().local_set(idx);
    i.local_get(scr).i32_const(1).i32_add().local_set(scr);
    i.br(0).end().end();
    i.local_get(scr2).if_(BlockType::Empty);
    i.i32_const(0).local_get(idx).i32_sub().local_set(idx);
    i.end();
    i.end();
}

/// A fresh empty-Object Value block.
fn empty_obj(i: &mut wasm_encoder::InstructionSink, tmp: u32) {
    i.i32_const(16).call(F_ALLOC).local_set(tmp);
    i.local_get(tmp).i32_const(VT_OBJECT).i32_store(slot_memarg(almide_layout::SUM_TAG));
    i.local_get(tmp);
    i.i32_const(0).call(F_ALLOC);
    i.i32_store(slot_memarg(almide_layout::SUM_FIELD));
    i.local_get(tmp);
}

/// Box a pairs/items list as an Object/Array Value.
fn box_value(i: &mut wasm_encoder::InstructionSink, tag: i32, list_local: u32, tmp: u32) {
    i.i32_const(16).call(F_ALLOC).local_set(tmp);
    i.local_get(tmp).i32_const(tag).i32_store(slot_memarg(almide_layout::SUM_TAG));
    i.local_get(tmp).local_get(list_local).i32_store(slot_memarg(almide_layout::SUM_FIELD));
    i.local_get(tmp);
}

/// `$jp_set(j, path, k, nv) -> Value` — see the module doc.
pub(crate) fn emit_json_path_set_helper(helper_base: u32, helpers: &[Helper]) -> Function {
    let self_idx = helper_base
        + helpers
            .iter()
            .position(|h| matches!(h, Helper::JsonPathSet))
            .expect("registered") as u32;
    let (j, path, k, nv) = (0u32, 1u32, 2u32, 3u32);
    let (seg, rest, idx, scr, scr2, pairs, n, cur, out, w, keyl, has, tmp) =
        (4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32, 13u32, 14u32, 15u32, 16u32);
    let m_tag = slot_memarg(almide_layout::SUM_TAG);
    let m_pay = slot_memarg(almide_layout::SUM_FIELD);
    let mut f = Function::new([(13, ValType::I32)]);
    let mut i = f.instructions();
    // k >= path count → nv
    i.local_get(k);
    i.local_get(path).i32_load(len_memarg()).i32_const(2).i32_shr_u();
    i.i32_ge_u().if_(BlockType::Empty);
    i.local_get(nv).return_();
    i.end();
    seg_prologue(&mut i, path, k, seg, rest, idx, scr, scr2);
    // field step?
    i.local_get(path)
        .local_get(k)
        .i32_const(4)
        .i32_mul()
        .i32_add()
        .i32_load(slot_memarg(0))
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .i32_load8_u(raw8())
        .i32_const(102) // 'f'
        .i32_eq()
        .if_(BlockType::Empty);
    i.local_get(j).i32_load(m_tag).i32_const(VT_OBJECT).i32_eq().if_(BlockType::Empty);
    // object: rebuild pairs, upserting `rest`
    i.local_get(j).i32_load(m_pay).local_set(pairs);
    i.local_get(pairs).i32_load(len_memarg()).local_set(n);
    // has?
    i.i32_const(0).local_set(has);
    i.i32_const(0).local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).local_get(n).i32_ge_u().br_if(1);
    i.local_get(pairs).local_get(cur).i32_add().i32_load(slot_memarg(0));
    i.i32_load(slot_memarg(0));
    i.local_get(rest).call(F_STR_EQ).if_(BlockType::Empty);
    i.i32_const(1).local_set(has);
    i.br(2);
    i.end();
    i.local_get(cur).i32_const(4).i32_add().local_set(cur);
    i.br(0).end().end();
    // out = alloc(n + (has ? 0 : 4))
    i.local_get(n);
    i.local_get(has).i32_eqz().if_(BlockType::Result(ValType::I32));
    i.i32_const(4);
    i.else_();
    i.i32_const(0);
    i.end();
    i.i32_add().call(F_ALLOC).local_set(out);
    i.i32_const(0).local_set(w);
    i.i32_const(0).local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).local_get(n).i32_ge_u().br_if(1);
    i.local_get(pairs).local_get(cur).i32_add().i32_load(slot_memarg(0)).local_set(tmp);
    i.local_get(tmp).i32_load(slot_memarg(0)).local_set(keyl);
    i.local_get(keyl).local_get(rest).call(F_STR_EQ).if_(BlockType::Empty);
    // fresh pair: (key, self(old_val, path, k+1, nv))
    i.local_get(tmp).i32_load(slot_memarg(4)).local_set(scr);
    i.i32_const(8).call(F_ALLOC).local_set(tmp);
    i.local_get(tmp).local_get(keyl).i32_store(slot_memarg(0));
    i.local_get(tmp);
    i.local_get(scr).local_get(path).local_get(k).i32_const(1).i32_add().local_get(nv);
    i.call(self_idx);
    i.i32_store(slot_memarg(4));
    i.end();
    i.local_get(out).local_get(w).i32_add().local_get(tmp).i32_store(slot_memarg(0));
    i.local_get(w).i32_const(4).i32_add().local_set(w);
    i.local_get(cur).i32_const(4).i32_add().local_set(cur);
    i.br(0).end().end();
    i.local_get(has).i32_eqz().if_(BlockType::Empty);
    // append (rest, self(empty_obj, path, k+1, nv))
    i.i32_const(8).call(F_ALLOC).local_set(tmp);
    i.local_get(tmp).local_get(rest).i32_store(slot_memarg(0));
    i.local_get(tmp);
    let _ = i;
    let mut i = f.instructions();
    empty_obj(&mut i, scr);
    i.local_set(scr);
    i.local_get(scr).local_get(path).local_get(k).i32_const(1).i32_add().local_get(nv);
    i.call(self_idx);
    i.i32_store(slot_memarg(4));
    i.local_get(out).local_get(w).i32_add().local_get(tmp).i32_store(slot_memarg(0));
    i.local_get(w).i32_const(4).i32_add().local_set(w);
    i.end();
    box_value(&mut i, VT_OBJECT, out, tmp);
    i.return_();
    i.else_();
    // non-object under a field step: object([(rest, chain)])
    i.i32_const(8).call(F_ALLOC).local_set(tmp);
    i.local_get(tmp).local_get(rest).i32_store(slot_memarg(0));
    i.local_get(tmp);
    let _ = i;
    let mut i = f.instructions();
    empty_obj(&mut i, scr);
    i.local_set(scr);
    i.local_get(scr).local_get(path).local_get(k).i32_const(1).i32_add().local_get(nv);
    i.call(self_idx);
    i.i32_store(slot_memarg(4));
    i.i32_const(4).call(F_ALLOC).local_set(out);
    i.local_get(out).local_get(tmp).i32_store(slot_memarg(0));
    box_value(&mut i, VT_OBJECT, out, tmp);
    i.return_();
    i.end();
    i.end();
    // index step
    i.local_get(j).i32_load(m_tag).i32_const(VT_ARRAY).i32_eq().if_(BlockType::Empty);
    i.local_get(j).i32_load(m_pay).local_set(pairs);
    i.local_get(pairs).i32_load(len_memarg()).i32_const(2).i32_shr_u().local_set(n);
    // idx = idx < 0 ? n + idx : idx
    i.local_get(idx).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.local_get(n).local_get(idx).i32_add().local_set(idx);
    i.end();
    i.local_get(idx).i32_const(0).i32_ge_s();
    i.local_get(idx).local_get(n).i32_lt_s();
    i.i32_and().if_(BlockType::Empty);
    // rebuild items with [idx] recursed
    i.local_get(pairs).i32_load(len_memarg()).call(F_ALLOC).local_set(out);
    i.local_get(out).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(pairs).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(pairs).i32_load(len_memarg());
    i.memory_copy(0, 0);
    i.local_get(out).local_get(idx).i32_const(4).i32_mul().i32_add();
    i.local_get(pairs).local_get(idx).i32_const(4).i32_mul().i32_add().i32_load(slot_memarg(0));
    i.local_get(path).local_get(k).i32_const(1).i32_add().local_get(nv);
    i.call(self_idx);
    i.i32_store(slot_memarg(0));
    box_value(&mut i, VT_ARRAY, out, tmp);
    i.return_();
    i.end();
    i.end();
    // miss / non-container: pass through
    i.local_get(j);
    i.end();
    f
}

/// `$jp_remove(j, path, k) -> Value` — see the module doc.
pub(crate) fn emit_json_path_remove_helper(helper_base: u32, helpers: &[Helper]) -> Function {
    let self_idx = helper_base
        + helpers
            .iter()
            .position(|h| matches!(h, Helper::JsonPathRemove))
            .expect("registered") as u32;
    let (j, path, k) = (0u32, 1u32, 2u32);
    let (seg, rest, idx, scr, scr2, pairs, n, cur, out, w, keyl, last, tmp) =
        (3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32, 13u32, 14u32, 15u32);
    let m_tag = slot_memarg(almide_layout::SUM_TAG);
    let m_pay = slot_memarg(almide_layout::SUM_FIELD);
    let mut f = Function::new([(13, ValType::I32)]);
    let mut i = f.instructions();
    // k >= count → value.null()
    i.local_get(k);
    i.local_get(path).i32_load(len_memarg()).i32_const(2).i32_shr_u();
    i.i32_ge_u().if_(BlockType::Empty);
    i.i32_const(16).call(F_ALLOC).return_();
    i.end();
    // last = k + 1 >= count
    i.local_get(k).i32_const(1).i32_add();
    i.local_get(path).i32_load(len_memarg()).i32_const(2).i32_shr_u();
    i.i32_ge_u().local_set(last);
    seg_prologue(&mut i, path, k, seg, rest, idx, scr, scr2);
    // field step?
    i.local_get(path)
        .local_get(k)
        .i32_const(4)
        .i32_mul()
        .i32_add()
        .i32_load(slot_memarg(0))
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .i32_load8_u(raw8())
        .i32_const(102)
        .i32_eq()
        .if_(BlockType::Empty);
    i.local_get(j).i32_load(m_tag).i32_const(VT_OBJECT).i32_eq().if_(BlockType::Empty);
    i.local_get(j).i32_load(m_pay).local_set(pairs);
    i.local_get(pairs).i32_load(len_memarg()).local_set(n);
    i.local_get(n).call(F_ALLOC).local_set(out);
    i.i32_const(0).local_set(w);
    i.i32_const(0).local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).local_get(n).i32_ge_u().br_if(1);
    i.local_get(pairs).local_get(cur).i32_add().i32_load(slot_memarg(0)).local_set(tmp);
    i.local_get(tmp).i32_load(slot_memarg(0)).local_set(keyl);
    i.local_get(keyl).local_get(rest).call(F_STR_EQ).if_(BlockType::Empty);
    i.local_get(last).if_(BlockType::Empty);
    // drop the entry: continue the walk (br to the LOOP head)
    i.local_get(cur).i32_const(4).i32_add().local_set(cur);
    i.br(2);
    i.end();
    // (key, self(val, path, k+1))
    i.local_get(tmp).i32_load(slot_memarg(4)).local_set(scr);
    i.i32_const(8).call(F_ALLOC).local_set(tmp);
    i.local_get(tmp).local_get(keyl).i32_store(slot_memarg(0));
    i.local_get(tmp);
    i.local_get(scr).local_get(path).local_get(k).i32_const(1).i32_add();
    i.call(self_idx);
    i.i32_store(slot_memarg(4));
    i.end();
    i.local_get(out).local_get(w).i32_add().local_get(tmp).i32_store(slot_memarg(0));
    i.local_get(w).i32_const(4).i32_add().local_set(w);
    i.local_get(cur).i32_const(4).i32_add().local_set(cur);
    i.br(0).end().end();
    i.local_get(out).local_get(w).i32_store(len_memarg());
    box_value(&mut i, VT_OBJECT, out, tmp);
    i.return_();
    i.end();
    i.local_get(j).return_();
    i.end();
    // index step
    i.local_get(j).i32_load(m_tag).i32_const(VT_ARRAY).i32_eq().if_(BlockType::Empty);
    i.local_get(j).i32_load(m_pay).local_set(pairs);
    i.local_get(pairs).i32_load(len_memarg()).i32_const(2).i32_shr_u().local_set(n);
    i.local_get(idx).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.local_get(n).local_get(idx).i32_add().local_set(idx);
    i.end();
    i.local_get(idx).i32_const(0).i32_ge_s();
    i.local_get(idx).local_get(n).i32_lt_s();
    i.i32_and().if_(BlockType::Empty);
    i.local_get(pairs).i32_load(len_memarg()).call(F_ALLOC).local_set(out);
    i.i32_const(0).local_set(w);
    i.i32_const(0).local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).local_get(n).i32_ge_u().br_if(1);
    i.local_get(cur).local_get(idx).i32_eq().if_(BlockType::Empty);
    i.local_get(last).if_(BlockType::Empty);
    i.local_get(cur).i32_const(1).i32_add().local_set(cur);
    i.br(2);
    i.end();
    i.local_get(out).local_get(w).i32_add();
    i.local_get(pairs).local_get(cur).i32_const(4).i32_mul().i32_add().i32_load(slot_memarg(0));
    i.local_get(path).local_get(k).i32_const(1).i32_add();
    i.call(self_idx);
    i.i32_store(slot_memarg(0));
    i.else_();
    i.local_get(out).local_get(w).i32_add();
    i.local_get(pairs).local_get(cur).i32_const(4).i32_mul().i32_add().i32_load(slot_memarg(0));
    i.i32_store(slot_memarg(0));
    i.end();
    i.local_get(w).i32_const(4).i32_add().local_set(w);
    i.local_get(cur).i32_const(1).i32_add().local_set(cur);
    i.br(0).end().end();
    i.local_get(out).local_get(w).i32_store(len_memarg());
    box_value(&mut i, VT_ARRAY, out, tmp);
    i.return_();
    i.end();
    i.end();
    i.local_get(j);
    i.end();
    f
}
