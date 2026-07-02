//! WASM runtime: value.stringify and json.parse.
//!
//! __value_stringify(v: i32) -> i32 (String ptr)
//!   Tag-based dispatch to produce string representation of a Value.
//!
//! __json_parse(s: i32) -> i32 (Result[Value, String] ptr)
//!   Minimal recursive descent JSON parser.

use super::{CompiledFunc, WasmEmitter};
use super::rt_string::{string_data_off, string_hdr, string_cap_off, list_data_off, list_hdr};
use wasm_encoder::{ValType};
use super::TrackedFunction as Function;

// Value heap tags (see `compile_value_stringify`): 0=null, 1=bool, 2=int,
// 3=float, 4=string, 5=array, 6=object. Only the container tags matter for the
// path walkers; the rest are "scalar" and never index/field into.
const VTAG_ARRAY: i32 = 5;
const VTAG_OBJECT: i32 = 6;

/// Size in bytes of a heap value box: `[tag:i32][payload:i32]`. The payload is
/// the inline scalar (bool/int), or a pointer (string/array/object pairs-list).
const VALUE_BOX_SIZE: i32 = 8;

/// Emit in-place negative-index normalization, mirroring the native oracle
/// `if i < 0 { len as i64 + i }` (runtime/rs/src/json.rs get/set/remove paths):
/// `if idx_local < 0 { idx_local += len_local }`. After this, the standard
/// `idx < 0 || idx >= len` bounds check rejects an index that is still negative
/// (e.g. -5 over len 3), so an out-of-range normalized index stays a no-op.
fn emit_normalize_neg_index(f: &mut Function, idx_local: u32, len_local: u32) {
    wasm!(f, {
        local_get(idx_local); i32_const(0); i32_lt_s;
        if_empty;
          local_get(idx_local); local_get(len_local); i32_add; local_set(idx_local);
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
        i32_const(list_hdr()); call(alloc); local_set(15); // empty pairs list
        local_get(15); i32_const(0); i32_store(0);          // len = 0
        i32_const(VALUE_BOX_SIZE); call(alloc); local_set(dst_local);
        local_get(dst_local); i32_const(VTAG_OBJECT); i32_store(0);
        local_get(dst_local); local_get(15); i32_store(4);
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

    // __value_eq(a: i32, b: i32) -> i32 (Bool) — deep structural equality,
    // mirroring the native derived `PartialEq` on `Value` (strict tags,
    // in-order object pairs). Registered last to keep prior indices stable.
    let eq_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.value_eq = emitter.register_func("__value_eq", eq_ty);
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
    compile_value_eq(emitter);
}

/// __value_eq(a: i32, b: i32) -> i32 (Bool)
///
/// Deep structural Value equality, mirroring the native derived `PartialEq`
/// on `enum Value` (runtime/rs/src/value.rs): strict tag match (no Int/Float
/// widening), string payloads by content, arrays elementwise, objects as
/// IN-ORDER pair lists (the derive compares `Vec<(String, Value)>`
/// positionally — key order matters, exactly like native). Before this, a
/// `Value == Value` at the IR level fell to the childless-record arm and
/// compared POINTERS — two separately-built `json.null()`s were "unequal".
fn compile_value_eq(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.value_eq];
    let self_idx = emitter.rt.value_eq;
    let streq = emitter.rt.string.eq;
    // params: 0=a, 1=b | locals: 2=tag, 3=pa, 4=pb, 5=len, 6=i, 7=ea, 8=eb
    let mut f = Function::new([(7, ValType::I32)]);
    let dat = list_data_off();
    wasm!(f, {
        // Identical pointer (shared box) — trivially equal.
        local_get(0); local_get(1); i32_eq;
        if_empty; i32_const(1); return_; end;
        // Tags must match (native derive: no cross-tag equality).
        local_get(0); i32_load(0); local_set(2);
        local_get(2); local_get(1); i32_load(0); i32_ne;
        if_empty; i32_const(0); return_; end;
        // null
        local_get(2); i32_eqz;
        if_empty; i32_const(1); return_; end;
        // bool: i32 payload
        local_get(2); i32_const(1); i32_eq;
        if_empty;
          local_get(0); i32_load(4); local_get(1); i32_load(4); i32_eq; return_;
        end;
        // int: i64 payload
        local_get(2); i32_const(2); i32_eq;
        if_empty;
          local_get(0); i64_load(4); local_get(1); i64_load(4); i64_eq; return_;
        end;
        // float: f64 payload (NaN != NaN, like the native derive)
        local_get(2); i32_const(3); i32_eq;
        if_empty;
          local_get(0); f64_load(4); local_get(1); f64_load(4); f64_eq; return_;
        end;
        // string: content equality
        local_get(2); i32_const(4); i32_eq;
        if_empty;
          local_get(0); i32_load(4); local_get(1); i32_load(4); call(streq); return_;
        end;
        // array: elementwise recursion
        local_get(2); i32_const(VTAG_ARRAY); i32_eq;
        if_empty;
          local_get(0); i32_load(4); local_set(3);
          local_get(1); i32_load(4); local_set(4);
          local_get(3); i32_load(0); local_set(5);
          local_get(5); local_get(4); i32_load(0); i32_ne;
          if_empty; i32_const(0); return_; end;
          i32_const(0); local_set(6);
          block_empty; loop_empty;
            local_get(6); local_get(5); i32_ge_u; br_if(1);
            local_get(3); i32_const(dat); i32_add; local_get(6); i32_const(4); i32_mul; i32_add; i32_load(0);
            local_get(4); i32_const(dat); i32_add; local_get(6); i32_const(4); i32_mul; i32_add; i32_load(0);
            call(self_idx); i32_eqz;
            if_empty; i32_const(0); return_; end;
            local_get(6); i32_const(1); i32_add; local_set(6);
            br(0);
          end; end;
          i32_const(1); return_;
        end;
        // object: in-order pair list (key content + value recursion)
        local_get(2); i32_const(VTAG_OBJECT); i32_eq;
        if_empty;
          local_get(0); i32_load(4); local_set(3);
          local_get(1); i32_load(4); local_set(4);
          local_get(3); i32_load(0); local_set(5);
          local_get(5); local_get(4); i32_load(0); i32_ne;
          if_empty; i32_const(0); return_; end;
          i32_const(0); local_set(6);
          block_empty; loop_empty;
            local_get(6); local_get(5); i32_ge_u; br_if(1);
            local_get(3); i32_const(dat); i32_add; local_get(6); i32_const(4); i32_mul; i32_add; i32_load(0); local_set(7);
            local_get(4); i32_const(dat); i32_add; local_get(6); i32_const(4); i32_mul; i32_add; i32_load(0); local_set(8);
            local_get(7); i32_load(0); local_get(8); i32_load(0); call(streq); i32_eqz;
            if_empty; i32_const(0); return_; end;
            local_get(7); i32_load(4); local_get(8); i32_load(4); call(self_idx); i32_eqz;
            if_empty; i32_const(0); return_; end;
            local_get(6); i32_const(1); i32_add; local_set(6);
            br(0);
          end; end;
          i32_const(1); return_;
        end;
        // Unknown tag: pointer identity already failed above.
        i32_const(0); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.value_eq, type_idx, f));
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
    wasm!(f, { local_get(0); local_set(1); });
    // Replace \ first (before others to avoid double-escaping)
    wasm!(f, { local_get(1); i32_const(backslash_char as i32); i32_const(esc_backslash as i32); call(replace); local_set(1); });
    wasm!(f, { local_get(1); i32_const(quote_char as i32); i32_const(esc_quote as i32); call(replace); local_set(1); });
    wasm!(f, { local_get(1); i32_const(newline_char as i32); i32_const(esc_newline as i32); call(replace); local_set(1); });
    wasm!(f, { local_get(1); i32_const(tab_char as i32); i32_const(esc_tab as i32); call(replace); local_set(1); });
    wasm!(f, { local_get(1); i32_const(cr_char as i32); i32_const(esc_cr as i32); call(replace); local_set(1); });
    wasm!(f, { local_get(1); end; });

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
    wasm!(f, { local_get(0); i32_load(0); local_set(1); });

    // Tag 0: null
    wasm!(f, {
        local_get(1); i32_eqz;
        if_empty; i32_const(null_str as i32); return_; end;
    });

    // Tag 1: bool
    wasm!(f, {
        local_get(1); i32_const(1); i32_eq;
        if_empty;
          local_get(0); i32_load(4);
          if_i32; i32_const(true_str as i32);
          else_; i32_const(false_str as i32); end;
          return_;
        end;
    });

    // Tag 2: int
    wasm!(f, {
        local_get(1); i32_const(2); i32_eq;
        if_empty;
          local_get(0); i64_load(4); call(itoa); return_;
        end;
    });

    // Tag 3: float
    wasm!(f, {
        local_get(1); i32_const(3); i32_eq;
        if_empty;
          local_get(0); f64_load(4); call(fdisp); return_;
        end;
    });

    // Tag 4: string -> "\"" + escape(s) + "\""
    let escape_fn = emitter.rt.json_escape_string;
    wasm!(f, {
        local_get(1); i32_const(4); i32_eq;
        if_empty;
          i32_const(quote_str as i32);
          local_get(0); i32_load(4); call(escape_fn);
          call(concat);
          i32_const(quote_str as i32);
          call(concat);
          return_;
        end;
    });

    // Tag 5: array
    wasm!(f, {
        local_get(1); i32_const(5); i32_eq;
        if_empty;
          local_get(0); i32_load(4); local_set(3);
          local_get(3); i32_load(0); local_set(4);
          local_get(4); i32_eqz;
          if_empty; i32_const(empty_arr_str as i32); return_; end;
          i32_const(open_bracket as i32); local_set(2);
          i32_const(0); local_set(5);
    });
    wasm!(f, {
          block_empty; loop_empty;
            local_get(5); local_get(4); i32_ge_u; br_if(1);
            local_get(3); i32_const(list_data_off()); i32_add;
            local_get(5); i32_const(4); i32_mul; i32_add;
            i32_load(0); call(stringify_fn); local_set(6);
            local_get(5); i32_const(0); i32_gt_u;
            if_empty;
              local_get(2); i32_const(comma_str as i32); call(concat); local_set(2);
            end;
            local_get(2); local_get(6); call(concat); local_set(2);
            local_get(5); i32_const(1); i32_add; local_set(5);
            br(0);
          end; end;
          local_get(2); i32_const(close_bracket as i32); call(concat); return_;
        end;
    });

    // Tag 6: object
    wasm!(f, {
        local_get(1); i32_const(6); i32_eq;
        if_empty;
          local_get(0); i32_load(4); local_set(3);
          local_get(3); i32_load(0); local_set(4);
          local_get(4); i32_eqz;
          if_empty; i32_const(empty_obj_str as i32); return_; end;
          i32_const(open_brace as i32); local_set(2);
          i32_const(0); local_set(5);
    });
    wasm!(f, {
          block_empty; loop_empty;
            local_get(5); local_get(4); i32_ge_u; br_if(1);
            // Load tuple pointer: list[8 + i*4] (each list elem is an i32 ptr)
            local_get(3); i32_const(list_data_off()); i32_add;
            local_get(5); i32_const(4); i32_mul; i32_add;
            i32_load(0); // dereference to get tuple ptr
            local_set(6);
            local_get(5); i32_const(0); i32_gt_u;
            if_empty;
              local_get(2); i32_const(comma_str as i32); call(concat); local_set(2);
            end;
    });
    wasm!(f, {
            // tuple layout: [key_str_ptr: i32][value_ptr: i32]
            i32_const(quote_str as i32);
            local_get(6); i32_load(0); // key string ptr
            call(concat);
            i32_const(quote_str as i32); call(concat);
            i32_const(colon_str as i32); call(concat);
            local_get(6); i32_load(4); call(stringify_fn); // value ptr
            call(concat); local_set(7);
            local_get(2); local_get(7); call(concat); local_set(2);
            local_get(5); i32_const(1); i32_add; local_set(5);
            br(0);
          end; end;
          local_get(2); i32_const(close_brace as i32); call(concat); return_;
        end;
    });

    // Fallback
    wasm!(f, { i32_const(null_str as i32); end; });

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
    wasm!(f, { local_get(0); i32_load(0); local_set(2); });

    // Scalars (tag <= 4): delegate to __value_stringify (byte-identical to
    // compact; native pretty also emits the same scalar text).
    wasm!(f, {
        local_get(2); i32_const(5); i32_lt_u;
        if_empty;
          local_get(0); call(stringify_fn); return_;
        end;
    });

    // Build indentation strings: ind = "  ".repeat(depth), ind1 = "  ".repeat(depth+1).
    wasm!(f, {
        i32_const(two_sp as i32); local_get(1); call(repeat); local_set(4);
        i32_const(two_sp as i32); local_get(1); i32_const(1); i32_add; call(repeat); local_set(5);
    });

    // Tag 5: array.
    wasm!(f, {
        local_get(2); i32_const(5); i32_eq;
        if_empty;
          local_get(0); i32_load(4); local_set(6);
          local_get(6); i32_load(0); local_set(7);
          local_get(7); i32_eqz;
          if_empty; i32_const(empty_arr_str as i32); return_; end;
          // result = "[\n"
          i32_const(open_bracket as i32); i32_const(nl_str as i32); call(concat); local_set(3);
          i32_const(0); local_set(8);
    });
    wasm!(f, {
          block_empty; loop_empty;
            local_get(8); local_get(7); i32_ge_u; br_if(1);
            // separator before all but the first element
            local_get(8); i32_const(0); i32_gt_u;
            if_empty;
              local_get(3); i32_const(comma_nl as i32); call(concat); local_set(3);
            end;
            // result += ind1
            local_get(3); local_get(5); call(concat); local_set(3);
            // result += pretty(elem, depth+1)
            local_get(6); i32_const(list_data_off()); i32_add;
            local_get(8); i32_const(4); i32_mul; i32_add;
            i32_load(0);
            local_get(1); i32_const(1); i32_add;
            call(pretty_fn); local_set(9);
            local_get(3); local_get(9); call(concat); local_set(3);
            local_get(8); i32_const(1); i32_add; local_set(8);
            br(0);
          end; end;
          // result += "\n" + ind + "]"
          local_get(3); i32_const(nl_str as i32); call(concat); local_set(3);
          local_get(3); local_get(4); call(concat); local_set(3);
          local_get(3); i32_const(close_bracket as i32); call(concat); return_;
        end;
    });

    // Tag 6: object.
    wasm!(f, {
        local_get(2); i32_const(6); i32_eq;
        if_empty;
          local_get(0); i32_load(4); local_set(6);
          local_get(6); i32_load(0); local_set(7);
          local_get(7); i32_eqz;
          if_empty; i32_const(empty_obj_str as i32); return_; end;
          i32_const(open_brace as i32); i32_const(nl_str as i32); call(concat); local_set(3);
          i32_const(0); local_set(8);
    });
    wasm!(f, {
          block_empty; loop_empty;
            local_get(8); local_get(7); i32_ge_u; br_if(1);
            // load tuple ptr [key_str_ptr][value_ptr]
            local_get(6); i32_const(list_data_off()); i32_add;
            local_get(8); i32_const(4); i32_mul; i32_add;
            i32_load(0); local_set(9);
            local_get(8); i32_const(0); i32_gt_u;
            if_empty;
              local_get(3); i32_const(comma_nl as i32); call(concat); local_set(3);
            end;
            // result += ind1
            local_get(3); local_get(5); call(concat); local_set(3);
            // result += "\"" + escape(key) + "\"" + ": "
            local_get(3); i32_const(quote_str as i32); call(concat);
            local_get(9); i32_load(0); call(escape_fn); call(concat);
            i32_const(quote_str as i32); call(concat);
            i32_const(colon_sp as i32); call(concat); local_set(3);
            // result += pretty(value, depth+1)
            local_get(9); i32_load(4);
            local_get(1); i32_const(1); i32_add;
            call(pretty_fn); local_set(10);
            local_get(3); local_get(10); call(concat); local_set(3);
            local_get(8); i32_const(1); i32_add; local_set(8);
            br(0);
          end; end;
          local_get(3); i32_const(nl_str as i32); call(concat); local_set(3);
          local_get(3); local_get(4); call(concat); local_set(3);
          local_get(3); i32_const(close_brace as i32); call(concat); return_;
        end;
    });

    // Fallback (unreachable for valid Value tags): delegate to compact.
    wasm!(f, { local_get(0); call(stringify_fn); end; });

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

include!("rt_value_p2.rs");
include!("rt_value_p3.rs");
