//! String stdlib WASM runtime: replace/search variants and predicates.
//!
//! Split from rt_string.rs for file size. These are all standalone compile_* functions
//! called from `compile()` in rt_string.rs.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::{Function, ValType};

// ── Replace/search variants ──

pub(super) fn compile_replace_first(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.replace_first];
    let mut f = Function::new([
        (1, ValType::I64), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(0); local_get(1); call(emitter.rt.string.index_of); local_set(3);
        local_get(3); i64_const(-1); i64_eq;
        if_i32; local_get(0);
        else_;
          local_get(3); i32_wrap_i64; local_set(4);
          local_get(1); i32_load(0); local_set(5);
          local_get(0); i32_const(0); local_get(4);
          call(emitter.rt.string.slice); local_set(6);
          local_get(0); local_get(4); local_get(5); i32_add; local_get(0); i32_load(0);
          call(emitter.rt.string.slice); local_set(7);
          local_get(6); local_get(2); call(emitter.rt.concat_str);
          local_get(7); call(emitter.rt.concat_str);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

pub(super) fn compile_last_index_of(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.last_index_of];
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I64),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_load(0); local_set(3);
        i64_const(-1); local_set(7);
        local_get(3); i32_eqz;
        if_i64; i64_const(0);
        else_;
          i32_const(0); local_set(4);
          block_empty; loop_empty;
            local_get(4); local_get(2); local_get(3); i32_sub; i32_const(1); i32_add;
            i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(4); i32_add;
            local_get(1); i32_const(4); i32_add;
            local_get(3);
            call(emitter.rt.mem_eq);
            if_empty;
              local_get(4); i64_extend_i32_u; local_set(7);
            end;
            local_get(4); i32_const(1); i32_add; local_set(4);
            br(0);
          end; end;
          local_get(7);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

pub(super) fn compile_strip_prefix(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.strip_prefix];
    // params: 0=s, 1=prefix | locals: 2=s_len, 3=p_len, 4=result_str
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_load(0); local_set(3);
        local_get(3); local_get(2); i32_gt_u;
        if_i32; i32_const(0); // none
        else_;
          local_get(0); i32_const(4); i32_add;
          local_get(1); i32_const(4); i32_add;
          local_get(3);
          call(emitter.rt.mem_eq);
          if_i32;
            // some(slice): wrap string ptr in Option (alloc 4 bytes, store ptr)
            local_get(0); local_get(3); local_get(2);
            call(emitter.rt.string.slice); local_set(4);
            i32_const(4); call(emitter.rt.alloc);
            local_tee(3); // reuse local 3
            local_get(4); i32_store(0);
            local_get(3);
          else_;
            i32_const(0);
          end;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

pub(super) fn compile_strip_suffix(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.strip_suffix];
    // params: 0=s, 1=suffix | locals: 2=s_len, 3=p_len, 4=result_str
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_load(0); local_set(3);
        local_get(3); local_get(2); i32_gt_u;
        if_i32; i32_const(0);
        else_;
          local_get(0); i32_const(4); i32_add; local_get(2); i32_add; local_get(3); i32_sub;
          local_get(1); i32_const(4); i32_add;
          local_get(3);
          call(emitter.rt.mem_eq);
          if_i32;
            local_get(0); i32_const(0); local_get(2); local_get(3); i32_sub;
            call(emitter.rt.string.slice); local_set(4);
            i32_const(4); call(emitter.rt.alloc);
            local_tee(3);
            local_get(4); i32_store(0);
            local_get(3);
          else_;
            i32_const(0);
          end;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ── Predicates ──

/// Generic byte predicate: checks all bytes in single range [lo..hi].
/// Empty string returns false (not vacuous truth).
pub(super) fn compile_byte_predicate_range(emitter: &mut WasmEmitter, func_idx: u32, lo: i32, hi: i32) {
    let type_idx = emitter.func_type_indices[&func_idx];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        local_get(1); i32_eqz;
        if_i32; i32_const(0);
        else_;
          i32_const(0); local_set(2);
          block_empty; loop_empty;
            local_get(2); local_get(1); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(2); i32_add; i32_load8_u(0);
            local_tee(1);
            i32_const(lo); i32_lt_u;
            local_get(1); i32_const(hi); i32_gt_u;
            i32_or;
            br_if(1);
            local_get(0); i32_load(0); local_set(1); // restore len
            local_get(2); i32_const(1); i32_add; local_set(2);
            br(0);
          end; end;
          local_get(2); local_get(0); i32_load(0); i32_eq;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

pub(super) fn compile_is_digit(emitter: &mut WasmEmitter) {
    compile_byte_predicate_range(emitter, emitter.rt.string.is_digit, 48, 57);
}

/// is_alpha: all bytes in [A-Z] or [a-z]
/// Empty string returns false.
pub(super) fn compile_is_alpha(emitter: &mut WasmEmitter) {
    let func_idx = emitter.rt.string.is_alpha;
    let type_idx = emitter.func_type_indices[&func_idx];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        local_get(1); i32_eqz;
        if_i32; i32_const(0);
        else_;
          i32_const(0); local_set(2);
          block_empty; loop_empty;
            local_get(2); local_get(1); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(2); i32_add; i32_load8_u(0);
            local_tee(1);
            // (65..90) or (97..122)
            i32_const(65); i32_ge_u;
            local_get(1); i32_const(90); i32_le_u; i32_and;
            local_get(1); i32_const(97); i32_ge_u;
            local_get(1); i32_const(122); i32_le_u; i32_and;
            i32_or;
            i32_eqz;
            br_if(1);
            local_get(0); i32_load(0); local_set(1);
            local_get(2); i32_const(1); i32_add; local_set(2);
            br(0);
          end; end;
          local_get(2); local_get(0); i32_load(0); i32_eq;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// is_alnum: alpha or digit. Empty string returns false.
pub(super) fn compile_is_alnum(emitter: &mut WasmEmitter) {
    let func_idx = emitter.rt.string.is_alnum;
    let type_idx = emitter.func_type_indices[&func_idx];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        local_get(1); i32_eqz;
        if_i32; i32_const(0);
        else_;
          i32_const(0); local_set(2);
          block_empty; loop_empty;
            local_get(2); local_get(1); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(2); i32_add; i32_load8_u(0);
            local_tee(1);
            i32_const(65); i32_ge_u; local_get(1); i32_const(90); i32_le_u; i32_and;
            local_get(1); i32_const(97); i32_ge_u; local_get(1); i32_const(122); i32_le_u; i32_and;
            i32_or;
            local_get(1); i32_const(48); i32_ge_u; local_get(1); i32_const(57); i32_le_u; i32_and;
            i32_or;
            i32_eqz;
            br_if(1);
            local_get(0); i32_load(0); local_set(1);
            local_get(2); i32_const(1); i32_add; local_set(2);
            br(0);
          end; end;
          local_get(2); local_get(0); i32_load(0); i32_eq;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// is_whitespace: space(32), tab(9), LF(10), CR(13). Empty string returns false.
pub(super) fn compile_is_whitespace(emitter: &mut WasmEmitter) {
    let func_idx = emitter.rt.string.is_whitespace;
    let type_idx = emitter.func_type_indices[&func_idx];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        local_get(1); i32_eqz;
        if_i32; i32_const(0);
        else_;
          i32_const(0); local_set(2);
          block_empty; loop_empty;
            local_get(2); local_get(1); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(2); i32_add; i32_load8_u(0);
            local_tee(1);
            i32_const(32); i32_eq;
            local_get(1); i32_const(9); i32_eq; i32_or;
            local_get(1); i32_const(10); i32_eq; i32_or;
            local_get(1); i32_const(13); i32_eq; i32_or;
            i32_eqz;
            br_if(1);
            local_get(0); i32_load(0); local_set(1);
            local_get(2); i32_const(1); i32_add; local_set(2);
            br(0);
          end; end;
          local_get(2); local_get(0); i32_load(0); i32_eq;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// is_upper: all alpha chars in [A-Z], non-alpha chars allowed, empty=false
pub(super) fn compile_is_upper(emitter: &mut WasmEmitter) {
    compile_case_predicate(emitter, emitter.rt.string.is_upper, 65, 90);
}

/// is_lower: all alpha chars in [a-z], non-alpha chars allowed, empty=false
pub(super) fn compile_is_lower(emitter: &mut WasmEmitter) {
    compile_case_predicate(emitter, emitter.rt.string.is_lower, 97, 122);
}

fn compile_case_predicate(emitter: &mut WasmEmitter, func_idx: u32, lo: i32, hi: i32) {
    let type_idx = emitter.func_type_indices[&func_idx];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        local_get(1); i32_eqz;
        if_i32; i32_const(0); // empty → false
        else_;
          i32_const(0); local_set(2);
          block_empty; loop_empty;
            local_get(2); local_get(1); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(2); i32_add; i32_load8_u(0);
            local_tee(1);
            // in_range = (lo..hi)
            i32_const(lo); i32_ge_u; local_get(1); i32_const(hi); i32_le_u; i32_and;
            // is_alpha
            local_get(1); i32_const(65); i32_ge_u; local_get(1); i32_const(90); i32_le_u; i32_and;
            local_get(1); i32_const(97); i32_ge_u; local_get(1); i32_const(122); i32_le_u; i32_and;
            i32_or;
            i32_eqz; // not_alpha
            i32_or; // in_range OR not_alpha
            i32_eqz;
            br_if(1);
            local_get(0); i32_load(0); local_set(1);
            local_get(2); i32_const(1); i32_add; local_set(2);
            br(0);
          end; end;
          local_get(2); local_get(0); i32_load(0); i32_eq;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __str_cmp(a: i32, b: i32) -> i32
/// Lexicographic comparison: negative if a<b, 0 if equal, positive if a>b.
pub(super) fn compile_cmp(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.cmp];
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    use wasm_encoder::Instruction::*;
    let mem0 = wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 };
    let mem0_byte = wasm_encoder::MemArg { offset: 0, align: 0, memory_index: 0 };
    // min_len = min(a.len, b.len)
    f.instruction(&LocalGet(0)).instruction(&I32Load(mem0));
    f.instruction(&LocalGet(1)).instruction(&I32Load(mem0));
    f.instruction(&I32LeU);
    f.instruction(&If(wasm_encoder::BlockType::Result(ValType::I32)));
    f.instruction(&LocalGet(0)).instruction(&I32Load(mem0));
    f.instruction(&Else);
    f.instruction(&LocalGet(1)).instruction(&I32Load(mem0));
    f.instruction(&End);
    f.instruction(&LocalSet(2));
    f.instruction(&I32Const(0)).instruction(&LocalSet(3));
    f.instruction(&Block(wasm_encoder::BlockType::Empty));
    f.instruction(&Loop(wasm_encoder::BlockType::Empty));
    f.instruction(&LocalGet(3)).instruction(&LocalGet(2)).instruction(&I32GeU);
    f.instruction(&BrIf(1));
    f.instruction(&LocalGet(0)).instruction(&I32Const(4)).instruction(&I32Add);
    f.instruction(&LocalGet(3)).instruction(&I32Add);
    f.instruction(&I32Load8U(mem0_byte));
    f.instruction(&LocalSet(4));
    f.instruction(&LocalGet(1)).instruction(&I32Const(4)).instruction(&I32Add);
    f.instruction(&LocalGet(3)).instruction(&I32Add);
    f.instruction(&I32Load8U(mem0_byte));
    f.instruction(&LocalSet(5));
    f.instruction(&LocalGet(4)).instruction(&LocalGet(5)).instruction(&I32Ne);
    f.instruction(&If(wasm_encoder::BlockType::Empty));
    f.instruction(&LocalGet(4)).instruction(&LocalGet(5)).instruction(&I32Sub);
    f.instruction(&Return);
    f.instruction(&End);
    f.instruction(&LocalGet(3)).instruction(&I32Const(1)).instruction(&I32Add).instruction(&LocalSet(3));
    f.instruction(&Br(0));
    f.instruction(&End).instruction(&End);
    f.instruction(&LocalGet(0)).instruction(&I32Load(mem0));
    f.instruction(&LocalGet(1)).instruction(&I32Load(mem0));
    f.instruction(&I32Sub);
    f.instruction(&End);
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}
