//! WASM runtime: value.stringify and json.parse.
//!
//! __value_stringify(v: i32) -> i32 (String ptr)
//!   Tag-based dispatch to produce string representation of a Value.
//!
//! __json_parse(s: i32) -> i32 (Result[Value, String] ptr)
//!   Minimal recursive descent JSON parser.

use crate::emit_wasm::engine::{Imm32, Imm64, Local};
use super::{CompiledFunc, WasmEmitter};
use super::rt_string::{string_data_off, string_hdr, string_cap_off, list_data_off, list_hdr};
use wasm_encoder::{ValType};
use super::TrackedFunction as Function;

// Value heap tags (see `compile_value_stringify`): 0=null, 1=bool, 2=int,
// 3=float, 4=string, 5=array, 6=object. Only the container tags matter for the
// path walkers; the rest are "scalar" and never index/field into.
const VTAG_INT: i32 = 2;
const VTAG_FLOAT: i32 = 3;
const VTAG_STRING: i32 = 4;
const VTAG_ARRAY: i32 = 5;
const VTAG_OBJECT: i32 = 6;

/// Size in bytes of a heap value box: `[tag:i32][payload:i32]`. The payload is
/// the inline scalar (bool/int), or a pointer (string/array/object pairs-list).
const VALUE_BOX_SIZE: i32 = 8;

// JsonPath segment tags: 0=Root, 1=Field, 2=Index.
const JP_FIELD_TAG: i32 = 1;
const JP_INDEX_TAG: i32 = 2;

// Memory sizes and strides.
/// Byte width of an i32 / heap pointer.
const I32_BYTES: i32 = 4;
/// Allocation size of a parse-result struct: [value:i32][new_pos:i32][err_flag:i32].
const PARSE_RESULT_SIZE: i32 = 12;
/// Initial element capacity for growable array/object parse buffers.
const INIT_CAPACITY: i32 = 64;
/// Initial byte allocation for growable buffers: list_hdr() + INIT_CAPACITY * I32_BYTES.
const INIT_BUF_BYTES: i32 = 264;

// ASCII character codes used in the JSON parser and string escape decoder.
const ASCII_BS: i32 = 8;        // backspace (\b)
const ASCII_HT: i32 = 9;        // horizontal tab (\t)
const ASCII_LF: i32 = 10;       // line feed (\n)
const ASCII_FF: i32 = 12;       // form feed (\f)
const ASCII_CR: i32 = 13;       // carriage return (\r)
const ASCII_SPACE: i32 = 32;    // space
const ASCII_DQUOTE: i32 = 34;   // '"'
const ASCII_PLUS: i32 = 43;     // '+'
const ASCII_COMMA: i32 = 44;    // ','
const ASCII_MINUS: i32 = 45;    // '-'
const ASCII_DOT: i32 = 46;      // '.'
const ASCII_ZERO: i32 = 48;     // '0'
const ASCII_NINE: i32 = 57;     // '9'
const ASCII_COLON: i32 = 58;    // ':'
const ASCII_E_UPPER: i32 = 69;  // 'E'
const ASCII_LBRACKET: i32 = 91; // '['
const ASCII_BACKSLASH: i32 = 92;// '\\'
const ASCII_RBRACKET: i32 = 93; // ']'
const ASCII_A_LOWER: i32 = 97;  // 'a'
const ASCII_B: i32 = 98;        // 'b'
const ASCII_F_LOWER: i32 = 102; // 'f'
const ASCII_L: i32 = 108;       // 'l'
const ASCII_N: i32 = 110;       // 'n'
const ASCII_R: i32 = 114;       // 'r'
const ASCII_S: i32 = 115;       // 's'
const ASCII_T: i32 = 116;       // 't'
const ASCII_U: i32 = 117;       // 'u'
const ASCII_LBRACE: i32 = 123;  // '{'
const ASCII_RBRACE: i32 = 125;  // '}'
const ASCII_E_LOWER: i32 = 101; // 'e'

// Bit mask used to fold uppercase A-F → a-f: byte | ASCII_LOWER_BIT.
const ASCII_LOWER_BIT: i32 = 32; // 0x20
// Bias for hex-digit decoding of a-f/A-F: (byte | ASCII_LOWER_BIT) - HEX_ALPHA_BIAS = 10..15.
const HEX_ALPHA_BIAS: i32 = 87;  // 'a' - 10
// Shift applied to a hex nibble when accumulating a 4-hex-digit codepoint.
const HEX_NIBBLE_SHIFT: i32 = 4;

// JSON keyword lengths (ASCII chars consumed after matching the first character).
const JSON_KW4_LOOKAHEAD: i32 = 3; // need 3 more chars for a 4-char keyword (null/true)
const JSON_KW4_LEN: i32 = 4;       // advance pos past "null" or "true" (4 chars)
const JSON_KW5_LOOKAHEAD: i32 = 4; // need 4 more chars for "false" (alse)
const JSON_FALSE_LEN: i32 = 5;     // advance pos past "false" (5 chars)

// Unicode / surrogate-pair constants.
const SURR_HIGH_START: i32 = 0xD800;  // first high surrogate
const SURR_HIGH_END: i32 = 0xDBFF;    // last  high surrogate
const SURR_LOW_START: i32 = 0xDC00;   // first low surrogate
const SURR_LOW_END: i32 = 0xDFFF;     // last  low surrogate
const SURR_HIGH_SHIFT: i32 = 10;      // high-surrogate left-shift in astral codepoint formula
const SURR_PAIR_BASE: i32 = 0x10000;  // codepoint base added in astral formula
const UNICODE_ESCAPE_LEN: i32 = 5;    // chars consumed for first  \uXXXX: 'u' + 4 hex
const UNICODE_ESCAPE_PAIR_ADV: i32 = 6; // chars consumed for second \uYYYY: '\' + 'u' + 4 hex

// UTF-8 encoding thresholds (exclusive upper bounds per byte-count).
const UTF8_1BYTE_MAX: i32 = 0x80;     // codepoints < 0x80 encode as 1 byte
const UTF8_2BYTE_MAX: i32 = 0x800;    // codepoints < 0x800 encode as 2 bytes
const UTF8_3BYTE_MAX: i32 = 0x10000;  // codepoints < 0x10000 encode as 3 bytes

// Decimal radix used in i64 digit-accumulation loops (integer and float mantissa).
const DECIMAL_RADIX: i64 = 10;

/// Emit in-place negative-index normalization, mirroring the native oracle
/// `if i < 0 { len as i64 + i }` (runtime/rs/src/json.rs get/set/remove paths):
/// `if idx_local < 0 { idx_local += len_local }`. After this, the standard
/// `idx < 0 || idx >= len` bounds check rejects an index that is still negative
/// (e.g. -5 over len 3), so an out-of-range normalized index stays a no-op.
fn emit_normalize_neg_index(f: &mut Function, idx_local: u32, len_local: u32) {
    wasm!(f, {
        local_get(Local(idx_local)); i32_const(Imm32(0)); i32_lt_s;
        if_empty;
          local_get(Local(idx_local)); local_get(Local(len_local)); i32_add; local_set(Local(idx_local));
        end;
    });
}

/// Emit a freshly-allocated empty-object value `{}` and leave its pointer in
/// `dst_local` (a scratch i32 local).
///
/// This is the wasm mirror of the native autovivification seed: every native
/// `set_at_steps` branch that descends into a missing key OR a non-object node
/// recurses with `&Value::Object(vec![])` (runtime/rs/src/json.rs:284,288), and
/// an Index step over that empty object is a local no-op that yields `{}`
/// (`j.clone()`, json.rs:299). Seeding the forward-walk placeholder with `{}`
/// rather than `null` makes a following Index step rebuild as `{}` (not `null`)
/// and a following Field step append into the empty pairs list — byte-matching
/// native for both Field-over-non-object and Field-then-Index autoviv chains.
///
/// Layout: pairs list is `[len=0]` (header only, no slots) → `alloc(list_hdr())`;
/// the value box is `[tag=VTAG_OBJECT][pairs_ptr]` → `alloc(VALUE_BOX_SIZE)`.
fn emit_make_empty_object(f: &mut Function, dst_local: u32, alloc: u32) {
    // scratch local 15 holds the pairs list while we build the value box; both
    // 15 and dst are clobbered deterministically here.
    wasm!(f, {
        i32_const(Imm32(list_hdr())); call(alloc); local_set(Local(15)); // empty pairs list
        local_get(Local(15)); i32_const(Imm32(0)); i32_store(0);          // len = 0
        i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(dst_local));
        local_get(Local(dst_local)); i32_const(Imm32(VTAG_OBJECT)); i32_store(0);
        local_get(Local(dst_local)); local_get(Local(15)); i32_store(4);
    });
}

/// Register runtime function signatures.
pub(super) fn register(emitter: &mut WasmEmitter) {
    let ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.value_stringify = emitter.register_func("__value_stringify", ty);

    let ty2 = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.json_parse = emitter.register_func("__json_parse", ty2);

    // __json_parse_at(str_ptr: i32, pos: i32) -> i32
    let parse_at_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.json_parse_at = emitter.register_func("__json_parse_at", parse_at_ty);

    // __json_get_path(value: i32, path: i32) -> i32 (Option[Value]: 0=none, ptr=some)
    let gp_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.json_get_path = emitter.register_func("__json_get_path", gp_ty);

    // __json_set_path(value: i32, path: i32, new_val: i32) -> i32 (Result[Value, String])
    let sp_ty = emitter.register_type(vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.json_set_path = emitter.register_func("__json_set_path", sp_ty);

    // __json_remove_path(value: i32, path: i32) -> i32 (Value)
    let rp_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.json_remove_path = emitter.register_func("__json_remove_path", rp_ty);

    // Register at end to avoid shifting existing function indices
    let esc_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.json_escape_string = emitter.register_func("__json_escape_string", esc_ty);

    // __json_stringify_pretty(v: i32, depth: i32) -> i32 (String ptr)
    let pretty_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.json_stringify_pretty = emitter.register_func("__json_stringify_pretty", pretty_ty);
}

/// Compile all runtime function bodies.
pub(super) fn compile(emitter: &mut WasmEmitter) {
    compile_value_stringify(emitter);
    compile_json_parse(emitter);
    compile_json_get_path(emitter);
    compile_json_set_path(emitter);
    compile_json_remove_path(emitter);
    compile_json_escape_string(emitter);
    // MUST be compiled in the same order it was registered (last) — the emitter
    // matches compiled bodies to registered func indices positionally (#526).
    compile_json_stringify_pretty(emitter);
}

/// __json_escape_string(str_ptr: i32) -> i32
/// Escapes \, ", \n, \t, \r in a string for JSON output.
/// Uses string.replace chain for simplicity and correctness.
fn compile_json_escape_string(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.json_escape_string];

    // Source chars to escape (single-char strings)
    let backslash_char = emitter.intern_string("\\");
    let quote_char = emitter.intern_string("\"");
    let newline_char = emitter.intern_string("\n");
    let tab_char = emitter.intern_string("\t");
    let cr_char = emitter.intern_string("\r");

    // Replacement sequences (two-char strings)
    let esc_backslash = emitter.intern_string("\\\\");
    let esc_quote = emitter.intern_string("\\\"");
    let esc_newline = emitter.intern_string("\\n");
    let esc_tab = emitter.intern_string("\\t");
    let esc_cr = emitter.intern_string("\\r");

    let replace = emitter.rt.string.replace;

    // Chain: replace(\, \\) → replace(", \") → replace(\n, \\n) → replace(\t, \\t) → replace(\r, \\r)
    // Order matters: backslash first to avoid double-escaping
    // local 0 = input, local 1 = result
    let mut f = Function::new([(1, ValType::I32)]);
    wasm!(f, { local_get(Local(0)); local_set(Local(1)); });
    // Replace \ first (before others to avoid double-escaping)
    wasm!(f, { local_get(Local(1)); i32_const(Imm32(backslash_char as i32)); i32_const(Imm32(esc_backslash as i32)); call(replace); local_set(Local(1)); });
    wasm!(f, { local_get(Local(1)); i32_const(Imm32(quote_char as i32)); i32_const(Imm32(esc_quote as i32)); call(replace); local_set(Local(1)); });
    wasm!(f, { local_get(Local(1)); i32_const(Imm32(newline_char as i32)); i32_const(Imm32(esc_newline as i32)); call(replace); local_set(Local(1)); });
    wasm!(f, { local_get(Local(1)); i32_const(Imm32(tab_char as i32)); i32_const(Imm32(esc_tab as i32)); call(replace); local_set(Local(1)); });
    wasm!(f, { local_get(Local(1)); i32_const(Imm32(cr_char as i32)); i32_const(Imm32(esc_cr as i32)); call(replace); local_set(Local(1)); });
    wasm!(f, { local_get(Local(1)); end; });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.json_escape_string, type_idx, f));
}

/// __value_stringify(v: i32) -> i32
fn compile_value_stringify(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.value_stringify];

    let null_str = emitter.intern_string("null");
    let true_str = emitter.intern_string("true");
    let false_str = emitter.intern_string("false");
    let quote_str = emitter.intern_string("\"");
    let open_bracket = emitter.intern_string("[");
    let close_bracket = emitter.intern_string("]");
    let comma_str = emitter.intern_string(",");
    let open_brace = emitter.intern_string("{");
    let close_brace = emitter.intern_string("}");
    let colon_str = emitter.intern_string(":");
    let empty_arr_str = emitter.intern_string("[]");
    let empty_obj_str = emitter.intern_string("{}");

    let concat = emitter.rt.concat_str;
    let itoa = emitter.rt.int_to_string;
    // Floats render in native `format!("{}", f)` (Display) form — which drops the
    // trailing `.0` of an integer-valued float — to match `almide_rt_value_stringify`
    // (the native oracle) byte-for-byte. `float_to_string` (the round-trip form)
    // keeps `3.0` and so diverged: `value.stringify` / a `Value` float repr now
    // agree native == wasm.
    let fdisp = emitter.rt.float_display;
    let stringify_fn = emitter.rt.value_stringify;

    // Locals: param 0 = v
    // 1=tag, 2=result, 3=list_ptr, 4=len, 5=i, 6=elem_str, 7=tmp
    let mut f = Function::new([(7, ValType::I32)]);

    // Load tag
    wasm!(f, { local_get(Local(0)); i32_load(0); local_set(Local(1)); });

    // Tag 0: null
    wasm!(f, {
        local_get(Local(1)); i32_eqz;
        if_empty; i32_const(Imm32(null_str as i32)); return_; end;
    });

    // Tag 1: bool
    wasm!(f, {
        local_get(Local(1)); i32_const(Imm32(1)); i32_eq;
        if_empty;
          local_get(Local(0)); i32_load(4);
          if_i32; i32_const(Imm32(true_str as i32));
          else_; i32_const(Imm32(false_str as i32)); end;
          return_;
        end;
    });

    // Tag 2: int
    wasm!(f, {
        local_get(Local(1)); i32_const(Imm32(VTAG_INT)); i32_eq;
        if_empty;
          local_get(Local(0)); i64_load(4); call(itoa); return_;
        end;
    });

    // Tag 3: float
    wasm!(f, {
        local_get(Local(1)); i32_const(Imm32(VTAG_FLOAT)); i32_eq;
        if_empty;
          local_get(Local(0)); f64_load(4); call(fdisp); return_;
        end;
    });

    // Tag 4: string -> "\"" + escape(s) + "\""
    let escape_fn = emitter.rt.json_escape_string;
    wasm!(f, {
        local_get(Local(1)); i32_const(Imm32(VTAG_STRING)); i32_eq;
        if_empty;
          i32_const(Imm32(quote_str as i32));
          local_get(Local(0)); i32_load(4); call(escape_fn);
          call(concat);
          i32_const(Imm32(quote_str as i32));
          call(concat);
          return_;
        end;
    });

    // Tag 5: array
    wasm!(f, {
        local_get(Local(1)); i32_const(Imm32(VTAG_ARRAY)); i32_eq;
        if_empty;
          local_get(Local(0)); i32_load(4); local_set(Local(3));
          local_get(Local(3)); i32_load(0); local_set(Local(4));
          local_get(Local(4)); i32_eqz;
          if_empty; i32_const(Imm32(empty_arr_str as i32)); return_; end;
          i32_const(Imm32(open_bracket as i32)); local_set(Local(2));
          i32_const(Imm32(0)); local_set(Local(5));
    });
    wasm!(f, {
          block_empty; loop_empty;
            local_get(Local(5)); local_get(Local(4)); i32_ge_u; br_if(1);
            local_get(Local(3)); i32_const(Imm32(list_data_off())); i32_add;
            local_get(Local(5)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            i32_load(0); call(stringify_fn); local_set(Local(6));
            local_get(Local(5)); i32_const(Imm32(0)); i32_gt_u;
            if_empty;
              local_get(Local(2)); i32_const(Imm32(comma_str as i32)); call(concat); local_set(Local(2));
            end;
            local_get(Local(2)); local_get(Local(6)); call(concat); local_set(Local(2));
            local_get(Local(5)); i32_const(Imm32(1)); i32_add; local_set(Local(5));
            br(0);
          end; end;
          local_get(Local(2)); i32_const(Imm32(close_bracket as i32)); call(concat); return_;
        end;
    });

    // Tag 6: object
    wasm!(f, {
        local_get(Local(1)); i32_const(Imm32(VTAG_OBJECT)); i32_eq;
        if_empty;
          local_get(Local(0)); i32_load(4); local_set(Local(3));
          local_get(Local(3)); i32_load(0); local_set(Local(4));
          local_get(Local(4)); i32_eqz;
          if_empty; i32_const(Imm32(empty_obj_str as i32)); return_; end;
          i32_const(Imm32(open_brace as i32)); local_set(Local(2));
          i32_const(Imm32(0)); local_set(Local(5));
    });
    wasm!(f, {
          block_empty; loop_empty;
            local_get(Local(5)); local_get(Local(4)); i32_ge_u; br_if(1);
            // Load tuple pointer: list[8 + i*4] (each list elem is an i32 ptr)
            local_get(Local(3)); i32_const(Imm32(list_data_off())); i32_add;
            local_get(Local(5)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            i32_load(0); // dereference to get tuple ptr
            local_set(Local(6));
            local_get(Local(5)); i32_const(Imm32(0)); i32_gt_u;
            if_empty;
              local_get(Local(2)); i32_const(Imm32(comma_str as i32)); call(concat); local_set(Local(2));
            end;
    });
    wasm!(f, {
            // tuple layout: [key_str_ptr: i32][value_ptr: i32]
            i32_const(Imm32(quote_str as i32));
            local_get(Local(6)); i32_load(0); // key string ptr
            call(concat);
            i32_const(Imm32(quote_str as i32)); call(concat);
            i32_const(Imm32(colon_str as i32)); call(concat);
            local_get(Local(6)); i32_load(4); call(stringify_fn); // value ptr
            call(concat); local_set(Local(7));
            local_get(Local(2)); local_get(Local(7)); call(concat); local_set(Local(2));
            local_get(Local(5)); i32_const(Imm32(1)); i32_add; local_set(Local(5));
            br(0);
          end; end;
          local_get(Local(2)); i32_const(Imm32(close_brace as i32)); call(concat); return_;
        end;
    });

    // Fallback
    wasm!(f, { i32_const(Imm32(null_str as i32)); end; });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.value_stringify, type_idx, f));
}

/// __json_stringify_pretty(v: i32, depth: i32) -> i32 (String ptr)
///
/// Mirrors native runtime/rs/src/json.rs `stringify_value(v, depth)`:
///   2-space indent per depth level; arrays render
///   `[\n{ind1}elem,\n...{ind}]`, objects `{\n{ind1}"k": v,\n...{ind}}`,
///   empty containers collapse to `[]`/`{}`, and ALL scalars (null/bool/int/
///   float/string) delegate to __value_stringify so escape/number formatting
///   is byte-identical with the compact path (and with native's common case).
fn compile_json_stringify_pretty(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.json_stringify_pretty];

    let two_sp = emitter.intern_string("  ");
    let nl_str = emitter.intern_string("\n");
    let comma_nl = emitter.intern_string(",\n");
    let open_bracket = emitter.intern_string("[");
    let close_bracket = emitter.intern_string("]");
    let open_brace = emitter.intern_string("{");
    let close_brace = emitter.intern_string("}");
    let quote_str = emitter.intern_string("\"");
    let colon_sp = emitter.intern_string(": ");
    let empty_arr_str = emitter.intern_string("[]");
    let empty_obj_str = emitter.intern_string("{}");

    let concat = emitter.rt.concat_str;
    let repeat = emitter.rt.string.repeat;
    let stringify_fn = emitter.rt.value_stringify;
    let pretty_fn = emitter.rt.json_stringify_pretty;
    let escape_fn = emitter.rt.json_escape_string;

    // Locals (params 0=v, 1=depth):
    //   2=tag, 3=result(acc), 4=ind, 5=ind1, 6=list_ptr, 7=len, 8=i,
    //   9=elem_str/tuple_ptr, 10=tmp
    let mut f = Function::new([(9, ValType::I32)]);

    // Load tag.
    wasm!(f, { local_get(Local(0)); i32_load(0); local_set(Local(2)); });

    // Scalars (tag <= 4): delegate to __value_stringify (byte-identical to
    // compact; native pretty also emits the same scalar text).
    wasm!(f, {
        local_get(Local(2)); i32_const(Imm32(VTAG_ARRAY)); i32_lt_u;
        if_empty;
          local_get(Local(0)); call(stringify_fn); return_;
        end;
    });

    // Build indentation strings: ind = "  ".repeat(depth), ind1 = "  ".repeat(depth+1).
    wasm!(f, {
        i32_const(Imm32(two_sp as i32)); local_get(Local(1)); call(repeat); local_set(Local(4));
        i32_const(Imm32(two_sp as i32)); local_get(Local(1)); i32_const(Imm32(1)); i32_add; call(repeat); local_set(Local(5));
    });

    // Tag 5: array.
    wasm!(f, {
        local_get(Local(2)); i32_const(Imm32(VTAG_ARRAY)); i32_eq;
        if_empty;
          local_get(Local(0)); i32_load(4); local_set(Local(6));
          local_get(Local(6)); i32_load(0); local_set(Local(7));
          local_get(Local(7)); i32_eqz;
          if_empty; i32_const(Imm32(empty_arr_str as i32)); return_; end;
          // result = "[\n"
          i32_const(Imm32(open_bracket as i32)); i32_const(Imm32(nl_str as i32)); call(concat); local_set(Local(3));
          i32_const(Imm32(0)); local_set(Local(8));
    });
    wasm!(f, {
          block_empty; loop_empty;
            local_get(Local(8)); local_get(Local(7)); i32_ge_u; br_if(1);
            // separator before all but the first element
            local_get(Local(8)); i32_const(Imm32(0)); i32_gt_u;
            if_empty;
              local_get(Local(3)); i32_const(Imm32(comma_nl as i32)); call(concat); local_set(Local(3));
            end;
            // result += ind1
            local_get(Local(3)); local_get(Local(5)); call(concat); local_set(Local(3));
            // result += pretty(elem, depth+1)
            local_get(Local(6)); i32_const(Imm32(list_data_off())); i32_add;
            local_get(Local(8)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            i32_load(0);
            local_get(Local(1)); i32_const(Imm32(1)); i32_add;
            call(pretty_fn); local_set(Local(9));
            local_get(Local(3)); local_get(Local(9)); call(concat); local_set(Local(3));
            local_get(Local(8)); i32_const(Imm32(1)); i32_add; local_set(Local(8));
            br(0);
          end; end;
          // result += "\n" + ind + "]"
          local_get(Local(3)); i32_const(Imm32(nl_str as i32)); call(concat); local_set(Local(3));
          local_get(Local(3)); local_get(Local(4)); call(concat); local_set(Local(3));
          local_get(Local(3)); i32_const(Imm32(close_bracket as i32)); call(concat); return_;
        end;
    });

    // Tag 6: object.
    wasm!(f, {
        local_get(Local(2)); i32_const(Imm32(VTAG_OBJECT)); i32_eq;
        if_empty;
          local_get(Local(0)); i32_load(4); local_set(Local(6));
          local_get(Local(6)); i32_load(0); local_set(Local(7));
          local_get(Local(7)); i32_eqz;
          if_empty; i32_const(Imm32(empty_obj_str as i32)); return_; end;
          i32_const(Imm32(open_brace as i32)); i32_const(Imm32(nl_str as i32)); call(concat); local_set(Local(3));
          i32_const(Imm32(0)); local_set(Local(8));
    });
    wasm!(f, {
          block_empty; loop_empty;
            local_get(Local(8)); local_get(Local(7)); i32_ge_u; br_if(1);
            // load tuple ptr [key_str_ptr][value_ptr]
            local_get(Local(6)); i32_const(Imm32(list_data_off())); i32_add;
            local_get(Local(8)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            i32_load(0); local_set(Local(9));
            local_get(Local(8)); i32_const(Imm32(0)); i32_gt_u;
            if_empty;
              local_get(Local(3)); i32_const(Imm32(comma_nl as i32)); call(concat); local_set(Local(3));
            end;
            // result += ind1
            local_get(Local(3)); local_get(Local(5)); call(concat); local_set(Local(3));
            // result += "\"" + escape(key) + "\"" + ": "
            local_get(Local(3)); i32_const(Imm32(quote_str as i32)); call(concat);
            local_get(Local(9)); i32_load(0); call(escape_fn); call(concat);
            i32_const(Imm32(quote_str as i32)); call(concat);
            i32_const(Imm32(colon_sp as i32)); call(concat); local_set(Local(3));
            // result += pretty(value, depth+1)
            local_get(Local(9)); i32_load(4);
            local_get(Local(1)); i32_const(Imm32(1)); i32_add;
            call(pretty_fn); local_set(Local(10));
            local_get(Local(3)); local_get(Local(10)); call(concat); local_set(Local(3));
            local_get(Local(8)); i32_const(Imm32(1)); i32_add; local_set(Local(8));
            br(0);
          end; end;
          local_get(Local(3)); i32_const(Imm32(nl_str as i32)); call(concat); local_set(Local(3));
          local_get(Local(3)); local_get(Local(4)); call(concat); local_set(Local(3));
          local_get(Local(3)); i32_const(Imm32(close_brace as i32)); call(concat); return_;
        end;
    });

    // Fallback (unreachable for valid Value tags): delegate to compact.
    wasm!(f, { local_get(Local(0)); call(stringify_fn); end; });

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// __json_parse(s: i32) -> i32 (Result[Value, String])
fn compile_json_parse(emitter: &mut WasmEmitter) {
    let parse_at_fn = emitter.rt.json_parse_at;

    let type_idx = emitter.func_type_indices[&emitter.rt.json_parse];
    let alloc = emitter.rt.alloc;

    // param 0 = s, local 1 = parse_result, local 2 = result
    let mut f = Function::new([(2, ValType::I32)]);

    wasm!(f, {
        local_get(Local(0)); i32_const(Imm32(0)); call(parse_at_fn); local_set(Local(1));
        local_get(Local(1)); i32_load(8);
        if_i32;
          i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(2));
          local_get(Local(2)); i32_const(Imm32(1)); i32_store(0);
          local_get(Local(2)); local_get(Local(1)); i32_load(0); i32_store(4);
          local_get(Local(2));
        else_;
          i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(2));
          local_get(Local(2)); i32_const(Imm32(0)); i32_store(0);
          local_get(Local(2)); local_get(Local(1)); i32_load(0); i32_store(4);
          local_get(Local(2));
        end;
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.json_parse, type_idx, f));

    compile_json_parse_at(emitter);
}

/// __json_parse_at(str_ptr: i32, pos: i32) -> i32
/// Returns ptr to [value_or_err: i32][new_pos: i32][err_flag: i32]
fn compile_json_parse_at(emitter: &mut WasmEmitter) {
    let parse_at_fn = emitter.rt.json_parse_at;
    let type_idx = emitter.func_type_indices[&parse_at_fn];
    let alloc = emitter.rt.alloc;
    let string_alloc = emitter.rt.string_alloc;
    let _concat = emitter.rt.concat_str;
    let _str_eq = emitter.rt.string.eq;
    // The number parser hands a float token to the SAME correctly-rounded parser
    // float.parse uses (__dec2flt), via a slice of the source (#663 / #667).
    let str_slice = emitter.rt.string.slice;
    let float_parse = emitter.rt.float_parse;

    let err_msg = emitter.intern_string("unexpected character in JSON");
    let err_eof = emitter.intern_string("unexpected end of input");

    // Locals:
    // param 0 = str_ptr, param 1 = pos
    // 2=result_ptr, 3=str_len, 4=ch, 5=start, 6=value_ptr, 7=tmp
    // 8=list_ptr, 9=count, 10=sub_result, 11=sign
    // 12=num_val(i64), 13=divisor(f64)
    // 14=capacity, 15=old_buf_save (for growable array/object parsing)
    let mut f = Function::new([
        (10, ValType::I32),
        (1, ValType::I64),
        (1, ValType::F64),
        (2, ValType::I32), // local 14 = capacity, local 15 = old_buf_save
    ]);

    // Allocate result struct (12 bytes)
    wasm!(f, {
        i32_const(Imm32(PARSE_RESULT_SIZE)); call(alloc); local_set(Local(2));
        local_get(Local(0)); i32_load(0); local_set(Local(3));
    });

    // Skip whitespace
    emit_skip_ws(&mut f);

    // Check EOF
    wasm!(f, {
        local_get(Local(1)); local_get(Local(3)); i32_ge_u;
        if_empty;
          local_get(Local(2)); i32_const(Imm32(err_eof as i32)); i32_store(0);
          local_get(Local(2)); i32_const(Imm32(0)); i32_store(4);
          local_get(Local(2)); i32_const(Imm32(1)); i32_store(8);
          local_get(Local(2)); return_;
        end;
    });

    // Load current char
    wasm!(f, {
        local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
        i32_load8_u(0); local_set(Local(4));
    });

    // ── null: check n,u,l,l ──
    wasm!(f, {
        local_get(Local(4)); i32_const(Imm32(ASCII_N)); i32_eq; // 'n'
        if_empty;
          // Validate remaining chars: u(117), l(108), l(108)
          local_get(Local(1)); i32_const(Imm32(JSON_KW4_LOOKAHEAD)); i32_add; local_get(Local(3)); i32_lt_u; // need 3 more chars
          local_get(Local(0)); i32_const(Imm32(string_data_off() + 1)); i32_add; local_get(Local(1)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_U)); i32_eq;
          i32_and;
          local_get(Local(0)); i32_const(Imm32(string_data_off() + 2)); i32_add; local_get(Local(1)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_L)); i32_eq;
          i32_and;
          local_get(Local(0)); i32_const(Imm32(string_data_off() + 3)); i32_add; local_get(Local(1)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_L)); i32_eq;
          i32_and;
    });
    wasm!(f, {
          if_empty;
            local_get(Local(1)); i32_const(Imm32(JSON_KW4_LEN)); i32_add; local_set(Local(1));
            i32_const(Imm32(I32_BYTES)); call(alloc); local_set(Local(6));
            local_get(Local(6)); i32_const(Imm32(0)); i32_store(0);
            local_get(Local(2)); local_get(Local(6)); i32_store(0);
            local_get(Local(2)); local_get(Local(1)); i32_store(4);
            local_get(Local(2)); i32_const(Imm32(0)); i32_store(8);
            local_get(Local(2)); return_;
          end;
        end;
    });

    // ── true: check t,r,u,e ──
    wasm!(f, {
        local_get(Local(4)); i32_const(Imm32(ASCII_T)); i32_eq; // 't'
        if_empty;
          local_get(Local(1)); i32_const(Imm32(JSON_KW4_LOOKAHEAD)); i32_add; local_get(Local(3)); i32_lt_u;
          local_get(Local(0)); i32_const(Imm32(string_data_off() + 1)); i32_add; local_get(Local(1)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_R)); i32_eq;
          i32_and;
          local_get(Local(0)); i32_const(Imm32(string_data_off() + 2)); i32_add; local_get(Local(1)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_U)); i32_eq;
          i32_and;
          local_get(Local(0)); i32_const(Imm32(string_data_off() + 3)); i32_add; local_get(Local(1)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_E_LOWER)); i32_eq;
          i32_and;
    });
    wasm!(f, {
          if_empty;
            local_get(Local(1)); i32_const(Imm32(JSON_KW4_LEN)); i32_add; local_set(Local(1));
            i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(6));
            local_get(Local(6)); i32_const(Imm32(1)); i32_store(0);
            local_get(Local(6)); i32_const(Imm32(1)); i32_store(4);
            local_get(Local(2)); local_get(Local(6)); i32_store(0);
            local_get(Local(2)); local_get(Local(1)); i32_store(4);
            local_get(Local(2)); i32_const(Imm32(0)); i32_store(8);
            local_get(Local(2)); return_;
          end;
        end;
    });

    // ── false: check f,a,l,s,e ──
    wasm!(f, {
        local_get(Local(4)); i32_const(Imm32(ASCII_F_LOWER)); i32_eq; // 'f'
        if_empty;
          local_get(Local(1)); i32_const(Imm32(JSON_KW5_LOOKAHEAD)); i32_add; local_get(Local(3)); i32_lt_u;
          local_get(Local(0)); i32_const(Imm32(string_data_off() + 1)); i32_add; local_get(Local(1)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_A_LOWER)); i32_eq;
          i32_and;
          local_get(Local(0)); i32_const(Imm32(string_data_off() + 2)); i32_add; local_get(Local(1)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_L)); i32_eq;
          i32_and;
          local_get(Local(0)); i32_const(Imm32(string_data_off() + 3)); i32_add; local_get(Local(1)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_S)); i32_eq;
          i32_and;
          local_get(Local(0)); i32_const(Imm32(string_data_off() + 4)); i32_add; local_get(Local(1)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_E_LOWER)); i32_eq;
          i32_and;
    });
    wasm!(f, {
          if_empty;
            local_get(Local(1)); i32_const(Imm32(JSON_FALSE_LEN)); i32_add; local_set(Local(1));
            i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(6));
            local_get(Local(6)); i32_const(Imm32(1)); i32_store(0);
            local_get(Local(6)); i32_const(Imm32(0)); i32_store(4);
            local_get(Local(2)); local_get(Local(6)); i32_store(0);
            local_get(Local(2)); local_get(Local(1)); i32_store(4);
            local_get(Local(2)); i32_const(Imm32(0)); i32_store(8);
            local_get(Local(2)); return_;
          end;
        end;
    });

    // ── String ──
    emit_parse_string(&mut f, alloc, string_alloc);

    // ── Number ──
    emit_parse_number(&mut f, alloc, str_slice, float_parse);

    // ── Array ──
    emit_parse_array(&mut f, alloc, parse_at_fn);

    // ── Object ──
    emit_parse_object(&mut f, alloc, parse_at_fn);

    // Fallback: error
    wasm!(f, {
        local_get(Local(2)); i32_const(Imm32(err_msg as i32)); i32_store(0);
        local_get(Local(2)); i32_const(Imm32(0)); i32_store(4);
        local_get(Local(2)); i32_const(Imm32(1)); i32_store(8);
        local_get(Local(2));
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// Parse optional JSON exponent (e/E followed by optional +/- and digits).
/// Pushes f64 multiplier onto the stack: 10^exp (or 1.0 if no exponent).
/// Uses locals: 0=str_ptr, 1=pos, 3=str_len, 4=ch, 7=tmp, 12=num_val(i64), 13=divisor(f64)
fn emit_parse_exponent(f: &mut Function) {
    // Default multiplier = 1.0 (no exponent)
    wasm!(f, { f64_const(1.0); local_set(Local(13)); });

    // Check if we have e/E
    wasm!(f, {
        local_get(Local(1)); local_get(Local(3)); i32_lt_u;
        if_empty;
          local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
          i32_load8_u(0); local_set(Local(4));
          local_get(Local(4)); i32_const(Imm32(ASCII_E_LOWER)); i32_eq; // 'e'
          local_get(Local(4)); i32_const(Imm32(ASCII_E_UPPER)); i32_eq;  // 'E'
          i32_or;
          if_empty;
            local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
            // exp_sign: check +/-
            i32_const(Imm32(1)); local_set(Local(7)); // exp_sign = 1 (positive)
            local_get(Local(1)); local_get(Local(3)); i32_lt_u;
            if_empty;
              local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
              i32_load8_u(0); local_set(Local(4));
              local_get(Local(4)); i32_const(Imm32(ASCII_MINUS)); i32_eq; // '-'
              if_empty;
                i32_const(Imm32(-1)); local_set(Local(7));
                local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
              else_;
                local_get(Local(4)); i32_const(Imm32(ASCII_PLUS)); i32_eq; // '+'
                if_empty;
                  local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
                end;
              end;
            end;
            // Parse exponent digits
            i64_const(Imm64(0)); local_set(Local(12));
    });
    wasm!(f, {
            block_empty; loop_empty;
              local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
              local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
              i32_load8_u(0); local_set(Local(4));
              local_get(Local(4)); i32_const(Imm32(ASCII_ZERO)); i32_lt_u; br_if(1);
              local_get(Local(4)); i32_const(Imm32(ASCII_NINE)); i32_gt_u; br_if(1);
              local_get(Local(12)); i64_const(Imm64(DECIMAL_RADIX)); i64_mul;
              local_get(Local(4)); i32_const(Imm32(ASCII_ZERO)); i32_sub; i64_extend_i32_u;
              i64_add; local_set(Local(12));
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
              br(0);
            end; end;
    });
    // Compute multiplier = 10^exp via loop, store in local 13
    wasm!(f, {
            f64_const(1.0); local_set(Local(13));
            block_empty; loop_empty;
              local_get(Local(12)); i64_eqz; br_if(1);
              local_get(Local(7)); i32_const(Imm32(0)); i32_lt_s;
              if_empty;
                local_get(Local(13)); f64_const(0.1); f64_mul; local_set(Local(13));
              else_;
                local_get(Local(13)); f64_const(10.0); f64_mul; local_set(Local(13));
              end;
              local_get(Local(12)); i64_const(Imm64(1)); i64_sub; local_set(Local(12));
              br(0);
            end; end;
          end; // if e/E
        end; // if pos < len
    });

    // Push multiplier onto stack
    wasm!(f, { local_get(Local(13)); });
}

/// Emit whitespace-skipping loop.
/// Uses locals: 0=str_ptr, 1=pos, 3=str_len, 4=ch
fn emit_skip_ws(f: &mut Function) {
    wasm!(f, {
        block_empty; loop_empty;
          local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
          local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
          i32_load8_u(0); local_set(Local(4));
          local_get(Local(4)); i32_const(Imm32(ASCII_SPACE)); i32_eq;
          local_get(Local(4)); i32_const(Imm32(ASCII_HT)); i32_eq; i32_or;
          local_get(Local(4)); i32_const(Imm32(ASCII_LF)); i32_eq; i32_or;
          local_get(Local(4)); i32_const(Imm32(ASCII_CR)); i32_eq; i32_or;
          i32_eqz; br_if(1);
          local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
          br(0);
        end; end;
    });
}

/// Decode 4 hex digits at `[str_ptr + data + start(5) + read_off(8) + off0 .. +4)`
/// into `target` (an i32 local). Uppercase and lowercase A–F both decode. Used
/// for `\uXXXX` escapes and their low-surrogate continuation (#651). Local 11 is
/// scratch.
fn emit_parse_hex4(f: &mut Function, off0: i32, target: u32) {
    wasm!(f, { i32_const(Imm32(0)); local_set(Local(target)); });
    for k in off0..off0 + 4 {
        wasm!(f, {
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(5)); i32_add; local_get(Local(8)); i32_add; i32_const(Imm32(k)); i32_add;
            i32_load8_u(0); local_set(Local(11));
            // digit = byte < ':' ? byte - '0' : (byte | 0x20) - ('a' - 10)
            local_get(Local(11)); i32_const(Imm32(ASCII_COLON)); i32_lt_u;
            if_i32;
              local_get(Local(11)); i32_const(Imm32(ASCII_ZERO)); i32_sub;
            else_;
              local_get(Local(11)); i32_const(Imm32(ASCII_LOWER_BIT)); i32_or; i32_const(Imm32(HEX_ALPHA_BIAS)); i32_sub;
            end;
            local_get(Local(target)); i32_const(Imm32(HEX_NIBBLE_SHIFT)); i32_shl; i32_add; local_set(Local(target));
        });
    }
}

/// Emit `out[write_off(9)] = (cp(10) >> shift & mask) | high; write_off += 1`.
/// One UTF-8 byte derived from codepoint local 10. For a continuation byte:
/// `high=0x80, mask=0x3F`; for a lead byte the high prefix and a wider mask. The
/// 1-byte (ASCII) case uses `shift=0, mask=0x7F, high=0`. `out` base is local 6.
fn emit_utf8_byte(f: &mut Function, shift: i32, mask: i32, high: i32) {
    wasm!(f, {
        local_get(Local(6)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(9)); i32_add;
        local_get(Local(10)); i32_const(Imm32(shift)); i32_shr_u; i32_const(Imm32(mask)); i32_and; i32_const(Imm32(high)); i32_or;
        i32_store8(0);
        local_get(Local(9)); i32_const(Imm32(1)); i32_add; local_set(Local(9));
    });
}

/// `\uXXXX` (plus a high+low surrogate pair) → UTF-8 bytes, then continue the
/// copy loop. Assumes the escape char is in local 4, read_off (8) points at the
/// `u`, and we are nested inside `loop → backslash-if` so `br(2)` re-enters the
/// loop — bypassing the single-byte write that handles the simple escapes. The
/// simple-escape rewrites that follow this call run only when the char is not
/// `u`. Mirrors serde_json's `\u` decoding. #651.
fn emit_unicode_escape_branch(f: &mut Function) {
    wasm!(f, {
        local_get(Local(4)); i32_const(Imm32(ASCII_U)); i32_eq; // 'u'
        if_empty;
    });
    // codepoint (local 10) from the 4 hex following the `u`.
    emit_parse_hex4(f, 1, 10);
    wasm!(f, { local_get(Local(8)); i32_const(Imm32(UNICODE_ESCAPE_LEN)); i32_add; local_set(Local(8)); }); // consumed `u` + 4 hex
    // Surrogate pair: a high surrogate (D800..DBFF) immediately followed by a
    // "\uYYYY" low surrogate (DC00..DFFF) forms one astral codepoint.
    wasm!(f, {
        local_get(Local(10)); i32_const(Imm32(SURR_HIGH_START)); i32_ge_u;
        local_get(Local(10)); i32_const(Imm32(SURR_HIGH_END)); i32_le_u;
        i32_and;
        if_empty;
          local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(5)); i32_add; local_get(Local(8)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_BACKSLASH)); i32_eq;
          local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(5)); i32_add; local_get(Local(8)); i32_add; i32_const(Imm32(1)); i32_add; i32_load8_u(0); i32_const(Imm32(ASCII_U)); i32_eq;
          i32_and;
          if_empty;
    });
    emit_parse_hex4(f, 2, 14); // low surrogate → local 14
    wasm!(f, {
            local_get(Local(14)); i32_const(Imm32(SURR_LOW_START)); i32_ge_u;
            local_get(Local(14)); i32_const(Imm32(SURR_LOW_END)); i32_le_u;
            i32_and;
            if_empty;
              local_get(Local(10)); i32_const(Imm32(SURR_HIGH_START)); i32_sub; i32_const(Imm32(SURR_HIGH_SHIFT)); i32_shl;
              local_get(Local(14)); i32_const(Imm32(SURR_LOW_START)); i32_sub; i32_add;
              i32_const(Imm32(SURR_PAIR_BASE)); i32_add; local_set(Local(10));
              local_get(Local(8)); i32_const(Imm32(UNICODE_ESCAPE_PAIR_ADV)); i32_add; local_set(Local(8)); // consumed "\uYYYY"
            end;
          end;
        end;
    });
    // UTF-8 encode cp(10): 1 byte (<0x80), 2 (<0x800), 3 (<0x10000), else 4.
    wasm!(f, { local_get(Local(10)); i32_const(Imm32(UTF8_1BYTE_MAX)); i32_lt_u; if_empty; });
    emit_utf8_byte(f, 0, 0x7F, 0x00);
    wasm!(f, { else_; local_get(Local(10)); i32_const(Imm32(UTF8_2BYTE_MAX)); i32_lt_u; if_empty; });
    emit_utf8_byte(f, 6, 0x1F, 0xC0);
    emit_utf8_byte(f, 0, 0x3F, 0x80);
    wasm!(f, { else_; local_get(Local(10)); i32_const(Imm32(UTF8_3BYTE_MAX)); i32_lt_u; if_empty; });
    emit_utf8_byte(f, 12, 0x0F, 0xE0);
    emit_utf8_byte(f, 6, 0x3F, 0x80);
    emit_utf8_byte(f, 0, 0x3F, 0x80);
    wasm!(f, { else_; });
    emit_utf8_byte(f, 18, 0x07, 0xF0);
    emit_utf8_byte(f, 12, 0x3F, 0x80);
    emit_utf8_byte(f, 6, 0x3F, 0x80);
    emit_utf8_byte(f, 0, 0x3F, 0x80);
    wasm!(f, {
            end; end; end; // close the <0x80 / <0x800 / <0x10000 chain
        br(2);             // continue the copy loop; read_off already advanced
        end;               // close the `u`-if
    });
}

/// Parse JSON string starting at current pos (ch=='"').
/// Uses locals: 0=str_ptr, 1=pos, 2=result_ptr, 3=str_len, 4=ch, 5=start, 6=value_ptr, 7=tmp, 9=count
fn emit_parse_string(f: &mut Function, alloc: u32, string_alloc: u32) {
    wasm!(f, {
        local_get(Local(4)); i32_const(Imm32(ASCII_DQUOTE)); i32_eq;
        if_empty;
          local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
          local_get(Local(1)); local_set(Local(5));
    });
    // Find closing quote
    wasm!(f, {
          block_empty; loop_empty;
            local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
            i32_load8_u(0); local_set(Local(7));
            local_get(Local(7)); i32_const(Imm32(ASCII_DQUOTE)); i32_eq; br_if(1);
            local_get(Local(7)); i32_const(Imm32(ASCII_BACKSLASH)); i32_eq;
            if_empty;
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
            end;
            local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
            br(0);
          end; end;
    });
    // Build string
    wasm!(f, {
          local_get(Local(1)); local_get(Local(5)); i32_sub; local_set(Local(7));
          local_get(Local(7)); call(string_alloc); local_set(Local(6));
          i32_const(Imm32(0)); local_set(Local(9));
    });
    // Copy bytes loop with JSON escape decoding.
    // Locals: 8 = read offset, 9 = write offset, 4 = current byte (reused).
    // Handles \n \t \r \" \\ \/ \b \f. \uXXXX is not yet supported (passes through).
    wasm!(f, {
          i32_const(Imm32(0)); local_set(Local(8));
          i32_const(Imm32(0)); local_set(Local(9));
          block_empty; loop_empty;
            local_get(Local(8)); local_get(Local(7)); i32_ge_u; br_if(1);
            // byte = in[in_base + read_off]
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(5)); i32_add; local_get(Local(8)); i32_add;
            i32_load8_u(0); local_set(Local(4));
            // if byte == 0x5C (backslash): decode next byte
            local_get(Local(4)); i32_const(Imm32(ASCII_BACKSLASH)); i32_eq;
            if_empty;
              // advance read past backslash
              local_get(Local(8)); i32_const(Imm32(1)); i32_add; local_set(Local(8));
              local_get(Local(8)); local_get(Local(7)); i32_ge_u; br_if(2);
              // load next byte
              local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(5)); i32_add; local_get(Local(8)); i32_add;
              i32_load8_u(0); local_set(Local(4));
    });
    // \uXXXX decodes to UTF-8 and continues the loop (multi-byte); the simple
    // single-byte escapes below run only when the char is not `u`. #651.
    emit_unicode_escape_branch(f);
    wasm!(f, {
              // Decode escape: overwrite local 4 in-place via if/else chain.
              // Decoded values (8,9,10,12,13) and idempotent ones (34,47,92) don't
              // collide with the source codes for other escapes (110,116,114,98,102),
              // so a sequential pass is safe.
              local_get(Local(4)); i32_const(Imm32(ASCII_N)); i32_eq; if_empty; i32_const(Imm32(ASCII_LF)); local_set(Local(4)); end;  // n
              local_get(Local(4)); i32_const(Imm32(ASCII_T)); i32_eq; if_empty; i32_const(Imm32(ASCII_HT));  local_set(Local(4)); end;  // t
              local_get(Local(4)); i32_const(Imm32(ASCII_R)); i32_eq; if_empty; i32_const(Imm32(ASCII_CR)); local_set(Local(4)); end;  // r
              local_get(Local(4)); i32_const(Imm32(ASCII_B));  i32_eq; if_empty; i32_const(Imm32(ASCII_BS));  local_set(Local(4)); end;  // b
              local_get(Local(4)); i32_const(Imm32(ASCII_F_LOWER)); i32_eq; if_empty; i32_const(Imm32(ASCII_FF)); local_set(Local(4)); end;  // f
              // ", \, /  decode to themselves — no rewrite needed.
            end;
            // out[out_base + write_off] = byte
            local_get(Local(6)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(9)); i32_add;
            local_get(Local(4));
            i32_store8(0);
            local_get(Local(9)); i32_const(Imm32(1)); i32_add; local_set(Local(9));
            local_get(Local(8)); i32_const(Imm32(1)); i32_add; local_set(Local(8));
            br(0);
          end; end;
    });
    // Build Value and return — write actual decoded length (write_off)
    wasm!(f, {
          local_get(Local(6)); local_get(Local(9)); i32_store(0);
          local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
          i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(7));
          local_get(Local(7)); i32_const(Imm32(VTAG_STRING)); i32_store(0);
          local_get(Local(7)); local_get(Local(6)); i32_store(4);
          local_get(Local(2)); local_get(Local(7)); i32_store(0);
          local_get(Local(2)); local_get(Local(1)); i32_store(4);
          local_get(Local(2)); i32_const(Imm32(0)); i32_store(8);
          local_get(Local(2)); return_;
        end;
    });
}

/// Parse JSON number. Uses: 0=str_ptr, 1=pos, 2=result_ptr, 3=str_len, 4=ch,
/// 5=start, 6=value_ptr, 11=sign, 12=num_val(i64), 13=divisor(f64)
fn emit_parse_number(f: &mut Function, alloc: u32, str_slice: u32, float_parse: u32) {
    // Check if number
    wasm!(f, {
        local_get(Local(4)); i32_const(Imm32(ASCII_MINUS)); i32_eq;
        local_get(Local(4)); i32_const(Imm32(ASCII_ZERO)); i32_ge_u;
        local_get(Local(4)); i32_const(Imm32(ASCII_NINE)); i32_le_u;
        i32_and; i32_or;
        if_empty;
          // Save the token start (incl. a leading '-') so a float token can be
          // re-parsed by the correctly-rounded float.parse (#663 / #667).
          local_get(Local(1)); local_set(Local(5));
          i32_const(Imm32(1)); local_set(Local(11));
          i64_const(Imm64(0)); local_set(Local(12));
          local_get(Local(4)); i32_const(Imm32(ASCII_MINUS)); i32_eq;
          if_empty;
            i32_const(Imm32(-1)); local_set(Local(11));
            local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
          end;
    });
    // Parse integer digits
    wasm!(f, {
          block_empty; loop_empty;
            local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
            i32_load8_u(0); local_set(Local(4));
            local_get(Local(4)); i32_const(Imm32(ASCII_ZERO)); i32_lt_u; br_if(1);
            local_get(Local(4)); i32_const(Imm32(ASCII_NINE)); i32_gt_u; br_if(1);
            local_get(Local(12)); i64_const(Imm64(DECIMAL_RADIX)); i64_mul;
            local_get(Local(4)); i32_const(Imm32(ASCII_ZERO)); i32_sub; i64_extend_i32_u;
            i64_add; local_set(Local(12));
            local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
            br(0);
          end; end;
    });
    // Check for decimal point
    wasm!(f, {
          local_get(Local(1)); local_get(Local(3)); i32_lt_u;
          if_empty;
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
            i32_load8_u(0); i32_const(Imm32(ASCII_DOT)); i32_eq;
            if_empty;
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
              f64_const(1.0); local_set(Local(13));
    });
    // Parse decimal digits
    wasm!(f, {
              block_empty; loop_empty;
                local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
                local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
                i32_load8_u(0); local_set(Local(4));
                local_get(Local(4)); i32_const(Imm32(ASCII_ZERO)); i32_lt_u; br_if(1);
                local_get(Local(4)); i32_const(Imm32(ASCII_NINE)); i32_gt_u; br_if(1);
                local_get(Local(12)); i64_const(Imm64(DECIMAL_RADIX)); i64_mul;
                local_get(Local(4)); i32_const(Imm32(ASCII_ZERO)); i32_sub; i64_extend_i32_u;
                i64_add; local_set(Local(12));
                local_get(Local(13)); f64_const(10.0); f64_mul; local_set(Local(13));
                local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
                br(0);
              end; end;
    });
    // Build float Value. The number token [start, pos) is handed to the SAME
    // correctly-rounded parser float.parse uses (__dec2flt) instead of an ad-hoc
    // `digits/divisor * 10^exp` that dropped the -0.0 sign (#663) and rounded
    // exponent forms off by a ULP (#667). emit_parse_exponent only advances pos
    // past the e/E digits here; its ad-hoc multiplier is dropped.
    emit_parse_exponent(f);
    wasm!(f, {
              drop;
              local_get(Local(0)); local_get(Local(5)); local_get(Local(1)); call(str_slice);
              call(float_parse); local_set(Local(7)); // Result[Float, String] ptr; valid JSON ⇒ ok
              i32_const(Imm32(PARSE_RESULT_SIZE)); call(alloc); local_set(Local(6));
              local_get(Local(6)); i32_const(Imm32(VTAG_FLOAT)); i32_store(0);
              local_get(Local(6)); local_get(Local(7)); f64_load(4); f64_store(4);
              local_get(Local(2)); local_get(Local(6)); i32_store(0);
              local_get(Local(2)); local_get(Local(1)); i32_store(4);
              local_get(Local(2)); i32_const(Imm32(0)); i32_store(8);
              local_get(Local(2)); return_;
            end;
          end;
    });
    // Integer path: check for exponent → becomes float
    wasm!(f, {
          // Check for e/E after integer
          local_get(Local(1)); local_get(Local(3)); i32_lt_u;
          if_empty;
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
            i32_load8_u(0); local_set(Local(4));
            local_get(Local(4)); i32_const(Imm32(ASCII_E_LOWER)); i32_eq; // 'e'
            local_get(Local(4)); i32_const(Imm32(ASCII_E_UPPER)); i32_eq;  // 'E'
            i32_or;
            if_empty;
              // Integer mantissa with an exponent suffix → a float. Re-parse the
              // whole token with the correctly-rounded float.parse (#667).
    });
    emit_parse_exponent(f);
    wasm!(f, {
              drop;
              local_get(Local(0)); local_get(Local(5)); local_get(Local(1)); call(str_slice);
              call(float_parse); local_set(Local(7));
              i32_const(Imm32(PARSE_RESULT_SIZE)); call(alloc); local_set(Local(6));
              local_get(Local(6)); i32_const(Imm32(VTAG_FLOAT)); i32_store(0);
              local_get(Local(6)); local_get(Local(7)); f64_load(4); f64_store(4);
              local_get(Local(2)); local_get(Local(6)); i32_store(0);
              local_get(Local(2)); local_get(Local(1)); i32_store(4);
              local_get(Local(2)); i32_const(Imm32(0)); i32_store(8);
              local_get(Local(2)); return_;
            end;
          end;
    });
    // Build int Value (no exponent)
    wasm!(f, {
          i32_const(Imm32(PARSE_RESULT_SIZE)); call(alloc); local_set(Local(6));
          local_get(Local(6)); i32_const(Imm32(VTAG_INT)); i32_store(0);
          local_get(Local(6));
          local_get(Local(11)); i64_extend_i32_s; local_get(Local(12)); i64_mul;
          i64_store(4);
          local_get(Local(2)); local_get(Local(6)); i32_store(0);
          local_get(Local(2)); local_get(Local(1)); i32_store(4);
          local_get(Local(2)); i32_const(Imm32(0)); i32_store(8);
          local_get(Local(2)); return_;
        end;
    });
}

/// Parse JSON array.
fn emit_parse_array(f: &mut Function, alloc: u32, parse_at_fn: u32) {
    wasm!(f, {
        local_get(Local(4)); i32_const(Imm32(ASCII_LBRACKET)); i32_eq;
        if_empty;
          local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
    });
    // Skip whitespace (inline)
    wasm!(f, {
          block_empty; loop_empty;
            local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
            i32_load8_u(0); local_set(Local(4));
            local_get(Local(4)); i32_const(Imm32(ASCII_SPACE)); i32_eq;
            local_get(Local(4)); i32_const(Imm32(ASCII_HT)); i32_eq; i32_or;
            local_get(Local(4)); i32_const(Imm32(ASCII_LF)); i32_eq; i32_or;
            local_get(Local(4)); i32_const(Imm32(ASCII_CR)); i32_eq; i32_or;
            i32_eqz; br_if(1);
            local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
            br(0);
          end; end;
    });
    // Check empty array
    wasm!(f, {
          local_get(Local(1)); local_get(Local(3)); i32_lt_u;
          if_empty;
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
            i32_load8_u(0); i32_const(Imm32(ASCII_RBRACKET)); i32_eq;
            if_empty;
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
              i32_const(Imm32(list_hdr())); call(alloc); local_set(Local(8));
              local_get(Local(8)); i32_const(Imm32(0)); i32_store(0);
              i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(6));
              local_get(Local(6)); i32_const(Imm32(VTAG_ARRAY)); i32_store(0);
              local_get(Local(6)); local_get(Local(8)); i32_store(4);
              local_get(Local(2)); local_get(Local(6)); i32_store(0);
              local_get(Local(2)); local_get(Local(1)); i32_store(4);
              local_get(Local(2)); i32_const(Imm32(0)); i32_store(8);
              local_get(Local(2)); return_;
            end;
          end;
    });
    // Parse elements — growable buffer (local 14 = capacity)
    wasm!(f, {
          i32_const(Imm32(INIT_CAPACITY)); local_set(Local(14)); // initial capacity
          i32_const(Imm32(INIT_BUF_BYTES)); call(alloc); local_set(Local(8)); // list_hdr + INIT_CAPACITY*4
          i32_const(Imm32(0)); local_set(Local(9)); // count = 0
          block_empty; loop_empty;
            local_get(Local(0)); local_get(Local(1));
            call(parse_at_fn); local_set(Local(10));
            local_get(Local(10)); i32_load(8);
            if_empty;
              local_get(Local(2)); local_get(Local(10)); i32_load(0); i32_store(0);
              local_get(Local(2)); i32_const(Imm32(0)); i32_store(4);
              local_get(Local(2)); i32_const(Imm32(1)); i32_store(8);
              local_get(Local(2)); return_;
            end;
    });
    // Grow buffer if count >= capacity (uses only locals 14, 15)
    wasm!(f, {
            local_get(Local(9)); local_get(Local(14)); i32_ge_u;
            if_empty;
              local_get(Local(8)); local_set(Local(15)); // save old buf
              local_get(Local(14)); i32_const(Imm32(1)); i32_shl; local_set(Local(14)); // cap *= 2
              i32_const(Imm32(list_hdr())); local_get(Local(14)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              call(alloc); local_set(Local(8)); // new buf → local 8
              local_get(Local(8)); local_get(Local(15));
              i32_const(Imm32(list_hdr())); local_get(Local(9)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              memory_copy;
            end;
    });
    wasm!(f, {
            local_get(Local(8)); i32_const(Imm32(list_data_off())); i32_add;
            local_get(Local(9)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            local_get(Local(10)); i32_load(0); i32_store(0);
            local_get(Local(10)); i32_load(4); local_set(Local(1));
            local_get(Local(9)); i32_const(Imm32(1)); i32_add; local_set(Local(9));
    });
    // Skip whitespace after element
    wasm!(f, {
            block_empty; loop_empty;
              local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
              local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
              i32_load8_u(0); local_set(Local(4));
              local_get(Local(4)); i32_const(Imm32(ASCII_SPACE)); i32_eq;
              local_get(Local(4)); i32_const(Imm32(ASCII_HT)); i32_eq; i32_or;
              local_get(Local(4)); i32_const(Imm32(ASCII_LF)); i32_eq; i32_or;
              local_get(Local(4)); i32_const(Imm32(ASCII_CR)); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
              br(0);
            end; end;
    });
    // Check ] or ,
    wasm!(f, {
            local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
            i32_load8_u(0); local_set(Local(4));
            local_get(Local(4)); i32_const(Imm32(ASCII_RBRACKET)); i32_eq;
            if_empty;
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
              br(2);
            end;
            local_get(Local(4)); i32_const(Imm32(ASCII_COMMA)); i32_eq;
            if_empty;
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
            end;
            br(0);
          end; end;
    });
    // Build result
    wasm!(f, {
          local_get(Local(8)); local_get(Local(9)); i32_store(0);
          i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(6));
          local_get(Local(6)); i32_const(Imm32(VTAG_ARRAY)); i32_store(0);
          local_get(Local(6)); local_get(Local(8)); i32_store(4);
          local_get(Local(2)); local_get(Local(6)); i32_store(0);
          local_get(Local(2)); local_get(Local(1)); i32_store(4);
          local_get(Local(2)); i32_const(Imm32(0)); i32_store(8);
          local_get(Local(2)); return_;
        end;
    });
}

/// Parse JSON object.
fn emit_parse_object(f: &mut Function, alloc: u32, parse_at_fn: u32) {
    wasm!(f, {
        local_get(Local(4)); i32_const(Imm32(ASCII_LBRACE)); i32_eq;
        if_empty;
          local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
    });
    // Skip whitespace
    wasm!(f, {
          block_empty; loop_empty;
            local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
            i32_load8_u(0); local_set(Local(4));
            local_get(Local(4)); i32_const(Imm32(ASCII_SPACE)); i32_eq;
            local_get(Local(4)); i32_const(Imm32(ASCII_HT)); i32_eq; i32_or;
            local_get(Local(4)); i32_const(Imm32(ASCII_LF)); i32_eq; i32_or;
            local_get(Local(4)); i32_const(Imm32(ASCII_CR)); i32_eq; i32_or;
            i32_eqz; br_if(1);
            local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
            br(0);
          end; end;
    });
    // Check empty object
    wasm!(f, {
          local_get(Local(1)); local_get(Local(3)); i32_lt_u;
          if_empty;
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
            i32_load8_u(0); i32_const(Imm32(ASCII_RBRACE)); i32_eq;
            if_empty;
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
              i32_const(Imm32(list_hdr())); call(alloc); local_set(Local(8));
              local_get(Local(8)); i32_const(Imm32(0)); i32_store(0);
              i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(6));
              local_get(Local(6)); i32_const(Imm32(VTAG_OBJECT)); i32_store(0);
              local_get(Local(6)); local_get(Local(8)); i32_store(4);
              local_get(Local(2)); local_get(Local(6)); i32_store(0);
              local_get(Local(2)); local_get(Local(1)); i32_store(4);
              local_get(Local(2)); i32_const(Imm32(0)); i32_store(8);
              local_get(Local(2)); return_;
            end;
          end;
    });
    // Parse key-value pairs — growable buffer (local 14 = capacity)
    wasm!(f, {
          i32_const(Imm32(INIT_CAPACITY)); local_set(Local(14)); // initial capacity
          i32_const(Imm32(INIT_BUF_BYTES)); call(alloc); local_set(Local(8)); // list_hdr + INIT_CAPACITY*4
          i32_const(Imm32(0)); local_set(Local(9));
          block_empty; loop_empty;
    });
    // Skip whitespace before key
    wasm!(f, {
            block_empty; loop_empty;
              local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
              local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
              i32_load8_u(0); local_set(Local(4));
              local_get(Local(4)); i32_const(Imm32(ASCII_SPACE)); i32_eq;
              local_get(Local(4)); i32_const(Imm32(ASCII_HT)); i32_eq; i32_or;
              local_get(Local(4)); i32_const(Imm32(ASCII_LF)); i32_eq; i32_or;
              local_get(Local(4)); i32_const(Imm32(ASCII_CR)); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
              br(0);
            end; end;
    });
    // Parse key
    wasm!(f, {
            local_get(Local(0)); local_get(Local(1));
            call(parse_at_fn); local_set(Local(10));
            local_get(Local(10)); i32_load(8);
            if_empty;
              local_get(Local(2)); local_get(Local(10)); i32_load(0); i32_store(0);
              local_get(Local(2)); i32_const(Imm32(0)); i32_store(4);
              local_get(Local(2)); i32_const(Imm32(1)); i32_store(8);
              local_get(Local(2)); return_;
            end;
            local_get(Local(10)); i32_load(0); i32_load(4); local_set(Local(7));
            local_get(Local(10)); i32_load(4); local_set(Local(1));
    });
    // Skip whitespace and colon
    wasm!(f, {
            block_empty; loop_empty;
              local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
              local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
              i32_load8_u(0); local_set(Local(4));
              local_get(Local(4)); i32_const(Imm32(ASCII_SPACE)); i32_eq;
              local_get(Local(4)); i32_const(Imm32(ASCII_HT)); i32_eq; i32_or;
              local_get(Local(4)); i32_const(Imm32(ASCII_LF)); i32_eq; i32_or;
              local_get(Local(4)); i32_const(Imm32(ASCII_CR)); i32_eq; i32_or;
              local_get(Local(4)); i32_const(Imm32(ASCII_COLON)); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
              br(0);
            end; end;
    });
    // Parse value
    wasm!(f, {
            local_get(Local(0)); local_get(Local(1));
            call(parse_at_fn); local_set(Local(10));
            local_get(Local(10)); i32_load(8);
            if_empty;
              local_get(Local(2)); local_get(Local(10)); i32_load(0); i32_store(0);
              local_get(Local(2)); i32_const(Imm32(0)); i32_store(4);
              local_get(Local(2)); i32_const(Imm32(1)); i32_store(8);
              local_get(Local(2)); return_;
            end;
    });
    // Grow object list buffer if count >= capacity (uses only locals 14, 15)
    wasm!(f, {
            local_get(Local(9)); local_get(Local(14)); i32_ge_u;
            if_empty;
              local_get(Local(8)); local_set(Local(15)); // save old buf
              local_get(Local(14)); i32_const(Imm32(1)); i32_shl; local_set(Local(14)); // cap *= 2
              i32_const(Imm32(list_hdr())); local_get(Local(14)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              call(alloc); local_set(Local(8)); // new buf
              local_get(Local(8)); local_get(Local(15));
              i32_const(Imm32(list_hdr())); local_get(Local(9)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              memory_copy;
            end;
    });
    // Allocate tuple (key_str_ptr, value_ptr) and store pointer in list
    wasm!(f, {
            // Allocate VALUE_BOX_SIZE tuple: [key_str_ptr: i32][value_ptr: i32]
            i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(5)); // reuse local 5 as tuple_ptr
            local_get(Local(5)); local_get(Local(7)); i32_store(0); // key
            local_get(Local(5)); local_get(Local(10)); i32_load(0); i32_store(4); // value
            // Store tuple pointer in list at position count
            local_get(Local(8)); i32_const(Imm32(list_data_off())); i32_add;
            local_get(Local(9)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            local_get(Local(5)); i32_store(0);
            local_get(Local(10)); i32_load(4); local_set(Local(1));
            local_get(Local(9)); i32_const(Imm32(1)); i32_add; local_set(Local(9));
    });
    // Skip whitespace after value
    wasm!(f, {
            block_empty; loop_empty;
              local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
              local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
              i32_load8_u(0); local_set(Local(4));
              local_get(Local(4)); i32_const(Imm32(ASCII_SPACE)); i32_eq;
              local_get(Local(4)); i32_const(Imm32(ASCII_HT)); i32_eq; i32_or;
              local_get(Local(4)); i32_const(Imm32(ASCII_LF)); i32_eq; i32_or;
              local_get(Local(4)); i32_const(Imm32(ASCII_CR)); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
              br(0);
            end; end;
    });
    // Check } or ,
    wasm!(f, {
            local_get(Local(1)); local_get(Local(3)); i32_ge_u; br_if(1);
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(1)); i32_add;
            i32_load8_u(0); local_set(Local(4));
            local_get(Local(4)); i32_const(Imm32(ASCII_RBRACE)); i32_eq;
            if_empty;
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
              br(2);
            end;
            local_get(Local(4)); i32_const(Imm32(ASCII_COMMA)); i32_eq;
            if_empty;
              local_get(Local(1)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
            end;
            br(0);
          end; end;
    });
    // Build object result
    wasm!(f, {
          local_get(Local(8)); local_get(Local(9)); i32_store(0);
          i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(6));
          local_get(Local(6)); i32_const(Imm32(VTAG_OBJECT)); i32_store(0);
          local_get(Local(6)); local_get(Local(8)); i32_store(4);
          local_get(Local(2)); local_get(Local(6)); i32_store(0);
          local_get(Local(2)); local_get(Local(1)); i32_store(4);
          local_get(Local(2)); i32_const(Imm32(0)); i32_store(8);
          local_get(Local(2)); return_;
        end;
    });
}

// ── JsonPath runtime functions ──────────────────────────────────────
//
// JsonPath WASM memory layout (tagged heap pointer):
//   JpRoot:  [tag:i32=0]                              (4 bytes)
//   JpField: [tag:i32=1][parent_ptr:i32][name_str:i32] (12 bytes)
//   JpIndex: [tag:i32=2][parent_ptr:i32][idx:i32]      (12 bytes)
//
// The path is a linked list from leaf to root. Runtime functions linearize
// it into a flat segment array before traversal.

/// __json_get_path(value: i32, path: i32) -> i32 (Option[Value]: 0=none, ptr=some)
///
/// Linearize path, then walk value following each segment.
/// For field segments: value must be object (tag=6), find matching key.
/// For index segments: value must be array (tag=5), bounds-check index.
fn compile_json_get_path(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.json_get_path];
    let alloc = emitter.rt.alloc;
    let str_eq = emitter.rt.string.eq;

    // Locals: param 0=value, param 1=path
    // 2=seg_count, 3=cur_path, 4=segs_arr, 5=i, 6=seg_ptr, 7=seg_tag
    // 8=cur_val, 9=list, 10=len, 11=j, 12=pair_ptr, 13=found
    let mut f = Function::new([(12, ValType::I32)]);

    // --- Phase 1: Count segments ---
    // Walk path from leaf to root, counting non-root nodes.
    wasm!(f, {
        i32_const(Imm32(0)); local_set(Local(2));     // seg_count = 0
        local_get(Local(1)); local_set(Local(3));     // cur_path = path
        block_empty; loop_empty;
          local_get(Local(3)); i32_load(0);    // tag
          i32_eqz; br_if(1);           // tag==0 (root) → done
          local_get(Local(2)); i32_const(Imm32(1)); i32_add; local_set(Local(2));
          local_get(Local(3)); i32_load(4); local_set(Local(3)); // cur_path = parent
          br(0);
        end; end;
    });

    // --- Phase 2: Allocate segments array and fill in reverse ---
    // segs_arr = alloc(seg_count * I32_BYTES), each slot is a path node ptr.
    wasm!(f, {
        local_get(Local(2)); i32_eqz;
        if_empty;
          // Empty path → return some(value): alloc option box.
          // SHARE: the box holds a second reference to the input tree.
          i32_const(Imm32(I32_BYTES)); call(alloc); local_set(Local(13));
          local_get(Local(13)); local_get(Local(0)); call(emitter.rt.rc_inc); i32_store(0);
          local_get(Local(13));
          return_;
        end;
        local_get(Local(2)); i32_const(Imm32(I32_BYTES)); i32_mul; call(alloc); local_set(Local(4)); // segs_arr
        local_get(Local(2)); local_set(Local(5)); // i = seg_count (fill from end)
        local_get(Local(1)); local_set(Local(3)); // cur_path = path (start from leaf)
        block_empty; loop_empty;
          local_get(Local(3)); i32_load(0); i32_eqz; br_if(1); // root → done
          local_get(Local(5)); i32_const(Imm32(1)); i32_sub; local_set(Local(5)); // i--
          local_get(Local(4)); local_get(Local(5)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          local_get(Local(3)); i32_store(0); // segs_arr[i] = cur_path
          local_get(Local(3)); i32_load(4); local_set(Local(3)); // cur_path = parent
          br(0);
        end; end;
    });

    // --- Phase 3: Walk value following segments ---
    // cur_val = value
    wasm!(f, {
        local_get(Local(0)); local_set(Local(8)); // cur_val = value
        i32_const(Imm32(0)); local_set(Local(5)); // i = 0
        block_empty; loop_empty;
          local_get(Local(5)); local_get(Local(2)); i32_ge_u; br_if(1); // i >= seg_count → done
          // Load segment
          local_get(Local(4)); local_get(Local(5)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          i32_load(0); local_set(Local(6)); // seg_ptr
          local_get(Local(6)); i32_load(0); local_set(Local(7)); // seg_tag
    });

    // --- Field segment (tag=1) ---
    wasm!(f, {
          local_get(Local(7)); i32_const(Imm32(JP_FIELD_TAG)); i32_eq;
          if_empty;
            // cur_val must be object (tag=6)
            local_get(Local(8)); i32_load(0); i32_const(Imm32(VTAG_OBJECT)); i32_ne;
            if_empty; i32_const(Imm32(0)); return_; end; // not object → none
            local_get(Local(8)); i32_load(4); local_set(Local(9)); // list (pairs)
            local_get(Local(9)); i32_load(0); local_set(Local(10)); // len
            i32_const(Imm32(0)); local_set(Local(11)); // j = 0
            i32_const(Imm32(0)); local_set(Local(13)); // found = 0
            block_empty; loop_empty;
              local_get(Local(11)); local_get(Local(10)); i32_ge_u; br_if(1);
              local_get(Local(9)); i32_const(Imm32(list_data_off())); i32_add;
              local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              i32_load(0); local_set(Local(12)); // pair_ptr
              local_get(Local(12)); i32_load(0); // pair key
              local_get(Local(6)); i32_load(8); // segment field name
              call(str_eq);
              if_empty;
                local_get(Local(12)); i32_load(4); local_set(Local(8)); // cur_val = pair value
                i32_const(Imm32(1)); local_set(Local(13)); // found = 1
                br(2);
              end;
              local_get(Local(11)); i32_const(Imm32(1)); i32_add; local_set(Local(11));
              br(0);
            end; end;
            local_get(Local(13)); i32_eqz;
            if_empty; i32_const(Imm32(0)); return_; end; // key not found → none
          end;
    });

    // --- Index segment (tag=2) ---
    wasm!(f, {
          local_get(Local(7)); i32_const(Imm32(JP_INDEX_TAG)); i32_eq;
          if_empty;
            // cur_val must be array (tag=5)
            local_get(Local(8)); i32_load(0); i32_const(Imm32(VTAG_ARRAY)); i32_ne;
            if_empty; i32_const(Imm32(0)); return_; end; // not array → none
            local_get(Local(8)); i32_load(4); local_set(Local(9)); // list
            local_get(Local(9)); i32_load(0); local_set(Local(10)); // len
            local_get(Local(6)); i32_load(8); local_set(Local(11)); // index value
    });
    emit_normalize_neg_index(&mut f, 11, 10); // native: i<0 → len+i
    wasm!(f, {
            // Bounds check (still-negative-after-normalize counts as OOB)
            local_get(Local(11)); i32_const(Imm32(0)); i32_lt_s;
            local_get(Local(11)); local_get(Local(10)); i32_ge_s;
            i32_or;
            if_empty; i32_const(Imm32(0)); return_; end; // out of bounds → none
            // cur_val = list[index]
            local_get(Local(9)); i32_const(Imm32(list_data_off())); i32_add;
            local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            i32_load(0); local_set(Local(8));
          end;
    });

    // --- Next segment ---
    wasm!(f, {
          local_get(Local(5)); i32_const(Imm32(1)); i32_add; local_set(Local(5));
          br(0);
        end; end;
    });

    // --- Return some(cur_val): alloc Option box ---
    // SHARE: cur_val is an interior pointer into the surviving input tree.
    wasm!(f, {
        i32_const(Imm32(I32_BYTES)); call(alloc); local_set(Local(13));
        local_get(Local(13)); local_get(Local(8)); call(emitter.rt.rc_inc); i32_store(0);
        local_get(Local(13));
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.json_get_path, type_idx, f));
}

/// __json_set_path(value: i32, path: i32, new_val: i32) -> i32 (Result[Value, String])
///
/// Linearize path, then iteratively walk down saving values at each depth,
/// then rebuild from leaf to root replacing at the target path.
fn compile_json_set_path(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.json_set_path];
    let alloc = emitter.rt.alloc;
    let str_eq = emitter.rt.string.eq;

    // set_path is now FULLY INFALLIBLE, mirroring native `set_at_steps`: an Index
    // step over a non-array or an OOB index is a local no-op, and a Field step
    // over a non-object node AUTOVIVIFIES (replaces it with a fresh single-key
    // object). No path produces an Err — the prior "expected array" / "index out
    // of bounds" / "expected object" strings were all removed.

    // Locals: param 0=value, param 1=path, param 2=new_val
    // 3=seg_count, 4=cur_path, 5=segs_arr, 6=depth, 7=seg_ptr, 8=seg_tag
    // 9=cur_val, 10=list, 11=len, 12=j, 13=pair_ptr, 14=result
    // 15=new_list, 16=val_stack, 17=found, 18=idx
    let mut f = Function::new([(16, ValType::I32)]);

    // --- Phase 1: Count segments ---
    wasm!(f, {
        i32_const(Imm32(0)); local_set(Local(3));
        local_get(Local(1)); local_set(Local(4));
        block_empty; loop_empty;
          local_get(Local(4)); i32_load(0); i32_eqz; br_if(1);
          local_get(Local(3)); i32_const(Imm32(1)); i32_add; local_set(Local(3));
          local_get(Local(4)); i32_load(4); local_set(Local(4));
          br(0);
        end; end;
    });

    // --- Phase 2: Allocate and fill segments array ---
    wasm!(f, {
        local_get(Local(3)); i32_eqz;
        if_empty;
          // Empty path → ok(new_val)
          i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(14));
          local_get(Local(14)); i32_const(Imm32(0)); i32_store(0);
          local_get(Local(14)); local_get(Local(2)); i32_store(4);
          local_get(Local(14));
          return_;
        end;
        local_get(Local(3)); i32_const(Imm32(I32_BYTES)); i32_mul; call(alloc); local_set(Local(5));
        local_get(Local(3)); local_set(Local(6));
        local_get(Local(1)); local_set(Local(4));
        block_empty; loop_empty;
          local_get(Local(4)); i32_load(0); i32_eqz; br_if(1);
          local_get(Local(6)); i32_const(Imm32(1)); i32_sub; local_set(Local(6));
          local_get(Local(5)); local_get(Local(6)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          local_get(Local(4)); i32_store(0);
          local_get(Local(4)); i32_load(4); local_set(Local(4));
          br(0);
        end; end;
    });

    // --- Phase 3: Walk forward saving values at each depth ---
    // val_stack = alloc((seg_count+1) * I32_BYTES): val_stack[d] = value at depth d
    wasm!(f, {
        local_get(Local(3)); i32_const(Imm32(1)); i32_add; i32_const(Imm32(I32_BYTES)); i32_mul;
        call(alloc); local_set(Local(16)); // val_stack
        local_get(Local(16)); local_get(Local(0)); i32_store(0); // val_stack[0] = value
        i32_const(Imm32(0)); local_set(Local(6)); // depth = 0
        block_empty; loop_empty;
          local_get(Local(6)); local_get(Local(3)); i32_const(Imm32(1)); i32_sub; i32_ge_u; br_if(1); // depth >= seg_count-1 → done
          local_get(Local(5)); local_get(Local(6)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          i32_load(0); local_set(Local(7)); // seg_ptr = segs_arr[depth]
          local_get(Local(7)); i32_load(0); local_set(Local(8)); // seg_tag
          // Load cur_val from val_stack[depth]
          local_get(Local(16)); local_get(Local(6)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          i32_load(0); local_set(Local(9));
    });

    // Navigate field during forward walk.
    //
    // Native `set_at_steps` Field match (runtime/rs/src/json.rs:277-289) is
    // INFALLIBLE: an object recurses into the matching/absent key (seeding the
    // absent key with `Object(vec![])`), and a NON-object node is REPLACED by a
    // fresh single-key object built from `Object(vec![])` too. So both
    // "key absent in object" and "node is not an object" descend into an empty
    // object — i.e. the placeholder for the next depth is `{}`, never `null` and
    // never an Err. The prior `path error: expected object` Err diverged: native
    // autovivifies here instead of failing.
    wasm!(f, {
          local_get(Local(8)); i32_const(Imm32(JP_FIELD_TAG)); i32_eq;
          if_empty;
            i32_const(Imm32(0)); local_set(Local(17)); // found = 0
            // Only scan pairs when cur_val is actually an object; otherwise it
            // is a non-object that native replaces (autoviv) — leave found = 0.
            local_get(Local(9)); i32_load(0); i32_const(Imm32(VTAG_OBJECT)); i32_eq;
            if_empty;
              local_get(Local(9)); i32_load(4); local_set(Local(10)); // pairs list
              local_get(Local(10)); i32_load(0); local_set(Local(11)); // len
              i32_const(Imm32(0)); local_set(Local(12));
              block_empty; loop_empty;
                local_get(Local(12)); local_get(Local(11)); i32_ge_u; br_if(1);
                local_get(Local(10)); i32_const(Imm32(list_data_off())); i32_add;
                local_get(Local(12)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                i32_load(0); local_set(Local(13)); // pair
                local_get(Local(13)); i32_load(0);
                local_get(Local(7)); i32_load(8);
                call(str_eq);
                if_empty;
                  local_get(Local(16)); local_get(Local(6)); i32_const(Imm32(1)); i32_add; i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                  local_get(Local(13)); i32_load(4); i32_store(0);
                  i32_const(Imm32(1)); local_set(Local(17));
                  br(2);
                end;
                local_get(Local(12)); i32_const(Imm32(1)); i32_add; local_set(Local(12));
                br(0);
              end; end;
            end;
            // Key absent (or cur_val was a non-object): seed the next depth with
            // a fresh empty object, mirroring native `Object(vec![])`.
            local_get(Local(17)); i32_eqz;
            if_empty;
    });
    emit_make_empty_object(&mut f, 17, alloc);
    wasm!(f, {
              local_get(Local(16)); local_get(Local(6)); i32_const(Imm32(1)); i32_add; i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              local_get(Local(17)); i32_store(0);
            end;
          end;
    });

    // Navigate index during forward walk. Native `set_at_steps` Index match
    // (runtime/rs/src/json.rs:290-300) makes a non-array node OR an OOB index a
    // LOCAL no-op (`j.clone()` / array unchanged) — it does NOT abort the outer
    // operation. The leaf-to-root rebuild's Index branch already reproduces that
    // local no-op (it discards `cur_built` and re-emits `orig_val` = the node at
    // this depth), so the forward walk must NOT hard-return the original root:
    // doing so erased any autovivification that happened at a shallower depth
    // (e.g. `.x[0].y` over a scalar root → native `{"x":{}}`, not the untouched
    // root). The forward walk only needs a valid placeholder for the next depth;
    // the rebuild throws it away at this Index level. Use an empty object so the
    // placeholder is never garbage and matches native's `Object(vec![])` seed.
    let emit_index_noop_placeholder = |f: &mut Function| {
        // val_stack[depth+1] = {}  (don't-care value; rebuild discards it here)
        emit_make_empty_object(f, 17, alloc);
        wasm!(f, {
            local_get(Local(16)); local_get(Local(6)); i32_const(Imm32(1)); i32_add; i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            local_get(Local(17)); i32_store(0);
        });
    };
    wasm!(f, {
          local_get(Local(8)); i32_const(Imm32(JP_INDEX_TAG)); i32_eq;
          if_empty;
            local_get(Local(9)); i32_load(0); i32_const(Imm32(VTAG_ARRAY)); i32_ne;
            if_empty;
    });
    emit_index_noop_placeholder(&mut f);
    wasm!(f, {
            else_;
            local_get(Local(9)); i32_load(4); local_set(Local(10)); // list
            local_get(Local(10)); i32_load(0); local_set(Local(11)); // len
            local_get(Local(7)); i32_load(8); local_set(Local(18)); // idx
    });
    emit_normalize_neg_index(&mut f, 18, 11);
    wasm!(f, {
            local_get(Local(18)); i32_const(Imm32(0)); i32_lt_s;
            local_get(Local(18)); local_get(Local(11)); i32_ge_s;
            i32_or;
            if_empty;
    });
    emit_index_noop_placeholder(&mut f);
    wasm!(f, {
            else_;
            local_get(Local(16)); local_get(Local(6)); i32_const(Imm32(1)); i32_add; i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            local_get(Local(10)); i32_const(Imm32(list_data_off())); i32_add;
            local_get(Local(18)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            i32_load(0); i32_store(0);
            end;
            end;
          end;
    });

    // Next depth in forward walk
    wasm!(f, {
          local_get(Local(6)); i32_const(Imm32(1)); i32_add; local_set(Local(6));
          br(0);
        end; end;
    });

    // --- Phase 4: Rebuild from leaf to root ---
    // cur_built starts as new_val, then we wrap it at each level going backwards.
    wasm!(f, {
        local_get(Local(2)); local_set(Local(9)); // cur_built = new_val
        local_get(Local(3)); i32_const(Imm32(1)); i32_sub; local_set(Local(6)); // depth = seg_count - 1
        block_empty; loop_empty;
          local_get(Local(6)); i32_const(Imm32(0)); i32_lt_s; br_if(1);
          local_get(Local(5)); local_get(Local(6)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          i32_load(0); local_set(Local(7)); // seg
          local_get(Local(7)); i32_load(0); local_set(Local(8)); // seg_tag
          // orig_val at this depth
          local_get(Local(16)); local_get(Local(6)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          i32_load(0); local_set(Local(14));
    });

    // Rebuild for field segment
    wasm!(f, {
          local_get(Local(8)); i32_const(Imm32(JP_FIELD_TAG)); i32_eq;
          if_empty;
            local_get(Local(14)); i32_load(0); i32_const(Imm32(VTAG_OBJECT)); i32_eq;
            if_empty;
              // Clone pairs, replacing matching key
              local_get(Local(14)); i32_load(4); local_set(Local(10)); // old pairs
              local_get(Local(10)); i32_load(0); local_set(Local(11)); // old len
              // Check if key exists
              i32_const(Imm32(0)); local_set(Local(17));
              i32_const(Imm32(0)); local_set(Local(12));
              block_empty; loop_empty;
                local_get(Local(12)); local_get(Local(11)); i32_ge_u; br_if(1);
                local_get(Local(10)); i32_const(Imm32(list_data_off())); i32_add;
                local_get(Local(12)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                i32_load(0); local_set(Local(13));
                local_get(Local(13)); i32_load(0);
                local_get(Local(7)); i32_load(8);
                call(str_eq);
                if_empty; i32_const(Imm32(1)); local_set(Local(17)); end;
                local_get(Local(12)); i32_const(Imm32(1)); i32_add; local_set(Local(12));
                br(0);
              end; end;
              // new_len = old_len + (found ? 0 : 1)
              local_get(Local(11)); local_get(Local(17)); i32_eqz; i32_add; local_set(Local(18));
              // Alloc new pairs list
              i32_const(Imm32(list_data_off())); local_get(Local(18)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              call(alloc); local_set(Local(15));
              local_get(Local(15)); local_get(Local(18)); i32_store(0);
              // Copy, replacing match
              i32_const(Imm32(0)); local_set(Local(12));
              block_empty; loop_empty;
                local_get(Local(12)); local_get(Local(11)); i32_ge_u; br_if(1);
                local_get(Local(10)); i32_const(Imm32(list_data_off())); i32_add;
                local_get(Local(12)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                i32_load(0); local_set(Local(13));
                local_get(Local(13)); i32_load(0);
                local_get(Local(7)); i32_load(8);
                call(str_eq);
                if_empty;
                  // Replace value — the kept KEY string is shared from the
                  // old pair the source tree still owns: dup.
                  i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(17));
                  local_get(Local(17)); local_get(Local(13)); i32_load(0); call(emitter.rt.rc_inc); i32_store(0);
                  local_get(Local(17)); local_get(Local(9)); i32_store(4);
                  local_get(Local(15)); i32_const(Imm32(list_data_off())); i32_add;
                  local_get(Local(12)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                  local_get(Local(17)); i32_store(0);
                else_;
                  // Unchanged pair: shared between old and new object — dup.
                  local_get(Local(15)); i32_const(Imm32(list_data_off())); i32_add;
                  local_get(Local(12)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                  local_get(Local(13)); call(emitter.rt.rc_inc); i32_store(0);
                end;
                local_get(Local(12)); i32_const(Imm32(1)); i32_add; local_set(Local(12));
                br(0);
              end; end;
              // Append new pair if key was not found
              local_get(Local(18)); local_get(Local(11)); i32_gt_u;
              if_empty;
                i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(17));
                local_get(Local(17)); local_get(Local(7)); i32_load(8); call(emitter.rt.rc_inc); i32_store(0);
                local_get(Local(17)); local_get(Local(9)); i32_store(4);
                local_get(Local(15)); i32_const(Imm32(list_data_off())); i32_add;
                local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                local_get(Local(17)); i32_store(0);
              end;
              // Build object
              i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(9));
              local_get(Local(9)); i32_const(Imm32(VTAG_OBJECT)); i32_store(0);
              local_get(Local(9)); local_get(Local(15)); i32_store(4);
            else_;
              // Not an object → AUTOVIVIFY: replace it with a fresh single-key
              // object {seg_key: cur_built}, mirroring native `set_at_steps`
              // Field-over-non-object (json.rs:288).
              i32_const(Imm32(list_hdr() + I32_BYTES)); call(alloc); local_set(Local(15)); // pairs list: 1 slot
              local_get(Local(15)); i32_const(Imm32(1)); i32_store(0);
              i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(17)); // pair [key][val]
              local_get(Local(17)); local_get(Local(7)); i32_load(8); call(emitter.rt.rc_inc); i32_store(0);
              local_get(Local(17)); local_get(Local(9)); i32_store(4);
              local_get(Local(15)); i32_const(Imm32(list_data_off())); i32_add; local_get(Local(17)); i32_store(0);
              i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(9));
              local_get(Local(9)); i32_const(Imm32(VTAG_OBJECT)); i32_store(0);
              local_get(Local(9)); local_get(Local(15)); i32_store(4);
            end;
          end;
    });

    // Rebuild for index segment. Native `set_at_steps` Index match:
    //   - non-array → j.clone() (no-op); OOB → array unchanged (no-op).
    // So when orig is not an array OR the (normalized) index is OOB, keep the
    // original value as the rebuilt node instead of fabricating an array.
    wasm!(f, {
          local_get(Local(8)); i32_const(Imm32(JP_INDEX_TAG)); i32_eq;
          if_empty;
            // Non-array → no-op: cur_built = orig_val.
            local_get(Local(14)); i32_load(0); i32_const(Imm32(VTAG_ARRAY)); i32_ne;
            if_empty;
              local_get(Local(14)); call(emitter.rt.rc_inc); local_set(Local(9));
            else_;
              local_get(Local(14)); i32_load(4); local_set(Local(10)); // old list
              local_get(Local(10)); i32_load(0); local_set(Local(11)); // len
              local_get(Local(7)); i32_load(8); local_set(Local(18)); // idx
    });
    emit_normalize_neg_index(&mut f, 18, 11);
    wasm!(f, {
              // OOB (incl. still-negative) → no-op: cur_built = orig_val.
              local_get(Local(18)); i32_const(Imm32(0)); i32_lt_s;
              local_get(Local(18)); local_get(Local(11)); i32_ge_s;
              i32_or;
              if_empty;
                local_get(Local(14)); call(emitter.rt.rc_inc); local_set(Local(9));
              else_;
                // Clone list replacing at idx
                i32_const(Imm32(list_hdr())); local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                call(alloc); local_set(Local(15));
                local_get(Local(15)); local_get(Local(11)); i32_store(0);
                i32_const(Imm32(0)); local_set(Local(12));
                block_empty; loop_empty;
                  local_get(Local(12)); local_get(Local(11)); i32_ge_u; br_if(1);
                  local_get(Local(15)); i32_const(Imm32(list_data_off())); i32_add;
                  local_get(Local(12)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                  local_get(Local(12)); local_get(Local(18)); i32_eq;
                  if_i32; local_get(Local(9));
                  else_;
                    // Unchanged element: shared between old and new array — dup.
                    local_get(Local(10)); i32_const(Imm32(list_data_off())); i32_add;
                    local_get(Local(12)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                    i32_load(0); call(emitter.rt.rc_inc);
                  end;
                  i32_store(0);
                  local_get(Local(12)); i32_const(Imm32(1)); i32_add; local_set(Local(12));
                  br(0);
                end; end;
                i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(9));
                local_get(Local(9)); i32_const(Imm32(VTAG_ARRAY)); i32_store(0);
                local_get(Local(9)); local_get(Local(15)); i32_store(4);
              end;
            end;
          end;
    });

    // Next depth upward
    wasm!(f, {
          local_get(Local(6)); i32_const(Imm32(1)); i32_sub; local_set(Local(6));
          br(0);
        end; end;
    });

    // Return ok(result)
    wasm!(f, {
        i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(14));
        local_get(Local(14)); i32_const(Imm32(0)); i32_store(0);
        local_get(Local(14)); local_get(Local(9)); i32_store(4);
        local_get(Local(14));
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.json_set_path, type_idx, f));
}

/// __json_remove_path(value: i32, path: i32) -> i32 (Value)
///
/// Linearize path, walk to target, rebuild without the target element.
fn compile_json_remove_path(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.json_remove_path];
    let alloc = emitter.rt.alloc;
    let str_eq = emitter.rt.string.eq;

    // Locals: param 0=value, param 1=path
    // 2=seg_count, 3=cur_path, 4=segs_arr, 5=depth, 6=seg_ptr, 7=seg_tag
    // 8=cur_val, 9=list, 10=len, 11=j, 12=pair_ptr, 13=found
    // 14=val_stack, 15=new_list, 16=cur_built, 17=idx, 18=dst
    let mut f = Function::new([(17, ValType::I32)]);

    // --- Phase 1: Count segments ---
    wasm!(f, {
        i32_const(Imm32(0)); local_set(Local(2));
        local_get(Local(1)); local_set(Local(3));
        block_empty; loop_empty;
          local_get(Local(3)); i32_load(0); i32_eqz; br_if(1);
          local_get(Local(2)); i32_const(Imm32(1)); i32_add; local_set(Local(2));
          local_get(Local(3)); i32_load(4); local_set(Local(3));
          br(0);
        end; end;
    });

    // --- Phase 2: Allocate and fill segments array ---
    wasm!(f, {
        local_get(Local(2)); i32_eqz;
        if_empty;
          // Empty path → return null (removing root itself)
          i32_const(Imm32(I32_BYTES)); call(alloc); local_set(Local(16));
          local_get(Local(16)); i32_const(Imm32(0)); i32_store(0);
          local_get(Local(16));
          return_;
        end;
        local_get(Local(2)); i32_const(Imm32(I32_BYTES)); i32_mul; call(alloc); local_set(Local(4));
        local_get(Local(2)); local_set(Local(5));
        local_get(Local(1)); local_set(Local(3));
        block_empty; loop_empty;
          local_get(Local(3)); i32_load(0); i32_eqz; br_if(1);
          local_get(Local(5)); i32_const(Imm32(1)); i32_sub; local_set(Local(5));
          local_get(Local(4)); local_get(Local(5)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          local_get(Local(3)); i32_store(0);
          local_get(Local(3)); i32_load(4); local_set(Local(3));
          br(0);
        end; end;
    });

    // --- Phase 3: Walk forward saving values at each depth (all but last) ---
    wasm!(f, {
        local_get(Local(2)); i32_const(Imm32(I32_BYTES)); i32_mul; call(alloc); local_set(Local(14)); // val_stack
        local_get(Local(14)); local_get(Local(0)); i32_store(0); // val_stack[0] = value
        i32_const(Imm32(0)); local_set(Local(5)); // depth = 0
        block_empty; loop_empty;
          local_get(Local(5)); local_get(Local(2)); i32_const(Imm32(1)); i32_sub; i32_ge_u; br_if(1);
          local_get(Local(4)); local_get(Local(5)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          i32_load(0); local_set(Local(6));
          local_get(Local(6)); i32_load(0); local_set(Local(7));
          local_get(Local(14)); local_get(Local(5)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          i32_load(0); local_set(Local(8));
    });

    // Navigate field for walk
    wasm!(f, {
          local_get(Local(7)); i32_const(Imm32(JP_FIELD_TAG)); i32_eq;
          if_empty;
            local_get(Local(8)); i32_load(0); i32_const(Imm32(VTAG_OBJECT)); i32_ne;
            if_empty; local_get(Local(0)); call(emitter.rt.rc_inc); return_; end;
            local_get(Local(8)); i32_load(4); local_set(Local(9));
            local_get(Local(9)); i32_load(0); local_set(Local(10));
            i32_const(Imm32(0)); local_set(Local(11));
            i32_const(Imm32(0)); local_set(Local(13));
            block_empty; loop_empty;
              local_get(Local(11)); local_get(Local(10)); i32_ge_u; br_if(1);
              local_get(Local(9)); i32_const(Imm32(list_data_off())); i32_add;
              local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              i32_load(0); local_set(Local(12));
              local_get(Local(12)); i32_load(0);
              local_get(Local(6)); i32_load(8);
              call(str_eq);
              if_empty;
                local_get(Local(14)); local_get(Local(5)); i32_const(Imm32(1)); i32_add; i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                local_get(Local(12)); i32_load(4); i32_store(0);
                i32_const(Imm32(1)); local_set(Local(13));
                br(2);
              end;
              local_get(Local(11)); i32_const(Imm32(1)); i32_add; local_set(Local(11));
              br(0);
            end; end;
            local_get(Local(13)); i32_eqz;
            if_empty; local_get(Local(0)); call(emitter.rt.rc_inc); return_; end;
          end;
    });

    // Navigate index for walk
    wasm!(f, {
          local_get(Local(7)); i32_const(Imm32(JP_INDEX_TAG)); i32_eq;
          if_empty;
            local_get(Local(8)); i32_load(0); i32_const(Imm32(VTAG_ARRAY)); i32_ne;
            if_empty; local_get(Local(0)); call(emitter.rt.rc_inc); return_; end; // non-array → no-op (orig)
            local_get(Local(8)); i32_load(4); local_set(Local(9));
            local_get(Local(9)); i32_load(0); local_set(Local(10));
            local_get(Local(6)); i32_load(8); local_set(Local(17));
    });
    emit_normalize_neg_index(&mut f, 17, 10);
    wasm!(f, {
            local_get(Local(17)); i32_const(Imm32(0)); i32_lt_s;
            local_get(Local(17)); local_get(Local(10)); i32_ge_s;
            i32_or;
            if_empty; local_get(Local(0)); call(emitter.rt.rc_inc); return_; end; // OOB → no-op (orig)
            local_get(Local(14)); local_get(Local(5)); i32_const(Imm32(1)); i32_add; i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            local_get(Local(9)); i32_const(Imm32(list_data_off())); i32_add;
            local_get(Local(17)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            i32_load(0); i32_store(0);
          end;
    });

    wasm!(f, {
          local_get(Local(5)); i32_const(Imm32(1)); i32_add; local_set(Local(5));
          br(0);
        end; end;
    });

    // --- Phase 4: Remove at the last segment ---
    // Load last segment and value at that depth
    wasm!(f, {
        local_get(Local(4)); local_get(Local(2)); i32_const(Imm32(1)); i32_sub; i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
        i32_load(0); local_set(Local(6));
        local_get(Local(6)); i32_load(0); local_set(Local(7));
        local_get(Local(14)); local_get(Local(2)); i32_const(Imm32(1)); i32_sub; i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
        i32_load(0); local_set(Local(8));
    });

    // Remove field from object
    wasm!(f, {
        local_get(Local(7)); i32_const(Imm32(JP_FIELD_TAG)); i32_eq;
        if_empty;
          local_get(Local(8)); i32_load(0); i32_const(Imm32(VTAG_OBJECT)); i32_ne;
          if_empty; local_get(Local(0)); call(emitter.rt.rc_inc); return_; end;
          local_get(Local(8)); i32_load(4); local_set(Local(9));
          local_get(Local(9)); i32_load(0); local_set(Local(10));
          // Alloc new list (worst case same size)
          i32_const(Imm32(list_data_off())); local_get(Local(10)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          call(alloc); local_set(Local(15));
          i32_const(Imm32(0)); local_set(Local(11)); // src
          i32_const(Imm32(0)); local_set(Local(18)); // dst
          block_empty; loop_empty;
            local_get(Local(11)); local_get(Local(10)); i32_ge_u; br_if(1);
            local_get(Local(9)); i32_const(Imm32(list_data_off())); i32_add;
            local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            i32_load(0); local_set(Local(12));
            local_get(Local(12)); i32_load(0);
            local_get(Local(6)); i32_load(8);
            call(str_eq);
            i32_eqz;
            if_empty;
              // Surviving pair: shared with the source object — dup.
              local_get(Local(15)); i32_const(Imm32(list_data_off())); i32_add;
              local_get(Local(18)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              local_get(Local(12)); call(emitter.rt.rc_inc); i32_store(0);
              local_get(Local(18)); i32_const(Imm32(1)); i32_add; local_set(Local(18));
            end;
            local_get(Local(11)); i32_const(Imm32(1)); i32_add; local_set(Local(11));
            br(0);
          end; end;
          local_get(Local(15)); local_get(Local(18)); i32_store(0); // set actual len
          i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(16));
          local_get(Local(16)); i32_const(Imm32(VTAG_OBJECT)); i32_store(0);
          local_get(Local(16)); local_get(Local(15)); i32_store(4);
        end;
    });

    // Remove index from array
    wasm!(f, {
        local_get(Local(7)); i32_const(Imm32(JP_INDEX_TAG)); i32_eq;
        if_empty;
          local_get(Local(8)); i32_load(0); i32_const(Imm32(VTAG_ARRAY)); i32_ne;
          if_empty; local_get(Local(0)); call(emitter.rt.rc_inc); return_; end; // non-array → no-op (orig)
          local_get(Local(8)); i32_load(4); local_set(Local(9));
          local_get(Local(9)); i32_load(0); local_set(Local(10));
          local_get(Local(6)); i32_load(8); local_set(Local(17));
    });
    emit_normalize_neg_index(&mut f, 17, 10);
    wasm!(f, {
          local_get(Local(17)); i32_const(Imm32(0)); i32_lt_s;
          local_get(Local(17)); local_get(Local(10)); i32_ge_s;
          i32_or;
          if_empty; local_get(Local(0)); call(emitter.rt.rc_inc); return_; end; // OOB → no-op (orig)
          // Alloc new list (len - 1)
          local_get(Local(10)); i32_const(Imm32(1)); i32_sub; local_set(Local(13));
          i32_const(Imm32(list_hdr())); local_get(Local(13)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          call(alloc); local_set(Local(15));
          local_get(Local(15)); local_get(Local(13)); i32_store(0);
          i32_const(Imm32(0)); local_set(Local(11)); // src
          i32_const(Imm32(0)); local_set(Local(18)); // dst
          block_empty; loop_empty;
            local_get(Local(11)); local_get(Local(10)); i32_ge_u; br_if(1);
            local_get(Local(11)); local_get(Local(17)); i32_ne;
            if_empty;
              local_get(Local(15)); i32_const(Imm32(list_data_off())); i32_add;
              local_get(Local(18)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              local_get(Local(9)); i32_const(Imm32(list_data_off())); i32_add;
              local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              i32_load(0); i32_store(0);
              local_get(Local(18)); i32_const(Imm32(1)); i32_add; local_set(Local(18));
            end;
            local_get(Local(11)); i32_const(Imm32(1)); i32_add; local_set(Local(11));
            br(0);
          end; end;
          i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(16));
          local_get(Local(16)); i32_const(Imm32(VTAG_ARRAY)); i32_store(0);
          local_get(Local(16)); local_get(Local(15)); i32_store(4);
        end;
    });

    // --- Phase 5: Rebuild upward from seg_count-2 to 0 ---
    // cur_built is in local 16
    wasm!(f, {
        local_get(Local(2)); i32_const(Imm32(JP_INDEX_TAG)); i32_sub; local_set(Local(5)); // depth = seg_count - 2
        block_empty; loop_empty;
          local_get(Local(5)); i32_const(Imm32(0)); i32_lt_s; br_if(1);
          local_get(Local(4)); local_get(Local(5)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          i32_load(0); local_set(Local(6));
          local_get(Local(6)); i32_load(0); local_set(Local(7));
          local_get(Local(14)); local_get(Local(5)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
          i32_load(0); local_set(Local(8)); // orig val
    });

    // Rebuild field
    wasm!(f, {
          local_get(Local(7)); i32_const(Imm32(JP_FIELD_TAG)); i32_eq;
          if_empty;
            local_get(Local(8)); i32_load(4); local_set(Local(9));
            local_get(Local(9)); i32_load(0); local_set(Local(10));
            i32_const(Imm32(list_data_off())); local_get(Local(10)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            call(alloc); local_set(Local(15));
            local_get(Local(15)); local_get(Local(10)); i32_store(0);
            i32_const(Imm32(0)); local_set(Local(11));
            block_empty; loop_empty;
              local_get(Local(11)); local_get(Local(10)); i32_ge_u; br_if(1);
              local_get(Local(9)); i32_const(Imm32(list_data_off())); i32_add;
              local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              i32_load(0); local_set(Local(12));
              local_get(Local(12)); i32_load(0);
              local_get(Local(6)); i32_load(8);
              call(str_eq);
              if_empty;
                i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(13));
                local_get(Local(13)); local_get(Local(12)); i32_load(0); call(emitter.rt.rc_inc); i32_store(0);
                local_get(Local(13)); local_get(Local(16)); i32_store(4);
                local_get(Local(15)); i32_const(Imm32(list_data_off())); i32_add;
                local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                local_get(Local(13)); call(emitter.rt.rc_inc); i32_store(0);
              else_;
                local_get(Local(15)); i32_const(Imm32(list_data_off())); i32_add;
                local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                local_get(Local(12)); call(emitter.rt.rc_inc); i32_store(0);
              end;
              local_get(Local(11)); i32_const(Imm32(1)); i32_add; local_set(Local(11));
              br(0);
            end; end;
            i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(16));
            local_get(Local(16)); i32_const(Imm32(VTAG_OBJECT)); i32_store(0);
            local_get(Local(16)); local_get(Local(15)); i32_store(4);
          end;
    });

    // Rebuild index (intermediate Index segment, e.g. xs[2].name). The forward
    // walk already validated this level is an array with an in-bounds index, so
    // a non-array here is a defensive no-op; the normalization mirrors the
    // forward walk so a negative intermediate index targets the same slot.
    wasm!(f, {
          local_get(Local(7)); i32_const(Imm32(JP_INDEX_TAG)); i32_eq;
          if_empty;
            local_get(Local(8)); i32_load(0); i32_const(Imm32(VTAG_ARRAY)); i32_ne;
            if_empty;
              local_get(Local(8)); local_set(Local(16)); // non-array → keep orig as built node
            else_;
              local_get(Local(8)); i32_load(4); local_set(Local(9));
              local_get(Local(9)); i32_load(0); local_set(Local(10));
              local_get(Local(6)); i32_load(8); local_set(Local(17));
    });
    emit_normalize_neg_index(&mut f, 17, 10);
    wasm!(f, {
              i32_const(Imm32(list_hdr())); local_get(Local(10)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
              call(alloc); local_set(Local(15));
              local_get(Local(15)); local_get(Local(10)); i32_store(0);
              i32_const(Imm32(0)); local_set(Local(11));
              block_empty; loop_empty;
                local_get(Local(11)); local_get(Local(10)); i32_ge_u; br_if(1);
                local_get(Local(15)); i32_const(Imm32(list_data_off())); i32_add;
                local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                local_get(Local(11)); local_get(Local(17)); i32_eq;
                if_i32; local_get(Local(16));
                else_;
                  // Unchanged element: shared between old and new array — dup.
                  local_get(Local(9)); i32_const(Imm32(list_data_off())); i32_add;
                  local_get(Local(11)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                  i32_load(0); call(emitter.rt.rc_inc);
                end;
                i32_store(0);
                local_get(Local(11)); i32_const(Imm32(1)); i32_add; local_set(Local(11));
                br(0);
              end; end;
              i32_const(Imm32(VALUE_BOX_SIZE)); call(alloc); local_set(Local(16));
              local_get(Local(16)); i32_const(Imm32(VTAG_ARRAY)); i32_store(0);
              local_get(Local(16)); local_get(Local(15)); i32_store(4);
            end;
          end;
    });

    wasm!(f, {
          local_get(Local(5)); i32_const(Imm32(1)); i32_sub; local_set(Local(5));
          br(0);
        end; end;
    });

    // Return cur_built
    wasm!(f, {
        local_get(Local(16));
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.json_remove_path, type_idx, f));
}
