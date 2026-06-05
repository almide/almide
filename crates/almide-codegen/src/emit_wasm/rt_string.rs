//! String stdlib WASM runtime functions.
//!
//! All `__str_*` runtime function registration and compilation lives here.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::{ValType};
use super::TrackedFunction as Function;
// Layout constants derived from LayoutRegistry at module init.
// These replace the old list_layout::* imports — values are identical but
// come from the single source of truth (engine::layout).
use super::engine::layout::{STRING, LIST, string as ls, list as ll};
use std::sync::LazyLock;
static LAYOUT_CONSTS: LazyLock<(i32, i32, i32, i32, i32)> = LazyLock::new(|| {
    let r = super::engine::LayoutRegistry::new();
    (
        r.fixed_offset(STRING, ls::DATA) as i32,   // string_data_off()
        r.header_size(STRING) as i32,               // string_hdr()
        r.fixed_offset(STRING, ls::CAP) as i32,     // string_cap_off()
        r.fixed_offset(LIST, ll::DATA) as i32,      // DATA_OFFSET (list)
        r.header_size(LIST) as i32,                  // HEADER_SIZE (list)
    )
});
pub(super) fn string_data_off() -> i32 { LAYOUT_CONSTS.0 }
pub(super) fn string_hdr() -> i32 { LAYOUT_CONSTS.1 }
pub(super) fn string_cap_off() -> i32 { LAYOUT_CONSTS.2 }
pub(super) fn list_data_off() -> i32 { LAYOUT_CONSTS.3 }
pub(super) fn list_hdr() -> i32 { LAYOUT_CONSTS.4 }

/// Register all string runtime function signatures.
pub fn register(emitter: &mut WasmEmitter) {
    // Reusable type signatures
    let ty_i32x3_i32 = emitter.register_type(vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]);
    let ty_i32x2_i32 = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    let ty_i32_i32 = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    let ty_i32x2_i64 = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I64]);
    let ty_i32_i64 = emitter.register_type(vec![ValType::I32], vec![ValType::I64]);

    let s = &mut emitter.rt.string;
    // Will be set after register_func calls — need to go through emitter
    let _ = s;

    let rt = &mut emitter.rt;
    // Can't borrow emitter and rt.string simultaneously. Use a different approach.
    let _ = rt;

    // Core string ops
    emitter.rt.string.eq = emitter.register_func("__str_eq", ty_i32x2_i32);
    emitter.rt.string.contains = emitter.register_func("__str_contains", ty_i32x2_i32);
    emitter.rt.string.trim = emitter.register_func("__str_trim", ty_i32_i32);

    // Slice / transform
    emitter.rt.string.slice = emitter.register_func("__str_slice", ty_i32x3_i32);
    emitter.rt.string.reverse = emitter.register_func("__str_reverse", ty_i32_i32);
    emitter.rt.string.repeat = emitter.register_func("__str_repeat", ty_i32x2_i32);
    emitter.rt.string.index_of = emitter.register_func("__str_index_of", ty_i32x2_i64);
    emitter.rt.string.replace = emitter.register_func("__str_replace", ty_i32x3_i32);
    emitter.rt.string.split = emitter.register_func("__str_split", ty_i32x2_i32);
    emitter.rt.string.join = emitter.register_func("__str_join", ty_i32x2_i32);
    emitter.rt.string.count = emitter.register_func("__str_count", ty_i32x2_i64);

    // Padding / trimming
    emitter.rt.string.pad_start = emitter.register_func("__str_pad_start", ty_i32x3_i32);
    emitter.rt.string.pad_end = emitter.register_func("__str_pad_end", ty_i32x3_i32);
    emitter.rt.string.trim_start = emitter.register_func("__str_trim_start", ty_i32_i32);
    emitter.rt.string.trim_end = emitter.register_func("__str_trim_end", ty_i32_i32);

    // Case transform
    emitter.rt.string.to_upper = emitter.register_func("__str_to_upper", ty_i32_i32);
    emitter.rt.string.to_lower = emitter.register_func("__str_to_lower", ty_i32_i32);

    // Decompose
    emitter.rt.string.chars = emitter.register_func("__str_chars", ty_i32_i32);
    emitter.rt.string.lines = emitter.register_func("__str_lines", ty_i32_i32);
    emitter.rt.string.from_bytes = emitter.register_func("__str_from_bytes", ty_i32_i32);
    emitter.rt.string.to_bytes = emitter.register_func("__str_to_bytes", ty_i32_i32);

    // Replace / search variants
    emitter.rt.string.replace_first = emitter.register_func("__str_replace_first", ty_i32x3_i32);
    emitter.rt.string.last_index_of = emitter.register_func("__str_last_index_of", ty_i32x2_i64);
    emitter.rt.string.strip_prefix = emitter.register_func("__str_strip_prefix", ty_i32x2_i32);
    emitter.rt.string.strip_suffix = emitter.register_func("__str_strip_suffix", ty_i32x2_i32);

    // Predicates — return i32 (Bool is i32 in WASM)
    emitter.rt.string.is_digit = emitter.register_func("__str_is_digit", ty_i32_i32);
    emitter.rt.string.is_alpha = emitter.register_func("__str_is_alpha", ty_i32_i32);
    emitter.rt.string.is_alnum = emitter.register_func("__str_is_alnum", ty_i32_i32);
    emitter.rt.string.is_whitespace = emitter.register_func("__str_is_whitespace", ty_i32_i32);
    emitter.rt.string.is_upper = emitter.register_func("__str_is_upper", ty_i32_i32);
    emitter.rt.string.is_lower = emitter.register_func("__str_is_lower", ty_i32_i32);

    // String comparison: (a: i32, b: i32) -> i32 (negative/0/positive)
    emitter.rt.string.cmp = emitter.register_func("__str_cmp", ty_i32x2_i32);

    // UTF-8 char count: (s: i32) -> i64 — counts code points, not bytes.
    // Distinct from `string.len` which reads the byte-count header and is used
    // for sizing; `char_count` walks the data section and skips UTF-8
    // continuation bytes (bytes whose top two bits are `10`).
    emitter.rt.string.char_count = emitter.register_func("__str_char_count", ty_i32_i64);

    // run_length_encode: (s) -> List[(String, Int)]. Byte-level runs (matches
    // the byte-based rest of this runtime; native is codepoint-based, so the
    // ASCII cases agree and multibyte joins the string-codepoint cluster).
    emitter.rt.string.run_length_encode = emitter.register_func("__str_rle", ty_i32_i32);

    // Unicode White_Space membership: __is_unicode_ws(scalar) -> i32. The single
    // source of truth for every trim / is_whitespace / parse-trim site. No
    // dependency on the utf8_* helpers (it takes an already-decoded scalar).
    emitter.rt.string.is_unicode_ws = emitter.register_func("__is_unicode_ws", ty_i32_i32);
    // UTF-8 sequence classifier for from_utf8_lossy: __utf8_classify(buf, i, n) -> i32.
    emitter.rt.string.utf8_classify = emitter.register_func("__utf8_classify", ty_i32x3_i32);

    // ── Shared UTF-8 codepoint helpers ──
    // IMPORTANT: registration order here MUST match the compile() call order
    // below — function bodies are emitted to the code section in compile order
    // and bound to indices in registration order. These four are registered
    // and compiled last, after run_length_encode.
    // utf8_width(s, byte_i) -> i32 : byte width (1-4) of the codepoint whose
    //   lead byte is at data offset byte_i. Bounds-safe (clamped to remaining
    //   bytes; stray continuation byte → 1).
    emitter.rt.string.utf8_width = emitter.register_func("__utf8_width", ty_i32x2_i32);
    // utf8_scalar(s, byte_i) -> i64 : Unicode scalar of the codepoint at byte_i.
    emitter.rt.string.utf8_scalar = emitter.register_func("__utf8_scalar", ty_i32x2_i64);
    // utf8_byte_of_cp(s, n) -> i32 : byte offset of the start of the n-th
    //   codepoint (n >= count → byte length).
    emitter.rt.string.utf8_byte_of_cp = emitter.register_func("__utf8_byte_of_cp", ty_i32x2_i32);
    // utf8_snap(s, byte_i) -> i32 : byte_i snapped down to a char boundary.
    emitter.rt.string.utf8_snap = emitter.register_func("__utf8_snap", ty_i32x2_i32);

    // ── Full-Unicode case folding ──
    // Registered + compiled LAST, in identical order (same discipline as the
    // utf8_* helpers and Dragon4): bodies bind to indices by registration and
    // emit by compile order, so a mismatch produces an invalid module.
    // __utf8_emit_scalar(dst, byte_off, scalar) -> new_byte_off
    emitter.rt.string.utf8_emit_scalar = emitter.register_func("__utf8_emit_scalar", ty_i32x3_i32);
    // __case_map_lookup(map_sel, scalar) -> VALS addr | -1
    emitter.rt.string.case_map_lookup = emitter.register_func("__case_map_lookup", ty_i32x2_i32);
    // __set_member(set_sel, scalar) -> 0/1
    emitter.rt.string.set_member = emitter.register_func("__set_member", ty_i32x2_i32);
    // __final_sigma(s, byte_off) -> ς|σ
    emitter.rt.string.final_sigma = emitter.register_func("__final_sigma", ty_i32x2_i32);
    // __str_case_map(s, is_upper) -> i32 (unified two-pass driver)
    emitter.rt.string.str_case_map = emitter.register_func("__str_case_map", ty_i32x2_i32);
    // __str_capitalize(s) -> i32
    emitter.rt.string.capitalize = emitter.register_func("__str_capitalize", ty_i32_i32);
}

/// Compile all string runtime function bodies.
pub fn compile(emitter: &mut WasmEmitter) {
    compile_eq(emitter);
    compile_contains(emitter);
    compile_trim(emitter);
    compile_slice(emitter);
    compile_reverse(emitter);
    compile_repeat(emitter);
    compile_index_of(emitter);
    compile_replace(emitter);
    compile_split(emitter);
    compile_join(emitter);
    compile_count(emitter);
    compile_pad_start(emitter);
    compile_pad_end(emitter);
    compile_trim_start(emitter);
    compile_trim_end(emitter);
    compile_to_upper(emitter);
    compile_to_lower(emitter);
    compile_chars(emitter);
    compile_lines(emitter);
    compile_from_bytes(emitter);
    compile_to_bytes(emitter);
    super::rt_string_extra::compile_replace_first(emitter);
    super::rt_string_extra::compile_last_index_of(emitter);
    super::rt_string_extra::compile_strip_prefix(emitter);
    super::rt_string_extra::compile_strip_suffix(emitter);
    super::rt_string_extra::compile_is_digit(emitter);
    super::rt_string_extra::compile_is_alpha(emitter);
    super::rt_string_extra::compile_is_alnum(emitter);
    super::rt_string_extra::compile_is_whitespace(emitter);
    super::rt_string_extra::compile_is_upper(emitter);
    super::rt_string_extra::compile_is_lower(emitter);
    super::rt_string_extra::compile_cmp(emitter);
    compile_char_count(emitter);
    compile_run_length_encode(emitter);
    compile_is_unicode_ws(emitter);
    compile_utf8_classify(emitter);
    compile_utf8_width(emitter);
    compile_utf8_scalar(emitter);
    compile_utf8_byte_of_cp(emitter);
    compile_utf8_snap(emitter);
    // Case folding — compiled LAST, in registration order.
    compile_utf8_emit_scalar(emitter);
    compile_case_map_lookup(emitter);
    compile_set_member(emitter);
    compile_final_sigma(emitter);
    compile_str_case_map(emitter);
    compile_str_capitalize(emitter);
}

// ── Shared UTF-8 codepoint helpers ──
//
// UTF-8 lead-byte classification (see task spec):
//   b0 < 0x80          → width 1 (ASCII)
//   0x80 <= b0 < 0xC0  → continuation byte (malformed as a lead) → width 1
//   0xC0 <= b0 < 0xE0  → width 2
//   0xE0 <= b0 < 0xF0  → width 3
//   b0 >= 0xF0         → width 4
// The width is then clamped so it never runs past the byte length — a
// truncated trailing sequence reads only the bytes that exist.

/// `utf8_width(s, byte_i) -> i32`. Returns the byte width (1-4) of the codepoint
/// whose lead byte is at data offset `byte_i`, clamped to the remaining bytes.
fn compile_utf8_width(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.utf8_width];
    // params: 0=s, 1=byte_i | locals: 2=blen, 3=b0, 4=width
    let mut f = Function::new([(3, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);                 // blen = *s
        // b0 = s[data + byte_i]
        local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
        i32_load8_u(0); local_set(3);
        // width by lead-byte class
        local_get(3); i32_const(0x80); i32_lt_u;
        if_i32; i32_const(1);
        else_;
          local_get(3); i32_const(0xF0); i32_ge_u;
          if_i32; i32_const(4);
          else_;
            local_get(3); i32_const(0xE0); i32_ge_u;
            if_i32; i32_const(3);
            else_;
              local_get(3); i32_const(0xC0); i32_ge_u;
              if_i32; i32_const(2);
              else_; i32_const(1); // continuation byte as lead → 1
              end;
            end;
          end;
        end;
        local_set(4);
        // clamp width to remaining bytes: if byte_i + width > blen → blen - byte_i
        local_get(1); local_get(4); i32_add; local_get(2); i32_gt_u;
        if_i32;
          local_get(2); local_get(1); i32_sub;
        else_;
          local_get(4);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// `utf8_scalar(s, byte_i) -> i64`. Decodes the Unicode scalar at data offset
/// `byte_i`. Combines the lead byte's low bits with `width-1` continuation
/// bytes. A malformed/truncated sequence (width clamped to 1) yields the raw
/// lead byte.
fn compile_utf8_scalar(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.utf8_scalar];
    // params: 0=s, 1=byte_i | locals: 2=width, 3=b0, 4=scalar(i64), 5=k(i32), 6=cont(i32)
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I64),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(0); local_get(1); call(emitter.rt.string.utf8_width); local_set(2);
        local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
        i32_load8_u(0); local_set(3);                            // b0
        // width 1 → scalar = b0 (ASCII or fallback)
        local_get(2); i32_const(1); i32_eq;
        if_i64;
          local_get(3); i64_extend_i32_u;
        else_;
          // mask lead bits: 2→0x1F, 3→0x0F, 4→0x07
          local_get(2); i32_const(2); i32_eq;
          if_i32; i32_const(0x1F);
          else_;
            local_get(2); i32_const(3); i32_eq;
            if_i32; i32_const(0x0F); else_; i32_const(0x07); end;
          end;
          local_get(3); i32_and; i64_extend_i32_u; local_set(4); // scalar = b0 & mask
          // fold in (width-1) continuation bytes
          i32_const(1); local_set(5);                            // k = 1
          block_empty; loop_empty;
            local_get(5); local_get(2); i32_ge_u; br_if(1);
            local_get(0); i32_const(string_data_off()); i32_add;
            local_get(1); i32_add; local_get(5); i32_add;
            i32_load8_u(0); i32_const(0x3F); i32_and; local_set(6); // cont = byte & 0x3F
            local_get(4); i64_const(6); i64_shl;
            local_get(6); i64_extend_i32_u; i64_or; local_set(4);   // scalar = (scalar<<6) | cont
            local_get(5); i32_const(1); i32_add; local_set(5);
            br(0);
          end; end;
          local_get(4);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// `utf8_snap(s, byte_i) -> i32`. Rounds `byte_i` DOWN to the nearest UTF-8
/// char boundary, clamped to `[0, byte_len]`. A boundary is any byte that is
/// not a continuation byte (`0x80..0xC0`). Mirrors native slice's
/// `is_char_boundary` round-down so a byte range never splits a codepoint.
fn compile_utf8_snap(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.utf8_snap];
    // params: 0=s, 1=byte_i | locals: 2=blen, 3=i, 4=byte
    let mut f = Function::new([(3, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);                 // blen
        // i = min(byte_i, blen)
        local_get(1); local_get(2); i32_lt_u;
        if_i32; local_get(1); else_; local_get(2); end;
        local_set(3);
        block_empty; loop_empty;
          // stop at 0 or at a non-continuation byte
          local_get(3); i32_eqz; br_if(1);
          local_get(3); local_get(2); i32_ge_u; br_if(1);       // i == blen is a boundary
          local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add; i32_load8_u(0);
          i32_const(0xC0); i32_and; i32_const(0x80); i32_ne; br_if(1); // not continuation → boundary
          local_get(3); i32_const(1); i32_sub; local_set(3);
          br(0);
        end; end;
        local_get(3);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// `utf8_byte_of_cp(s, n) -> i32`. Walks `n` codepoints from the start and
/// returns the byte offset of the n-th one (or the byte length if `n` exceeds
/// the codepoint count, so callers get a clean "to the end" boundary).
fn compile_utf8_byte_of_cp(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.utf8_byte_of_cp];
    // params: 0=s, 1=n | locals: 2=blen, 3=off, 4=cp
    let mut f = Function::new([(3, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);                 // blen
        i32_const(0); local_set(3);                              // off = 0
        i32_const(0); local_set(4);                              // cp = 0
        block_empty; loop_empty;
          // stop when we've passed n codepoints or run out of bytes
          local_get(4); local_get(1); i32_ge_s; br_if(1);
          local_get(3); local_get(2); i32_ge_u; br_if(1);
          // off += width(off)
          local_get(3);
          local_get(0); local_get(3); call(emitter.rt.string.utf8_width);
          i32_add; local_set(3);
          local_get(4); i32_const(1); i32_add; local_set(4);
          br(0);
        end; end;
        local_get(3);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// Count UTF-8 code points in a string (pointer to `[len:i32][bytes...]`).
/// Iterates the byte payload and skips continuation bytes (top bits `10xxxxxx`).
fn compile_char_count(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.char_count];
    // Locals: 1=byte_len, 2=i (byte index), 3=count
    let mut f = Function::new([(3, ValType::I32)]);
    wasm!(f, {
        // byte_len = *s
        local_get(0); i32_load(0); local_set(1);
        // i = 0, count = 0
        i32_const(0); local_set(2);
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(2); local_get(1); i32_ge_u; br_if(1);
          // byte = s[string_data_off() + i]
          local_get(0); i32_const(string_data_off()); i32_add;
          local_get(2); i32_add; i32_load8_u(0);
          // if (byte & 0xC0) != 0x80 then count++
          i32_const(0xC0); i32_and;
          i32_const(0x80); i32_ne;
          if_empty;
            local_get(3); i32_const(1); i32_add; local_set(3);
          end;
          local_get(2); i32_const(1); i32_add; local_set(2);
          br(0);
        end; end;
        local_get(3); i64_extend_i32_u;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

// ── Core ──

fn compile_eq(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.eq];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    // Same pointer → 1
    wasm!(f, {
        local_get(0); local_get(1); i32_eq;
        if_empty; i32_const(1); return_; end;
    });
    // Length mismatch → 0
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(2); local_get(1); i32_load(0); i32_ne;
        if_empty; i32_const(0); return_; end;
    });
    // Byte-by-byte compare
    wasm!(f, {
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(3); local_get(2); i32_ge_u;
          if_empty; i32_const(1); return_; end;
          local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add; i32_load8_u(0);
          local_get(1); i32_const(string_data_off()); i32_add; local_get(3); i32_add; i32_load8_u(0);
          i32_ne;
          if_empty; i32_const(0); return_; end;
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
    });
    wasm!(f, { i32_const(0); end; });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_contains(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.contains];
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2); // h_len
        local_get(1); i32_load(0); local_set(3); // n_len
        // empty needle → true
        local_get(3); i32_eqz;
        if_empty; i32_const(1); return_; end;
        // n_len > h_len → false
        local_get(3); local_get(2); i32_gt_u;
        if_empty; i32_const(0); return_; end;
        i32_const(0); local_set(4); // i=0
        block_empty; loop_empty;
          local_get(4); local_get(2); local_get(3); i32_sub; i32_const(1); i32_add;
          i32_ge_u; br_if(1);
          // compare h[string_data_off()+i..] with n[string_data_off()..n_len]
          local_get(0); i32_const(string_data_off()); i32_add; local_get(4); i32_add;
          local_get(1); i32_const(string_data_off()); i32_add;
          local_get(3);
          call(emitter.rt.mem_eq);
          if_empty; i32_const(1); return_; end;
          local_get(4); i32_const(1); i32_add; local_set(4);
          br(0);
        end; end;
        i32_const(0); end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// The Unicode `White_Space` codepoint ranges, derived AT EMIT TIME from Rust
/// `char::is_whitespace` — so the set is exactly what native `str::trim` /
/// `char::is_whitespace` use, and stays locked to the compiler's Unicode version
/// (no hardcoded codepoints). Currently 10 contiguous runs over 25 codepoints:
/// the ASCII run U+0009..=U+000D and U+0020, plus U+0085, U+00A0, U+1680,
/// U+2000..=U+200A, U+2028..=U+2029, U+202F, U+205F, U+3000. NOTE: VT (U+000B)
/// and FF (U+000C) ARE whitespace; ZWSP (U+200B) is NOT.
fn whitespace_ranges() -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for cp in 0u32..=char::MAX as u32 {
        if !char::from_u32(cp).is_some_and(|c| c.is_whitespace()) {
            continue;
        }
        match runs.last_mut() {
            Some(last) if last.1 + 1 == cp => last.1 = cp,
            _ => runs.push((cp, cp)),
        }
    }
    runs
}

#[cfg(test)]
mod ws_tests {
    /// The generated ranges must cover exactly Rust's White_Space set, contiguously.
    #[test]
    fn whitespace_ranges_match_char_is_whitespace() {
        let runs = super::whitespace_ranges();
        let total: u32 = runs.iter().map(|(lo, hi)| hi - lo + 1).sum();
        assert_eq!(total, 25, "Unicode White_Space is 25 codepoints");
        // Every codepoint in a run is whitespace; gaps between runs are not.
        for cp in 0u32..=0x3001 {
            let in_runs = runs.iter().any(|(lo, hi)| cp >= *lo && cp <= *hi);
            let is_ws = char::from_u32(cp).is_some_and(|c| c.is_whitespace());
            assert_eq!(in_runs, is_ws, "U+{cp:04X}");
        }
        // VT/FF are whitespace; ZWSP is not (the boundary the byte version got wrong).
        let member = |cp: u32| runs.iter().any(|(lo, hi)| cp >= *lo && cp <= *hi);
        assert!(member(0x0B) && member(0x0C) && member(0xA0) && member(0x3000));
        assert!(!member(0x200B));
    }
}

#[cfg(test)]
mod utf8_lossy_tests {
    use super::{ASCII_MAX, CONT_MASK, CONT_TAG, utf8_lead_groups};

    // Reference from_utf8_lossy built on the DERIVED lead groups + the same
    // maximal-subpart logic the WASM emits; must equal std byte-for-byte.
    fn my_lossy(bytes: &[u8]) -> Vec<u8> {
        let groups = utf8_lead_groups();
        let class = |b0: u8| groups.iter().find(|g| b0 >= g.0 && b0 <= g.1).map(|g| (g.2, g.3, g.4)).unwrap_or((0, 0, 0));
        let fffd = '\u{FFFD}'.to_string().into_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let b0 = bytes[i];
            if b0 as i32 <= ASCII_MAX { out.push(b0); i += 1; continue; }
            let (w, lo2, hi2) = class(b0);
            if w == 0 { out.extend_from_slice(&fffd); i += 1; continue; }
            let (mut consumed, mut ok) = (1usize, true);
            for k in 1..w as usize {
                if i + k >= bytes.len() { ok = false; break; }
                let bk = bytes[i + k];
                let valid = if k == 1 { bk >= lo2 && bk <= hi2 } else { (bk as i32 & CONT_MASK) == CONT_TAG };
                if !valid { ok = false; break; }
                consumed += 1;
            }
            if ok { out.extend_from_slice(&bytes[i..i + w as usize]); i += w as usize; }
            else { out.extend_from_slice(&fffd); i += consumed; }
        }
        out
    }

    #[test]
    fn derived_classification_matches_from_utf8_lossy() {
        assert_eq!(utf8_lead_groups().len(), 8, "the 8 canonical UTF-8 lead-byte groups");
        let mut seed = 0xABCDEF1234567890u64;
        let mut rng = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
        for _ in 0..300_000 {
            let n = (rng() % 8) as usize;
            // bias toward lead/continuation bytes to hit the edge cases
            let bytes: Vec<u8> = (0..n).map(|_| {
                match rng() % 4 { 0 => 0x80 + (rng() % 0x40) as u8, 1 => 0xC0 + (rng() % 0x40) as u8, _ => (rng() % 256) as u8 }
            }).collect();
            let want = String::from_utf8_lossy(&bytes).into_owned().into_bytes();
            assert_eq!(my_lossy(&bytes), want, "{bytes:?}");
        }
    }
}

/// `__is_unicode_ws(scalar) -> i32`: 1 iff `scalar` has the Unicode White_Space
/// property. The OR of `lo <= scalar <= hi` over the emit-time-generated ranges.
fn compile_is_unicode_ws(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.is_unicode_ws];
    let mut f = Function::new([]);
    wasm!(f, { i32_const(0); }); // running OR accumulator
    for (lo, hi) in whitespace_ranges() {
        if lo == hi {
            wasm!(f, { local_get(0); i32_const(lo as i32); i32_eq; i32_or; });
        } else {
            wasm!(f, {
                local_get(0); i32_const(lo as i32); i32_ge_u;
                local_get(0); i32_const(hi as i32); i32_le_u;
                i32_and; i32_or;
            });
        }
    }
    wasm!(f, { end; });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// Emit a forward codepoint loop that advances `pos_local` past leading
/// White_Space, stopping at `end_local` (decode scalar → `__is_unicode_ws`,
/// advance by `utf8_width`). The string pointer is local 0. Shared by the trim
/// runtime fns and the leading-trim in int/float `.parse`.
pub(super) fn emit_trim_forward(f: &mut Function, emitter: &WasmEmitter, pos_local: u32, end_local: u32) {
    let uw = emitter.rt.string.utf8_width;
    let us = emitter.rt.string.utf8_scalar;
    let isws = emitter.rt.string.is_unicode_ws;
    wasm!(f, {
        block_empty; loop_empty;
          local_get(pos_local); local_get(end_local); i32_ge_u; br_if(1);
          local_get(0); local_get(pos_local); call(us); i32_wrap_i64; call(isws); i32_eqz; br_if(1);
          local_get(0); local_get(pos_local); call(uw); local_get(pos_local); i32_add; local_set(pos_local);
          br(0);
        end; end;
    });
}

/// Emit a backward codepoint loop that shrinks `end_local` past trailing
/// White_Space, not below `floor_local` (step back over UTF-8 continuation bytes
/// to the lead byte, decode scalar → `__is_unicode_ws`). `q_local` is scratch.
/// The string pointer is local 0. Shared with int/float `.parse` trailing trim.
pub(super) fn emit_trim_backward(f: &mut Function, emitter: &WasmEmitter, end_local: u32, floor_local: u32, q_local: u32) {
    let us = emitter.rt.string.utf8_scalar;
    let isws = emitter.rt.string.is_unicode_ws;
    let do_ = string_data_off();
    wasm!(f, {
        block_empty; loop_empty;
          local_get(end_local); local_get(floor_local); i32_le_u; br_if(1);
          // q = end-1; step back over continuation bytes (0b10xxxxxx) to the lead byte.
          local_get(end_local); i32_const(1); i32_sub; local_set(q_local);
          block_empty; loop_empty;
            local_get(0); i32_const(do_); i32_add; local_get(q_local); i32_add; i32_load8_u(0);
            i32_const(0xC0); i32_and; i32_const(0x80); i32_ne; br_if(1);   // lead byte → stop
            local_get(q_local); i32_eqz; br_if(1);
            local_get(q_local); i32_const(1); i32_sub; local_set(q_local);
            br(0);
          end; end;
          local_get(0); local_get(q_local); call(us); i32_wrap_i64; call(isws); i32_eqz; br_if(1);
          local_get(q_local); local_set(end_local);
          br(0);
        end; end;
    });
}

fn compile_trim(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.trim];
    const S: u32 = 0; // param: string ptr
    const LEN: u32 = 1;
    const START: u32 = 2;
    const END: u32 = 3;
    const Q: u32 = 4; // scratch for the backward walk
    let mut f = Function::new([(4, ValType::I32)]);
    wasm!(f, {
        local_get(S); i32_load(0); local_set(LEN);
        i32_const(0); local_set(START);
        local_get(LEN); local_set(END);
    });
    emit_trim_forward(&mut f, emitter, START, LEN);
    emit_trim_backward(&mut f, emitter, END, START, Q);
    wasm!(f, {
        local_get(S); local_get(START); local_get(END);
        call(emitter.rt.string.slice);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

// ── Slice / transform ──

fn compile_slice(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.slice];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(2); local_get(1); i32_sub;
        call(emitter.rt.string_alloc); local_set(3);
        i32_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(4); local_get(2); local_get(1); i32_sub; i32_ge_u; br_if(1);
          local_get(3); i32_const(string_data_off()); i32_add; local_get(4); i32_add;
          local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add; local_get(4); i32_add;
          i32_load8_u(0); i32_store8(0);
          local_get(4); i32_const(1); i32_add; local_set(4);
          br(0);
        end; end;
        local_get(3); end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// reverse(s): reverse by CODEPOINT. Each codepoint's bytes are copied in
/// forward order, but whole codepoints are placed from the end of the output
/// toward the start — so multibyte sequences stay valid UTF-8 (native parity).
fn compile_reverse(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.reverse];
    // params: 0=s | locals: 1=blen, 2=result, 3=in_off, 4=out_off, 5=width, 6=k
    let mut f = Function::new([(6, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);                 // blen
        local_get(1); call(emitter.rt.string_alloc); local_set(2);
        i32_const(0); local_set(3);                              // in_off = 0
        local_get(1); local_set(4);                              // out_off = blen (write end-first)
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          // width of codepoint at in_off
          local_get(0); local_get(3); call(emitter.rt.string.utf8_width); local_set(5);
          // out_off -= width (start of this codepoint in the output)
          local_get(4); local_get(5); i32_sub; local_set(4);
          // copy width bytes forward: out[out_off + k] = in[in_off + k]
          i32_const(0); local_set(6);
          block_empty; loop_empty;
            local_get(6); local_get(5); i32_ge_u; br_if(1);
            local_get(2); i32_const(string_data_off()); i32_add; local_get(4); i32_add; local_get(6); i32_add;
            local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add; local_get(6); i32_add;
            i32_load8_u(0); i32_store8(0);
            local_get(6); i32_const(1); i32_add; local_set(6);
            br(0);
          end; end;
          local_get(3); local_get(5); i32_add; local_set(3);     // in_off += width
          br(0);
        end; end;
        local_get(2); end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_repeat(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.repeat];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_get(1); i32_mul; local_set(2);
        local_get(2); call(emitter.rt.string_alloc); local_set(3);
        i32_const(0); local_set(2); // reuse as offset
        block_empty; loop_empty;
          local_get(2); local_get(0); i32_load(0); local_get(1); i32_mul; i32_ge_u; br_if(1);
          local_get(3); i32_const(string_data_off()); i32_add; local_get(2); i32_add;
          local_get(0); i32_const(string_data_off()); i32_add;
          local_get(2); local_get(0); i32_load(0); i32_rem_u;
          i32_add; i32_load8_u(0); i32_store8(0);
          local_get(2); i32_const(1); i32_add; local_set(2);
          br(0);
        end; end;
        local_get(3); end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_index_of(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.index_of];
    // params: 0=s, 1=needle | locals: 2=s_len, 3=n_len, 4=i, 5=result(i64)
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32), (1, ValType::I64),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_load(0); local_set(3);
        i64_const(-1); local_set(5); // result = -1 (not found)
        // empty needle → 0
        local_get(3); i32_eqz;
        if_empty; i64_const(0); local_set(5); i64_const(0); return_; end;
        // n_len > s_len → -1
        local_get(3); local_get(2); i32_gt_u;
        if_empty; i64_const(-1); return_; end;
        // Scan
        i32_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(4); local_get(2); local_get(3); i32_sub; i32_const(1); i32_add;
          i32_ge_u; br_if(1);
          local_get(0); i32_const(string_data_off()); i32_add; local_get(4); i32_add;
          local_get(1); i32_const(string_data_off()); i32_add;
          local_get(3);
          call(emitter.rt.mem_eq);
          if_empty;
            local_get(4); i64_extend_i32_u; return_;
          end;
          local_get(4); i32_const(1); i32_add; local_set(4);
          br(0);
        end; end;
        i64_const(-1); end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_replace(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.replace];
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
          local_get(7); local_get(1); local_get(2);
          call(emitter.rt.string.replace); local_set(7);
          local_get(6); local_get(2); call(emitter.rt.concat_str);
          local_get(7); call(emitter.rt.concat_str);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// Recursive split using index_of. Supports multi-char delimiter.
/// Strategy: find first delimiter → [before] ++ split(rest, delim)
/// Base case: no delimiter found → [s]
fn compile_split(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.split];
    // params: 0=s, 1=delim | locals: 2=idx(i64), 3=d_len, 4=before, 5=rest, 6=rest_list, 7=result
    let mut f = Function::new([
        (1, ValType::I64), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(1); i32_load(0); local_set(3); // d_len
        // Empty delimiter: return [s] to avoid infinite recursion on index_of("x", "") == 0.
        local_get(3); i32_eqz;
        if_empty;
          i32_const(12); call(emitter.rt.alloc); local_set(7);
          local_get(7); i32_const(1); i32_store(0);
          local_get(7); local_get(0); i32_store(8);
          local_get(7); return_;
        end;
        local_get(0); local_get(1); call(emitter.rt.string.index_of); local_set(2);
        local_get(2); i64_const(-1); i64_eq;
        if_i32;
          // No match: return [s]
          i32_const(12); call(emitter.rt.alloc); local_set(7);
          local_get(7); i32_const(1); i32_store(0);
          local_get(7); local_get(0); i32_store(8);
          local_get(7);
        else_;
          // before = slice(s, 0, idx)
          local_get(0); i32_const(0); local_get(2); i32_wrap_i64;
          call(emitter.rt.string.slice); local_set(4);
          // rest = slice(s, idx + d_len, s_len)
          local_get(0);
          local_get(2); i32_wrap_i64; local_get(3); i32_add;
          local_get(0); i32_load(0);
          call(emitter.rt.string.slice); local_set(5);
          // rest_list = split(rest, delim) — recursive
          local_get(5); local_get(1);
          call(emitter.rt.string.split); local_set(6);
          // result = [before] ++ rest_list
          // Alloc: HEADER_SIZE + (1 + rest_list.len) * 4
          i32_const(list_hdr());
          local_get(6); i32_load(0); i32_const(1); i32_add;
          i32_const(4); i32_mul; i32_add;
          call(emitter.rt.alloc); local_set(7);
          local_get(7);
          local_get(6); i32_load(0); i32_const(1); i32_add;
          i32_store(0); // result.len
          // result[0] = before
          local_get(7); local_get(4); i32_store(8);
    });
    // Copy rest_list elements to result[1..]
    wasm!(f, {
          i32_const(0); local_set(3); // reuse as i
          block_empty; loop_empty;
            local_get(3); local_get(6); i32_load(0); i32_ge_u; br_if(1);
            local_get(7); i32_const(12); i32_add; // &result[1] = data_offset(8) + 1*4
            local_get(3); i32_const(4); i32_mul; i32_add;
            local_get(6); i32_const(list_data_off()); i32_add;
            local_get(3); i32_const(4); i32_mul; i32_add;
            i32_load(0); i32_store(0);
            local_get(3); i32_const(1); i32_add; local_set(3);
            br(0);
          end; end;
          local_get(7);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_join(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.join];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2); // len
        local_get(2); i32_eqz;
        if_i32;
          // empty list → empty string
          i32_const(0); call(emitter.rt.string_alloc);
        else_;
          // result = list[0]
          local_get(0); i32_const(list_data_off()); i32_add; i32_load(0); local_set(4);
          i32_const(1); local_set(3); // i=1
          block_empty; loop_empty;
            local_get(3); local_get(2); i32_ge_u; br_if(1);
            // result = concat(result, sep)
            local_get(4); local_get(1); call(emitter.rt.concat_str); local_set(4);
            // result = concat(result, list[i])
            local_get(4);
            local_get(0); i32_const(list_data_off()); i32_add;
            local_get(3); i32_const(4); i32_mul; i32_add; i32_load(0);
            call(emitter.rt.concat_str); local_set(4);
            local_get(3); i32_const(1); i32_add; local_set(3);
            br(0);
          end; end;
          local_get(4);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_count(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.count];
    let mut f = Function::new([
        (1, ValType::I64), (1, ValType::I64), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        i64_const(0); local_set(2); // count
        i32_const(0); local_set(4); // pos
        local_get(1); i32_load(0); local_set(5); // sub_len
        local_get(5); i32_eqz;
        if_i64; i64_const(0);
        else_;
          block_empty; loop_empty;
            local_get(0); local_get(4); local_get(0); i32_load(0);
            call(emitter.rt.string.slice); local_set(6);
            local_get(6); local_get(1); call(emitter.rt.string.index_of); local_set(3);
            local_get(3); i64_const(-1); i64_eq; br_if(1);
            local_get(2); i64_const(1); i64_add; local_set(2);
            local_get(4); local_get(3); i32_wrap_i64; i32_add;
            local_get(5); i32_add; local_set(4);
            br(0);
          end; end;
          local_get(2);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

// ── Padding / trimming ──

/// Build a 1-codepoint String holding the FIRST codepoint of `pad`. Empty
/// `pad` degenerates to a width-0 string (native uses `' '`, but pad is never
/// empty in practice for the padding ops; an empty pad simply pads with
/// nothing on both targets — kept consistent here).
fn emit_pad_first_cp(emitter: &mut WasmEmitter, f: &mut Function, pad_local: u32, out_local: u32) {
    // width of first codepoint (0 if pad empty)
    wasm!(*f, {
        local_get(pad_local); i32_load(0); i32_eqz;
        if_i32;
          i32_const(0); call(emitter.rt.string_alloc);
        else_;
          // unit = slice(pad, 0, width(pad, 0))
          local_get(pad_local); i32_const(0);
          local_get(pad_local); i32_const(0); call(emitter.rt.string.utf8_width);
          call(emitter.rt.string.slice);
        end;
        local_set(out_local);
    });
}

/// pad_start(s, width, pad): width measured in CODEPOINTS; pad unit = first
/// codepoint of `pad`, repeated (width - char_count(s)) times, prepended.
fn compile_pad_start(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.pad_start];
    // params: 0=s, 1=width, 2=pad | locals: 3=count, 4=n, 5=unit, 6=fill
    let mut f = Function::new([(4, ValType::I32)]);
    wasm!(f, {
        local_get(0); call(emitter.rt.string.char_count); i32_wrap_i64; local_set(3);
        local_get(3); local_get(1); i32_ge_u;
        if_i32; local_get(0);
        else_;
          local_get(1); local_get(3); i32_sub; local_set(4);    // n = width - count
    });
    emit_pad_first_cp(emitter, &mut f, 2, 5);                    // unit (local 5)
    wasm!(f, {
          local_get(5); local_get(4); call(emitter.rt.string.repeat); local_set(6);
          local_get(6); local_get(0); call(emitter.rt.concat_str);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// pad_end(s, width, pad): like pad_start but appended.
fn compile_pad_end(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.pad_end];
    let mut f = Function::new([(4, ValType::I32)]);
    wasm!(f, {
        local_get(0); call(emitter.rt.string.char_count); i32_wrap_i64; local_set(3);
        local_get(3); local_get(1); i32_ge_u;
        if_i32; local_get(0);
        else_;
          local_get(1); local_get(3); i32_sub; local_set(4);
    });
    emit_pad_first_cp(emitter, &mut f, 2, 5);
    wasm!(f, {
          local_get(5); local_get(4); call(emitter.rt.string.repeat); local_set(6);
          local_get(0); local_get(6); call(emitter.rt.concat_str);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_trim_start(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.trim_start];
    const S: u32 = 0; // param
    const LEN: u32 = 1;
    const START: u32 = 2;
    let mut f = Function::new([(2, ValType::I32)]);
    wasm!(f, {
        local_get(S); i32_load(0); local_set(LEN);
        i32_const(0); local_set(START);
    });
    emit_trim_forward(&mut f, emitter, START, LEN);
    wasm!(f, {
        local_get(S); local_get(START); local_get(LEN);
        call(emitter.rt.string.slice);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_trim_end(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.trim_end];
    const S: u32 = 0; // param
    const END: u32 = 1;
    const Q: u32 = 2; // scratch for the backward walk
    const FLOOR: u32 = 3; // 0 — never trim below the start
    let mut f = Function::new([(3, ValType::I32)]);
    wasm!(f, {
        local_get(S); i32_load(0); local_set(END);
        i32_const(0); local_set(FLOOR);
    });
    emit_trim_backward(&mut f, emitter, END, FLOOR, Q);
    wasm!(f, {
        local_get(S); i32_const(0); local_get(END);
        call(emitter.rt.string.slice);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

// ── Case transform ──
//
// Full-Unicode, byte-identical to native `str::to_uppercase()`/`to_lowercase()`.
// `to_upper`/`to_lower` are thin wrappers over the unified `__str_case_map`
// driver; the real work (oracle-derived table lookup, Final_Sigma scan, two-pass
// exact-size allocation) lives in the case-folding functions at the end of this
// file. The old ASCII-only ±32 byte loop (`compile_case_transform`) is gone.

fn compile_to_upper(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.to_upper];
    let map = emitter.rt.string.str_case_map;
    let mut f = Function::new([]);
    wasm!(f, { local_get(0); i32_const(1); call(map); end; });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_to_lower(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.to_lower];
    let map = emitter.rt.string.str_case_map;
    let mut f = Function::new([]);
    wasm!(f, { local_get(0); i32_const(0); call(map); end; });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

// ── Decompose ──

/// chars(s): one element per CODEPOINT, each a String holding that codepoint's
/// 1-4 UTF-8 bytes. The list length is the codepoint count (worst case = byte
/// length, so we size the list buffer by byte length and only fill `j` slots).
fn compile_chars(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.chars];
    // params: 0=s | locals: 1=blen, 2=result, 3=in_off, 4=str, 5=width, 6=j, 7=k
    let mut f = Function::new([(7, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);                 // blen
        // worst-case slots = blen (all-ASCII); fewer codepoints just leave gaps
        i32_const(list_hdr()); local_get(1); i32_const(4); i32_mul; i32_add;
        call(emitter.rt.alloc); local_set(2);
        i32_const(0); local_set(3);                              // in_off = 0
        i32_const(0); local_set(6);                              // j = 0 (codepoint index)
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(0); local_get(3); call(emitter.rt.string.utf8_width); local_set(5);
          // str = alloc(width); copy width bytes
          local_get(5); call(emitter.rt.string_alloc); local_set(4);
          i32_const(0); local_set(7);
          block_empty; loop_empty;
            local_get(7); local_get(5); i32_ge_u; br_if(1);
            local_get(4); i32_const(string_data_off()); i32_add; local_get(7); i32_add;
            local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add; local_get(7); i32_add;
            i32_load8_u(0); i32_store8(0);
            local_get(7); i32_const(1); i32_add; local_set(7);
            br(0);
          end; end;
          // result.data[j] = str
          local_get(2); i32_const(list_data_off()); i32_add; local_get(6); i32_const(4); i32_mul; i32_add;
          local_get(4); i32_store(0);
          local_get(3); local_get(5); i32_add; local_set(3);     // in_off += width
          local_get(6); i32_const(1); i32_add; local_set(6);     // j += 1
          br(0);
        end; end;
        local_get(2); local_get(6); i32_store(0);                // result.len = j
        local_get(2); end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// run_length_encode(s) -> List[(String, Int)].
/// Two passes over the byte payload: first count maximal runs of equal bytes to
/// size the list exactly, then build a 1-char String + i64 count tuple per run.
/// Each list slot holds a pointer to a 12-byte tuple `[str_ptr:i32 @0][cnt:i64 @4]`
/// (tuple fields are laid out sequentially with no padding — see values::byte_size).
/// Byte-granular like the rest of this runtime; native groups by codepoint, so
/// ASCII inputs agree and multibyte is part of the string-codepoint gap.
fn compile_run_length_encode(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.run_length_encode];
    // locals: 1=blen 2=nr 3=i 4=cur 5=result 6=j 7=cnt 8=strp 9=tup
    let mut f = Function::new([(9, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);                 // blen = *s
        // ── Pass 1: count maximal runs into nr ──
        i32_const(0); local_set(2);                              // nr = 0
        i32_const(0); local_set(3);                              // i = 0
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add; i32_load8_u(0); local_set(4); // cur
          local_get(2); i32_const(1); i32_add; local_set(2);     // nr += 1
          local_get(3); i32_const(1); i32_add; local_set(3);     // i += 1
          block_empty; loop_empty;                               // skip equal bytes
            local_get(3); local_get(1); i32_ge_u; br_if(1);
            local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add; i32_load8_u(0);
            local_get(4); i32_ne; br_if(1);
            local_get(3); i32_const(1); i32_add; local_set(3);
            br(0);
          end; end;
          br(0);
        end; end;
        // ── Allocate the result list: [len=nr][nr * ptr] ──
        i32_const(list_hdr()); local_get(2); i32_const(4); i32_mul; i32_add;
        call(emitter.rt.alloc); local_set(5);
        local_get(5); local_get(2); i32_store(0);
        // ── Pass 2: emit one (char, count) tuple per run ──
        i32_const(0); local_set(3);                              // i = 0
        i32_const(0); local_set(6);                              // j = 0
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add; i32_load8_u(0); local_set(4); // cur
          i32_const(1); local_set(7);                            // cnt = 1
          local_get(3); i32_const(1); i32_add; local_set(3);     // i += 1
          block_empty; loop_empty;
            local_get(3); local_get(1); i32_ge_u; br_if(1);
            local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add; i32_load8_u(0);
            local_get(4); i32_ne; br_if(1);
            local_get(7); i32_const(1); i32_add; local_set(7);   // cnt += 1
            local_get(3); i32_const(1); i32_add; local_set(3);   // i += 1
            br(0);
          end; end;
          // strp = one-char String holding `cur`
          i32_const(1); call(emitter.rt.string_alloc); local_set(8);
          local_get(8); i32_const(1); i32_store(0);              // len = 1
          local_get(8); i32_const(1); i32_store(string_cap_off() as u32, 0); // cap = 1
          local_get(8); local_get(4); i32_store8(string_data_off() as u32);  // data[0] = cur
          // tup = [strp @0][cnt:i64 @4]
          i32_const(12); call(emitter.rt.alloc); local_set(9);
          local_get(9); local_get(8); i32_store(0);
          local_get(9); local_get(7); i64_extend_i32_u; i64_store(4);
          // result.data[j] = tup
          local_get(5); i32_const(list_data_off()); i32_add; local_get(6); i32_const(4); i32_mul; i32_add;
          local_get(9); i32_store(0);
          local_get(6); i32_const(1); i32_add; local_set(6);     // j += 1
          br(0);
        end; end;
        local_get(5); end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_lines(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.lines];
    let mut f = Function::new([(1, ValType::I32)]);
    // If input string is empty, return empty list (alloc HEADER_SIZE bytes, len=0)
    wasm!(f, {
        local_get(0); i32_load(0); i32_eqz;
        if_i32;
          i32_const(list_hdr()); call(emitter.rt.alloc); local_set(1);
          local_get(1); i32_const(0); i32_store(0);
          local_get(1);
        else_;
          i32_const(1); call(emitter.rt.string_alloc); local_set(1);
          local_get(1); i32_const(1); i32_store(0);
          local_get(1); i32_const(1); i32_store(string_cap_off() as u32, 0);
          local_get(1); i32_const(10); i32_store8(string_data_off() as u32);
          local_get(0); local_get(1); call(emitter.rt.string.split);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// U+FFFD REPLACEMENT CHARACTER — `from_utf8_lossy` emits one per maximal invalid
/// subpart.
const REPLACEMENT_SCALAR: i32 = '\u{FFFD}' as i32;
/// Largest ASCII scalar; a byte `<=` this is a complete 1-byte sequence.
const ASCII_MAX: i32 = 0x7F;
/// A UTF-8 continuation byte has its top two bits `0b10`: `(b & CONT_MASK) == CONT_TAG`.
const CONT_MASK: i32 = 0b1100_0000;
const CONT_TAG: i32 = 0b1000_0000;
/// Any valid continuation byte (`0b10_111111`), used to probe `from_utf8` below.
const CONT_SAMPLE: u8 = 0b1011_1111;

/// Pack a `__utf8_classify` result: `consumed` bytes, `valid` flag (1 = well-formed
/// sequence to copy; 0 = maximal invalid subpart → emit one U+FFFD, resume after).
const fn classify_packed(consumed: i32, valid: i32) -> i32 {
    (consumed << 1) | valid
}

/// Derive a non-ASCII lead byte's UTF-8 classification from Rust's OWN validator
/// (no hardcoded Table 3-7 constants): returns `(width, lo2, hi2)` — the sequence
/// length and the valid 2nd-byte range — or `(0, 0, 0)` if `b0` can't start a valid
/// sequence. Probing `std::str::from_utf8` keeps this locked to std's exact UTF-8
/// rules, the same the native `from_utf8_lossy` runtime uses.
fn utf8_lead_class(b0: u8) -> (u8, u8, u8) {
    for width in 2u8..=4 {
        let (mut lo, mut hi) = (None, None);
        for b1 in 0u8..=u8::MAX {
            let mut seq = vec![b0, b1];
            seq.resize(width as usize, CONT_SAMPLE); // valid trailing continuations
            if std::str::from_utf8(&seq).is_ok_and(|s| s.chars().count() == 1) {
                lo.get_or_insert(b1);
                hi = Some(b1);
            }
        }
        if let (Some(lo), Some(hi)) = (lo, hi) {
            return (width, lo, hi);
        }
    }
    (0, 0, 0)
}

/// Lead bytes grouped into contiguous runs of identical `(width, lo2, hi2)` —
/// `(lead_lo, lead_hi, width, lo2, hi2)`. Built from [`utf8_lead_class`], so the
/// boundaries are oracle-derived, never hand-written hex. Cached: the derivation
/// probes `from_utf8` ~98k times, so compute it once per process rather than per
/// module compile.
fn utf8_lead_groups() -> &'static [(u8, u8, u8, u8, u8)] {
    static GROUPS: LazyLock<Vec<(u8, u8, u8, u8, u8)>> = LazyLock::new(|| {
        let mut groups: Vec<(u8, u8, u8, u8, u8)> = Vec::new();
        for b0 in (ASCII_MAX as u8 + 1)..=u8::MAX {
            let (w, lo2, hi2) = utf8_lead_class(b0);
            if w == 0 {
                continue; // invalid lead → handled by the width==0 default
            }
            match groups.last_mut() {
                Some(g) if g.1 + 1 == b0 && (g.2, g.3, g.4) == (w, lo2, hi2) => g.1 = b0,
                _ => groups.push((b0, b0, w, lo2, hi2)),
            }
        }
        groups
    });
    &GROUPS
}

/// `__utf8_classify(buf, i, n) -> i32`: classify the UTF-8 sequence starting at
/// `buf[i]` (within `n` bytes), returning `classify_packed(consumed, valid)`.
/// Replicates Rust `String::from_utf8_lossy`'s maximal-subpart subdivision.
fn compile_utf8_classify(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.utf8_classify];
    let groups = utf8_lead_groups();
    const BUF: u32 = 0; // params
    const I: u32 = 1;
    const N: u32 = 2;
    const B0: u32 = 3; // locals
    const WIDTH: u32 = 4; // 0 ⇒ invalid lead
    const LO2: u32 = 5;
    const HI2: u32 = 6;
    const CONSUMED: u32 = 7;
    const K: u32 = 8;
    const BK: u32 = 9;
    let mut f = Function::new([(7, ValType::I32)]);
    wasm!(f, {
        local_get(BUF); local_get(I); i32_add; i32_load8_u(0); local_set(B0);
        local_get(B0); i32_const(ASCII_MAX); i32_le_u;
        if_empty; i32_const(classify_packed(1, 1)); return_; end;   // ASCII: 1 byte, valid
        i32_const(0); local_set(WIDTH);
    });
    // Lead-byte width + 2nd-byte range, generated from the derived groups.
    for (lead_lo, lead_hi, width, lo2, hi2) in groups {
        wasm!(f, {
            local_get(B0); i32_const(*lead_lo as i32); i32_ge_u;
            local_get(B0); i32_const(*lead_hi as i32); i32_le_u; i32_and;
            if_empty;
              i32_const(*width as i32); local_set(WIDTH);
              i32_const(*lo2 as i32); local_set(LO2);
              i32_const(*hi2 as i32); local_set(HI2);
            end;
        });
    }
    wasm!(f, {
        local_get(WIDTH); i32_eqz;
        if_empty; i32_const(classify_packed(1, 0)); return_; end;   // invalid lead: 1-byte subpart
        // Validate continuation bytes: 2nd in [lo2,hi2]; 3rd/4th are plain continuations.
        // On the first failure the maximal subpart ends, so `consumed < width`.
        i32_const(1); local_set(CONSUMED);
        i32_const(1); local_set(K);
        block_empty; loop_empty;
          local_get(K); local_get(WIDTH); i32_ge_u; br_if(1);             // matched all → valid
          local_get(I); local_get(K); i32_add; local_get(N); i32_ge_u; br_if(1);   // truncated
          local_get(BUF); local_get(I); i32_add; local_get(K); i32_add; i32_load8_u(0); local_set(BK);
          // valid = (k == 1) ? lo2 <= bk <= hi2 : bk is a continuation byte
          local_get(K); i32_const(1); i32_eq;
          if_i32;
            local_get(BK); local_get(LO2); i32_ge_u; local_get(BK); local_get(HI2); i32_le_u; i32_and;
          else_;
            local_get(BK); i32_const(CONT_MASK); i32_and; i32_const(CONT_TAG); i32_eq;
          end;
          i32_eqz; br_if(1);
          local_get(CONSUMED); i32_const(1); i32_add; local_set(CONSUMED);
          local_get(K); i32_const(1); i32_add; local_set(K);
          br(0);
        end; end;
        // classify_packed(consumed, consumed == width)
        local_get(CONSUMED); i32_const(1); i32_shl;
        local_get(CONSUMED); local_get(WIDTH); i32_eq;
        i32_or;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// `from_bytes(list) -> String`: UTF-8-lossy decode of the byte list (each element
/// truncated to a byte), the inverse of `to_bytes`. Two passes over a scratch byte
/// buffer: classify each sequence, copy well-formed bytes through, emit one U+FFFD
/// per maximal invalid subpart — byte-identical to native `String::from_utf8_lossy`.
fn compile_from_bytes(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.from_bytes];
    let classify = emitter.rt.string.utf8_classify;
    let emit_scalar = emitter.rt.string.utf8_emit_scalar;
    let alloc = emitter.rt.alloc;
    let do_ = string_data_off();
    const LIST: u32 = 0; // param
    const N: u32 = 1; // byte count
    const BUF: u32 = 2; // scratch byte buffer
    const I: u32 = 3; // cursor
    const TOTAL: u32 = 4; // pass-1 output byte length
    const R: u32 = 5; // packed classify result
    const CONSUMED: u32 = 6;
    const OUT: u32 = 7; // output string ptr
    const WOFF: u32 = 8; // pass-2 write offset
    // ASCII-out and FFFD-out byte counts for pass-1 sizing.
    let fffd_len = '\u{FFFD}'.len_utf8() as i32;
    let mut f = Function::new([(8, ValType::I32)]);
    wasm!(f, {
        // n = list length; copy elements (truncated to bytes) into a scratch buffer.
        local_get(LIST); i32_load(0); local_set(N);
        local_get(N); call(alloc); local_set(BUF);
        i32_const(0); local_set(I);
        block_empty; loop_empty;
          local_get(I); local_get(N); i32_ge_u; br_if(1);
          local_get(BUF); local_get(I); i32_add;
          local_get(LIST); i32_const(list_data_off()); i32_add; local_get(I); i32_const(8); i32_mul; i32_add;
          i64_load(0); i32_wrap_i64; i32_store8(0);
          local_get(I); i32_const(1); i32_add; local_set(I);
          br(0);
        end; end;
        // PASS 1: output byte length (valid run = consumed bytes; invalid = one U+FFFD).
        i32_const(0); local_set(TOTAL);
        i32_const(0); local_set(I);
        block_empty; loop_empty;
          local_get(I); local_get(N); i32_ge_u; br_if(1);
          local_get(BUF); local_get(I); local_get(N); call(classify); local_set(R);
          local_get(R); i32_const(1); i32_shr_u; local_set(CONSUMED);   // R >> 1
          local_get(R); i32_const(1); i32_and;                          // valid bit
          if_i32; local_get(CONSUMED); else_; i32_const(fffd_len); end;
          local_get(TOTAL); i32_add; local_set(TOTAL);
          local_get(I); local_get(CONSUMED); i32_add; local_set(I);
          br(0);
        end; end;
        // alloc the output string.
        i32_const(string_hdr()); local_get(TOTAL); i32_add; call(alloc); local_set(OUT);
        local_get(OUT); local_get(TOTAL); i32_store(0);
        // PASS 2: fill (copy valid bytes, emit U+FFFD for each invalid subpart).
        i32_const(0); local_set(WOFF);
        i32_const(0); local_set(I);
        block_empty; loop_empty;
          local_get(I); local_get(N); i32_ge_u; br_if(1);
          local_get(BUF); local_get(I); local_get(N); call(classify); local_set(R);
          local_get(R); i32_const(1); i32_shr_u; local_set(CONSUMED);
          local_get(R); i32_const(1); i32_and;
          if_empty;
            local_get(OUT); i32_const(do_); i32_add; local_get(WOFF); i32_add;
            local_get(BUF); local_get(I); i32_add;
            local_get(CONSUMED);
            memory_copy;
            local_get(WOFF); local_get(CONSUMED); i32_add; local_set(WOFF);
          else_;
            local_get(OUT); local_get(WOFF); i32_const(REPLACEMENT_SCALAR); call(emit_scalar); local_set(WOFF);
          end;
          local_get(I); local_get(CONSUMED); i32_add; local_set(I);
          br(0);
        end; end;
        local_get(OUT); end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_to_bytes(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.to_bytes];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        i32_const(list_hdr()); local_get(1); i32_const(8); i32_mul; i32_add;
        call(emitter.rt.alloc); local_set(2);
        local_get(2); local_get(1); i32_store(0);
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(2); i32_const(list_data_off()); i32_add; local_get(3); i32_const(8); i32_mul; i32_add;
          local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add;
          i32_load8_u(0); i64_extend_i32_u; i64_store(0);
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
        local_get(2); end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

// ── Full-Unicode case folding ──
//
// `to_upper`/`to_lower`/`capitalize` are byte-identical to native (Rust
// `str::to_uppercase`/`to_lowercase` + char `to_uppercase`). The mapping tables
// are generated at emit time in `rt_string_case` from the SAME `std`, embedded at
// the front of the data section, and consulted here. Uppercasing is context-free;
// lowercasing is too EXCEPT Greek capital sigma U+03A3 (Final_Sigma), resolved by
// `__final_sigma`. See `rt_string_case` for the derivation + proofs.

/// `__utf8_emit_scalar(dst, byte_off, scalar) -> new_byte_off`. Encodes `scalar`
/// (a valid Unicode scalar, max U+10FFFF) as 1-4 UTF-8 bytes into `dst`'s data
/// section at `byte_off`; returns the advanced byte offset.
fn compile_utf8_emit_scalar(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.utf8_emit_scalar];
    // params: 0=dst, 1=byte_off, 2=scalar | local: 3=addr
    let mut f = Function::new([(1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add; local_set(3);
        local_get(2); i32_const(0x80); i32_lt_u;
        if_i32;
          local_get(3); local_get(2); i32_store8(0);
          local_get(1); i32_const(1); i32_add;
        else_;
          local_get(2); i32_const(0x800); i32_lt_u;
          if_i32;
            local_get(3); local_get(2); i32_const(6); i32_shr_u; i32_const(0xC0); i32_or; i32_store8(0);
            local_get(3); local_get(2); i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(1);
            local_get(1); i32_const(2); i32_add;
          else_;
            local_get(2); i32_const(0x10000); i32_lt_u;
            if_i32;
              local_get(3); local_get(2); i32_const(12); i32_shr_u; i32_const(0xE0); i32_or; i32_store8(0);
              local_get(3); local_get(2); i32_const(6); i32_shr_u; i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(1);
              local_get(3); local_get(2); i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(2);
              local_get(1); i32_const(3); i32_add;
            else_;
              local_get(3); local_get(2); i32_const(18); i32_shr_u; i32_const(0xF0); i32_or; i32_store8(0);
              local_get(3); local_get(2); i32_const(12); i32_shr_u; i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(1);
              local_get(3); local_get(2); i32_const(6); i32_shr_u; i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(2);
              local_get(3); local_get(2); i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(3);
              local_get(1); i32_const(4); i32_add;
            end;
          end;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// `__case_map_lookup(map_sel, scalar) -> i32`. Binary-search the UPPER(0)/LOWER(1)
/// map; returns the absolute address of the `[len:u8][utf8 bytes]` value record,
/// or -1 on miss (caller emits the scalar unchanged). Trivial when no case op is
/// present (then DCE-stubbed anyway).
fn compile_case_map_lookup(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.case_map_lookup];
    // params: 0=map_sel, 1=scalar | locals: 2=keys, 3=n, 4=offs, 5=lo, 6=hi, 7=mid, 8=k
    let mut f = Function::new([(7, ValType::I32)]);
    if let Some(ct) = emitter.case_tables {
        wasm!(f, {
            local_get(0); i32_eqz;
            if_empty;
              i32_const(ct.upper_keys as i32); local_set(2);
              i32_const(ct.upper_n as i32); local_set(3);
              i32_const(ct.upper_offs as i32); local_set(4);
            else_;
              i32_const(ct.lower_keys as i32); local_set(2);
              i32_const(ct.lower_n as i32); local_set(3);
              i32_const(ct.lower_offs as i32); local_set(4);
            end;
            i32_const(0); local_set(5);
            local_get(3); local_set(6);
            block_empty; loop_empty;
              local_get(5); local_get(6); i32_ge_u;
              if_empty; i32_const(-1); return_; end;
              local_get(5); local_get(6); i32_add; i32_const(1); i32_shr_u; local_set(7);
              local_get(2); local_get(7); i32_const(2); i32_shl; i32_add; i32_load(0); local_set(8);
              local_get(8); local_get(1); i32_eq;
              if_empty;
                local_get(4); local_get(7); i32_const(2); i32_shl; i32_add; i32_load(0); return_;
              end;
              local_get(8); local_get(1); i32_lt_u;
              if_empty;
                local_get(7); i32_const(1); i32_add; local_set(5);
              else_;
                local_get(7); local_set(6);
              end;
              br(0);
            end; end;
            i32_const(-1);
            end;
        });
    } else {
        wasm!(f, { i32_const(-1); end; });
    }
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// `__set_member(set_sel, scalar) -> i32`. 1 iff `scalar` is in the CASED(0) /
/// CASE_IGNORABLE(1) sorted key array (binary search). Used by `__final_sigma`.
fn compile_set_member(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.set_member];
    // params: 0=set_sel, 1=scalar | locals: 2=base, 3=n, 4=lo, 5=hi, 6=mid, 7=k
    let mut f = Function::new([(6, ValType::I32)]);
    if let Some(ct) = emitter.case_tables {
        wasm!(f, {
            local_get(0); i32_eqz;
            if_empty;
              i32_const(ct.cased as i32); local_set(2);
              i32_const(ct.cased_n as i32); local_set(3);
            else_;
              i32_const(ct.ci as i32); local_set(2);
              i32_const(ct.ci_n as i32); local_set(3);
            end;
            i32_const(0); local_set(4);
            local_get(3); local_set(5);
            block_empty; loop_empty;
              local_get(4); local_get(5); i32_ge_u;
              if_empty; i32_const(0); return_; end;
              local_get(4); local_get(5); i32_add; i32_const(1); i32_shr_u; local_set(6);
              local_get(2); local_get(6); i32_const(2); i32_shl; i32_add; i32_load(0); local_set(7);
              local_get(7); local_get(1); i32_eq;
              if_empty; i32_const(1); return_; end;
              local_get(7); local_get(1); i32_lt_u;
              if_empty;
                local_get(6); i32_const(1); i32_add; local_set(4);
              else_;
                local_get(6); local_set(5);
              end;
              br(0);
            end; end;
            i32_const(0);
            end;
        });
    } else {
        wasm!(f, { i32_const(0); end; });
    }
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// `__final_sigma(s, byte_off) -> i32`. The Unicode `Final_Sigma` rule for a Σ at
/// `byte_off`: ς (U+03C2) iff it is preceded by a Cased char (skipping
/// Case_Ignorable) AND not followed by one; else σ (U+03C3).
///
/// Both context scans cost O(length of the adjacent Case_Ignorable run), NOT
/// O(position): the "Before" scan steps BACKWARD over codepoints (skipping UTF-8
/// continuation bytes) rather than re-walking from byte 0, so a Σ-dense string
/// stays O(n) overall (a forward re-walk would be O(n²)). This mirrors Rust's
/// reverse-iterator `Final_Sigma` scan in `str::to_lowercase`.
fn compile_final_sigma(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.final_sigma];
    // params: 0=s, 1=byte_off | locals: 2=blen, 3=before, 4=after, 5=p, 6=q, 7=sc, 8=done
    let mut f = Function::new([(7, ValType::I32)]);
    let uw = emitter.rt.string.utf8_width;
    let us = emitter.rt.string.utf8_scalar;
    let setm = emitter.rt.string.set_member;
    let do_ = string_data_off();
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        i32_const(0); local_set(3);   // before
        i32_const(0); local_set(4);   // after
        // Before: step BACKWARD from byte_off over codepoints, skipping
        // Case_Ignorable; the first non-ignorable char's Cased-ness is `before`.
        local_get(1); local_set(5);   // p = byte_off
        i32_const(0); local_set(8);   // done
        block_empty; loop_empty;
          local_get(8); br_if(1);            // done → break
          local_get(5); i32_eqz; br_if(1);   // p == 0 → break (before stays 0)
          // q = p-1; skip UTF-8 continuation bytes (0b10xxxxxx) back to a lead byte.
          local_get(5); i32_const(1); i32_sub; local_set(6);
          block_empty; loop_empty;
            local_get(6); i32_eqz; br_if(1);                   // q == 0 → stop
            local_get(0); i32_const(do_); i32_add; local_get(6); i32_add; i32_load8_u(0);
            i32_const(0xC0); i32_and; i32_const(0x80); i32_eq; // continuation byte?
            i32_eqz; br_if(1);                                 // not continuation → stop (lead byte)
            local_get(6); i32_const(1); i32_sub; local_set(6);
            br(0);
          end; end;
          local_get(0); local_get(6); call(us); i32_wrap_i64; local_set(7);
          i32_const(1); local_get(7); call(setm); i32_eqz;     // not Case_Ignorable
          if_empty;
            i32_const(0); local_get(7); call(setm); local_set(3);  // before = Cased(sc)
            i32_const(1); local_set(8);
          else_;
            local_get(6); local_set(5);                            // p = q (keep scanning back)
          end;
          br(0);
        end; end;
        // After: first non-CI scalar at/after byte_off + width(Σ).
        local_get(1); local_get(0); local_get(1); call(uw); i32_add; local_set(5);
        i32_const(0); local_set(8);
        block_empty; loop_empty;
          local_get(5); local_get(2); i32_ge_u; br_if(1);
          local_get(8); br_if(1);
          local_get(0); local_get(5); call(uw); local_set(6);
          local_get(0); local_get(5); call(us); i32_wrap_i64; local_set(7);
          i32_const(1); local_get(7); call(setm); i32_eqz;
          if_empty;
            i32_const(0); local_get(7); call(setm); local_set(4);
            i32_const(1); local_set(8);
          end;
          local_get(5); local_get(6); i32_add; local_set(5);
          br(0);
        end; end;
        local_get(3); local_get(4); i32_eqz; i32_and;
        if_i32; i32_const(0x03C2); else_; i32_const(0x03C3); end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// `__str_case_map(s, is_upper) -> i32`. The unified two-pass case driver, exact
/// for all scalars. Pass 1 sizes the output (ASCII = 1 byte; Σ-lower = 2 bytes;
/// else table out_len or identity width); ONE allocation; pass 2 fills (ASCII
/// fold inline, Σ via Final_Sigma, else `memory.copy` of the table/identity bytes).
fn compile_str_case_map(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.str_case_map];
    // params: 0=s, 1=is_upper
    // locals: 2=blen,3=total,4=i,5=b0,6=w,7=sc,8=rec,9=out,10=woff,11=outlen,12=fold,13=msel
    let mut f = Function::new([(12, ValType::I32)]);
    let uw = emitter.rt.string.utf8_width;
    let us = emitter.rt.string.utf8_scalar;
    let lk = emitter.rt.string.case_map_lookup;
    let fsig = emitter.rt.string.final_sigma;
    let em = emitter.rt.string.utf8_emit_scalar;
    let alloc = emitter.rt.alloc;
    let do_ = string_data_off();
    let hdr = string_hdr();
    let capo = string_cap_off() as u32;
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_eqz; local_set(13);   // msel = is_upper==0 ? 1 : 0
        // PASS 1: total output bytes
        i32_const(0); local_set(3);
        i32_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(4); local_get(2); i32_ge_u; br_if(1);
          local_get(0); i32_const(do_); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(5);
          local_get(5); i32_const(0x80); i32_lt_u;
          if_empty;
            local_get(3); i32_const(1); i32_add; local_set(3);
            local_get(4); i32_const(1); i32_add; local_set(4);
          else_;
            local_get(0); local_get(4); call(uw); local_set(6);
            local_get(0); local_get(4); call(us); i32_wrap_i64; local_set(7);
            local_get(1); i32_eqz; local_get(7); i32_const(0x03A3); i32_eq; i32_and;
            if_empty;
              local_get(3); i32_const(2); i32_add; local_set(3);
            else_;
              local_get(13); local_get(7); call(lk); local_set(8);
              local_get(8); i32_const(-1); i32_eq;
              if_empty;
                local_get(3); local_get(6); i32_add; local_set(3);
              else_;
                local_get(3); local_get(8); i32_load8_u(0); i32_add; local_set(3);
              end;
            end;
            local_get(4); local_get(6); i32_add; local_set(4);
          end;
          br(0);
        end; end;
        // ALLOC exact-size output
        i32_const(hdr); local_get(3); i32_add; call(alloc); local_set(9);
        local_get(9); local_get(3); i32_store(0);
        local_get(9); local_get(3); i32_store(capo, 0);
        // PASS 2: fill
        i32_const(0); local_set(10);
        i32_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(4); local_get(2); i32_ge_u; br_if(1);
          local_get(0); i32_const(do_); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(5);
          local_get(5); i32_const(0x80); i32_lt_u;
          if_empty;
            local_get(1);
            if_i32;
              local_get(5); i32_const(0x61); i32_ge_u; local_get(5); i32_const(0x7A); i32_le_u; i32_and;
              if_i32; local_get(5); i32_const(32); i32_sub; else_; local_get(5); end;
            else_;
              local_get(5); i32_const(0x41); i32_ge_u; local_get(5); i32_const(0x5A); i32_le_u; i32_and;
              if_i32; local_get(5); i32_const(32); i32_add; else_; local_get(5); end;
            end;
            local_set(12);
            local_get(9); i32_const(do_); i32_add; local_get(10); i32_add; local_get(12); i32_store8(0);
            local_get(10); i32_const(1); i32_add; local_set(10);
            local_get(4); i32_const(1); i32_add; local_set(4);
          else_;
            local_get(0); local_get(4); call(uw); local_set(6);
            local_get(0); local_get(4); call(us); i32_wrap_i64; local_set(7);
            local_get(1); i32_eqz; local_get(7); i32_const(0x03A3); i32_eq; i32_and;
            if_empty;
              local_get(9); local_get(10);
              local_get(0); local_get(4); call(fsig);
              call(em); local_set(10);
            else_;
              local_get(13); local_get(7); call(lk); local_set(8);
              local_get(8); i32_const(-1); i32_eq;
              if_empty;
                local_get(9); i32_const(do_); i32_add; local_get(10); i32_add;
                local_get(0); i32_const(do_); i32_add; local_get(4); i32_add;
                local_get(6);
                memory_copy;
                local_get(10); local_get(6); i32_add; local_set(10);
              else_;
                local_get(8); i32_load8_u(0); local_set(11);
                local_get(9); i32_const(do_); i32_add; local_get(10); i32_add;
                local_get(8); i32_const(1); i32_add;
                local_get(11);
                memory_copy;
                local_get(10); local_get(11); i32_add; local_set(10);
              end;
            end;
            local_get(4); local_get(6); i32_add; local_set(4);
          end;
          br(0);
        end; end;
        local_get(9); end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// `__str_capitalize(s) -> i32`. First scalar uppercased (`char::to_uppercase` —
/// context-free, no Σ rule), the rest of the bytes copied VERBATIM (native
/// `string.capitalize` does not recase the tail).
fn compile_str_capitalize(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.capitalize];
    // params: 0=s (1 param ⇒ declared locals start at index 1; index 1 is unused).
    // locals: 2=blen,3=w0,4=sc0,5=b0,6=rec,7=hlen,8=total,9=out,10=hb
    let mut f = Function::new([(10, ValType::I32)]);
    let uw = emitter.rt.string.utf8_width;
    let us = emitter.rt.string.utf8_scalar;
    let lk = emitter.rt.string.case_map_lookup;
    let alloc = emitter.rt.alloc;
    let do_ = string_data_off();
    let hdr = string_hdr();
    let capo = string_cap_off() as u32;
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(2); i32_eqz; if_empty; local_get(0); return_; end;
        local_get(0); i32_const(do_); i32_add; i32_load8_u(0); local_set(5);
        local_get(0); i32_const(0); call(uw); local_set(3);
        local_get(5); i32_const(0x80); i32_lt_u;
        if_empty;
          i32_const(1); local_set(7);
          local_get(5); i32_const(0x61); i32_ge_u; local_get(5); i32_const(0x7A); i32_le_u; i32_and;
          if_i32; local_get(5); i32_const(32); i32_sub; else_; local_get(5); end;
          local_set(10);
          i32_const(-2); local_set(6);
        else_;
          local_get(0); i32_const(0); call(us); i32_wrap_i64; local_set(4);
          i32_const(0); local_get(4); call(lk); local_set(6);
          local_get(6); i32_const(-1); i32_eq;
          if_empty; local_get(3); local_set(7);
          else_; local_get(6); i32_load8_u(0); local_set(7); end;
        end;
        local_get(7); local_get(2); i32_add; local_get(3); i32_sub; local_set(8);
        i32_const(hdr); local_get(8); i32_add; call(alloc); local_set(9);
        local_get(9); local_get(8); i32_store(0);
        local_get(9); local_get(8); i32_store(capo, 0);
        // head
        local_get(6); i32_const(-2); i32_eq;
        if_empty;
          local_get(9); i32_const(do_); i32_add; local_get(10); i32_store8(0);
        else_;
          local_get(6); i32_const(-1); i32_eq;
          if_empty;
            local_get(9); i32_const(do_); i32_add;
            local_get(0); i32_const(do_); i32_add;
            local_get(3);
            memory_copy;
          else_;
            local_get(9); i32_const(do_); i32_add;
            local_get(6); i32_const(1); i32_add;
            local_get(7);
            memory_copy;
          end;
        end;
        // tail (verbatim): blen - w0 bytes from s data+w0 to out data+hlen
        local_get(9); i32_const(do_); i32_add; local_get(7); i32_add;
        local_get(0); i32_const(do_); i32_add; local_get(3); i32_add;
        local_get(2); local_get(3); i32_sub;
        memory_copy;
        local_get(9); end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

