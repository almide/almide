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
static LAYOUT_CONSTS: LazyLock<(i32, i32, i32, i32, i32, i32)> = LazyLock::new(|| {
    let r = super::engine::LayoutRegistry::new();
    (
        r.fixed_offset(STRING, ls::DATA) as i32,   // string_data_off()
        r.header_size(STRING) as i32,               // string_hdr()
        r.fixed_offset(STRING, ls::CAP) as i32,     // string_cap_off()
        r.fixed_offset(LIST, ll::DATA) as i32,      // DATA_OFFSET (list)
        r.header_size(LIST) as i32,                  // HEADER_SIZE (list)
        r.fixed_offset(LIST, ll::CAP) as i32,       // list_cap_off()
    )
});
pub(super) fn string_data_off() -> i32 { LAYOUT_CONSTS.0 }
pub(super) fn string_hdr() -> i32 { LAYOUT_CONSTS.1 }
pub(super) fn string_cap_off() -> i32 { LAYOUT_CONSTS.2 }
pub(super) fn list_data_off() -> i32 { LAYOUT_CONSTS.3 }
pub(super) fn list_hdr() -> i32 { LAYOUT_CONSTS.4 }
pub(super) fn list_cap_off() -> i32 { LAYOUT_CONSTS.5 }

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
    // cp_of_byte(s, byte) -> i64 : codepoint index of the byte offset `byte`
    // (count of non-continuation bytes in data[0..min(byte, len)]). Converts
    // the byte offset `find`/`rfind` produce into the CODEPOINT index the
    // user-facing position API speaks (#419).
    emitter.rt.string.cp_of_byte = emitter.register_func("__str_cp_of_byte", ty_i32x2_i64);

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

    // ── Unicode property membership (oracle-derived range tables) ──
    // (scalar) -> i32 (0/1): binary-search the embedded property range table.
    // Registered + compiled here (in order), AFTER the utf8_* helpers and BEFORE
    // the case-folding group — same registration==compile discipline as those.
    emitter.rt.string.prop_alpha = emitter.register_func("__str_prop_alpha", ty_i32_i32);
    emitter.rt.string.prop_alnum = emitter.register_func("__str_prop_alnum", ty_i32_i32);
    emitter.rt.string.prop_upper = emitter.register_func("__str_prop_upper", ty_i32_i32);
    emitter.rt.string.prop_lower = emitter.register_func("__str_prop_lower", ty_i32_i32);
    // Intern the range tables now so the helper bodies can reference their
    // offsets as constants (and the dead-data eliminator keeps a table iff a
    // live predicate references its offset).
    use super::rt_unicode_tables::{intern_table, UnicodeProp};
    emitter.rt.string.prop_alpha_table = intern_table(emitter, UnicodeProp::Alphabetic);
    emitter.rt.string.prop_alnum_table = intern_table(emitter, UnicodeProp::Alphanumeric);
    emitter.rt.string.prop_upper_table = intern_table(emitter, UnicodeProp::Uppercase);
    emitter.rt.string.prop_lower_table = intern_table(emitter, UnicodeProp::Lowercase);

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
    compile_cp_of_byte(emitter);
    compile_run_length_encode(emitter);
    compile_is_unicode_ws(emitter);
    compile_utf8_classify(emitter);
    compile_utf8_width(emitter);
    compile_utf8_scalar(emitter);
    compile_utf8_byte_of_cp(emitter);
    compile_utf8_snap(emitter);
    // Unicode property membership helpers — compiled here in registration order
    // (alpha, alnum, upper, lower), between the utf8_* helpers and case folding.
    super::rt_string_extra::compile_prop_membership(emitter);
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
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.utf8_width, type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.utf8_scalar, type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.utf8_snap, type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.utf8_byte_of_cp, type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.char_count, type_idx, f));
}

/// `cp_of_byte(s, byte) -> i64`. Codepoint index of byte offset `byte`:
/// counts non-continuation bytes in `data[0..min(byte, byte_len)]`. The
/// inverse of `utf8_byte_of_cp` on boundaries; `find`/`rfind` byte results
/// pass through here so `index_of`/`last_index_of` return codepoint indices.
fn compile_cp_of_byte(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.cp_of_byte];
    // Locals: 2=limit (bytes), 3=i (byte index), 4=count
    let mut f = Function::new([(3, ValType::I32)]);
    wasm!(f, {
        // limit = min(byte, byte_len)
        local_get(0); i32_load(0); local_set(2);
        local_get(1); local_get(2); i32_lt_u;
        if_empty;
          local_get(1); local_set(2);
        end;
        i32_const(0); local_set(3);
        i32_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(3); local_get(2); i32_ge_u; br_if(1);
          local_get(0); i32_const(string_data_off()); i32_add;
          local_get(3); i32_add; i32_load8_u(0);
          i32_const(0xC0); i32_and;
          i32_const(0x80); i32_ne;
          if_empty;
            local_get(4); i32_const(1); i32_add; local_set(4);
          end;
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
        local_get(4); i64_extend_i32_u;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.cp_of_byte, type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.eq, type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.contains, type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.is_unicode_ws, type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.trim, type_idx, f));
}

include!("rt_string_p2.rs");
include!("rt_string_p3.rs");
