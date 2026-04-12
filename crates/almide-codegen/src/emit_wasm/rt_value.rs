//! WASM runtime: value.stringify and json.parse.
//!
//! __value_stringify(v: i32) -> i32 (String ptr)
//!   Tag-based dispatch to produce string representation of a Value.
//!
//! __json_parse(s: i32) -> i32 (Result[Value, String] ptr)
//!   Minimal recursive descent JSON parser.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::{Function, ValType};

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
}

/// Compile all runtime function bodies.
pub(super) fn compile(emitter: &mut WasmEmitter) {
    compile_value_stringify(emitter);
    compile_json_parse(emitter);
    compile_json_get_path(emitter);
    compile_json_set_path(emitter);
    compile_json_remove_path(emitter);
    compile_json_escape_string(emitter);
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

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
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
    let ftoa = emitter.rt.float_to_string;
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
          local_get(0); f64_load(4); call(ftoa); return_;
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
            local_get(3); i32_const(4); i32_add;
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
            // Load tuple pointer: list[4 + i*4] (each list elem is an i32 ptr)
            local_get(3); i32_const(4); i32_add;
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

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __json_parse(s: i32) -> i32 (Result[Value, String])
fn compile_json_parse(emitter: &mut WasmEmitter) {
    let parse_at_fn = emitter.rt.json_parse_at;

    let type_idx = emitter.func_type_indices[&emitter.rt.json_parse];
    let alloc = emitter.rt.alloc;

    // param 0 = s, local 1 = parse_result, local 2 = result
    let mut f = Function::new([(2, ValType::I32)]);

    wasm!(f, {
        local_get(0); i32_const(0); call(parse_at_fn); local_set(1);
        local_get(1); i32_load(8);
        if_i32;
          i32_const(8); call(alloc); local_set(2);
          local_get(2); i32_const(1); i32_store(0);
          local_get(2); local_get(1); i32_load(0); i32_store(4);
          local_get(2);
        else_;
          i32_const(8); call(alloc); local_set(2);
          local_get(2); i32_const(0); i32_store(0);
          local_get(2); local_get(1); i32_load(0); i32_store(4);
          local_get(2);
        end;
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });

    compile_json_parse_at(emitter);
}

/// __json_parse_at(str_ptr: i32, pos: i32) -> i32
/// Returns ptr to [value_or_err: i32][new_pos: i32][err_flag: i32]
fn compile_json_parse_at(emitter: &mut WasmEmitter) {
    let parse_at_fn = emitter.rt.json_parse_at;
    let type_idx = emitter.func_type_indices[&parse_at_fn];
    let alloc = emitter.rt.alloc;
    let _concat = emitter.rt.concat_str;
    let _str_eq = emitter.rt.string.eq;

    let err_msg = emitter.intern_string("unexpected character in JSON");
    let err_eof = emitter.intern_string("unexpected end of input");

    // Locals:
    // param 0 = str_ptr, param 1 = pos
    // 2=result_ptr, 3=str_len, 4=ch, 5=start, 6=value_ptr, 7=tmp
    // 8=list_ptr, 9=count, 10=sub_result, 11=sign
    // 12=num_val(i64), 13=divisor(f64)
    let mut f = Function::new([
        (10, ValType::I32),
        (1, ValType::I64),
        (1, ValType::F64),
    ]);

    // Allocate result struct (12 bytes)
    wasm!(f, {
        i32_const(12); call(alloc); local_set(2);
        local_get(0); i32_load(0); local_set(3);
    });

    // Skip whitespace
    emit_skip_ws(&mut f);

    // Check EOF
    wasm!(f, {
        local_get(1); local_get(3); i32_ge_u;
        if_empty;
          local_get(2); i32_const(err_eof as i32); i32_store(0);
          local_get(2); i32_const(0); i32_store(4);
          local_get(2); i32_const(1); i32_store(8);
          local_get(2); return_;
        end;
    });

    // Load current char
    wasm!(f, {
        local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
        i32_load8_u(0); local_set(4);
    });

    // ── null: check n,u,l,l ──
    wasm!(f, {
        local_get(4); i32_const(110); i32_eq; // 'n'
        if_empty;
          // Validate remaining chars: u(117), l(108), l(108)
          local_get(1); i32_const(3); i32_add; local_get(3); i32_lt_u; // need 3 more chars
          local_get(0); i32_const(5); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(117); i32_eq;
          i32_and;
          local_get(0); i32_const(6); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(108); i32_eq;
          i32_and;
          local_get(0); i32_const(7); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(108); i32_eq;
          i32_and;
    });
    wasm!(f, {
          if_empty;
            local_get(1); i32_const(4); i32_add; local_set(1);
            i32_const(4); call(alloc); local_set(6);
            local_get(6); i32_const(0); i32_store(0);
            local_get(2); local_get(6); i32_store(0);
            local_get(2); local_get(1); i32_store(4);
            local_get(2); i32_const(0); i32_store(8);
            local_get(2); return_;
          end;
        end;
    });

    // ── true: check t,r,u,e ──
    wasm!(f, {
        local_get(4); i32_const(116); i32_eq; // 't'
        if_empty;
          local_get(1); i32_const(3); i32_add; local_get(3); i32_lt_u;
          local_get(0); i32_const(5); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(114); i32_eq;
          i32_and;
          local_get(0); i32_const(6); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(117); i32_eq;
          i32_and;
          local_get(0); i32_const(7); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(101); i32_eq;
          i32_and;
    });
    wasm!(f, {
          if_empty;
            local_get(1); i32_const(4); i32_add; local_set(1);
            i32_const(8); call(alloc); local_set(6);
            local_get(6); i32_const(1); i32_store(0);
            local_get(6); i32_const(1); i32_store(4);
            local_get(2); local_get(6); i32_store(0);
            local_get(2); local_get(1); i32_store(4);
            local_get(2); i32_const(0); i32_store(8);
            local_get(2); return_;
          end;
        end;
    });

    // ── false: check f,a,l,s,e ──
    wasm!(f, {
        local_get(4); i32_const(102); i32_eq; // 'f'
        if_empty;
          local_get(1); i32_const(4); i32_add; local_get(3); i32_lt_u;
          local_get(0); i32_const(5); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(97); i32_eq;
          i32_and;
          local_get(0); i32_const(6); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(108); i32_eq;
          i32_and;
          local_get(0); i32_const(7); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(115); i32_eq;
          i32_and;
          local_get(0); i32_const(8); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(101); i32_eq;
          i32_and;
    });
    wasm!(f, {
          if_empty;
            local_get(1); i32_const(5); i32_add; local_set(1);
            i32_const(8); call(alloc); local_set(6);
            local_get(6); i32_const(1); i32_store(0);
            local_get(6); i32_const(0); i32_store(4);
            local_get(2); local_get(6); i32_store(0);
            local_get(2); local_get(1); i32_store(4);
            local_get(2); i32_const(0); i32_store(8);
            local_get(2); return_;
          end;
        end;
    });

    // ── String ──
    emit_parse_string(&mut f, alloc);

    // ── Number ──
    emit_parse_number(&mut f, alloc);

    // ── Array ──
    emit_parse_array(&mut f, alloc, parse_at_fn);

    // ── Object ──
    emit_parse_object(&mut f, alloc, parse_at_fn);

    // Fallback: error
    wasm!(f, {
        local_get(2); i32_const(err_msg as i32); i32_store(0);
        local_get(2); i32_const(0); i32_store(4);
        local_get(2); i32_const(1); i32_store(8);
        local_get(2);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// Emit whitespace-skipping loop.
/// Uses locals: 0=str_ptr, 1=pos, 3=str_len, 4=ch
fn emit_skip_ws(f: &mut Function) {
    wasm!(f, {
        block_empty; loop_empty;
          local_get(1); local_get(3); i32_ge_u; br_if(1);
          local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
          i32_load8_u(0); local_set(4);
          local_get(4); i32_const(32); i32_eq;
          local_get(4); i32_const(9); i32_eq; i32_or;
          local_get(4); i32_const(10); i32_eq; i32_or;
          local_get(4); i32_const(13); i32_eq; i32_or;
          i32_eqz; br_if(1);
          local_get(1); i32_const(1); i32_add; local_set(1);
          br(0);
        end; end;
    });
}

/// Parse JSON string starting at current pos (ch=='"').
/// Uses locals: 0=str_ptr, 1=pos, 2=result_ptr, 3=str_len, 4=ch, 5=start, 6=value_ptr, 7=tmp, 9=count
fn emit_parse_string(f: &mut Function, alloc: u32) {
    wasm!(f, {
        local_get(4); i32_const(34); i32_eq;
        if_empty;
          local_get(1); i32_const(1); i32_add; local_set(1);
          local_get(1); local_set(5);
    });
    // Find closing quote
    wasm!(f, {
          block_empty; loop_empty;
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(7);
            local_get(7); i32_const(34); i32_eq; br_if(1);
            local_get(7); i32_const(92); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
            end;
            local_get(1); i32_const(1); i32_add; local_set(1);
            br(0);
          end; end;
    });
    // Build string
    wasm!(f, {
          local_get(1); local_get(5); i32_sub; local_set(7);
          i32_const(4); local_get(7); i32_add; call(alloc); local_set(6);
          local_get(6); local_get(7); i32_store(0);
          i32_const(0); local_set(9);
    });
    // Copy bytes loop
    wasm!(f, {
          block_empty; loop_empty;
            local_get(9); local_get(7); i32_ge_u; br_if(1);
            local_get(6); i32_const(4); i32_add; local_get(9); i32_add;
            local_get(0); i32_const(4); i32_add; local_get(5); i32_add; local_get(9); i32_add;
            i32_load8_u(0);
            i32_store8(0);
            local_get(9); i32_const(1); i32_add; local_set(9);
            br(0);
          end; end;
    });
    // Build Value and return
    wasm!(f, {
          local_get(1); i32_const(1); i32_add; local_set(1);
          i32_const(8); call(alloc); local_set(7);
          local_get(7); i32_const(4); i32_store(0);
          local_get(7); local_get(6); i32_store(4);
          local_get(2); local_get(7); i32_store(0);
          local_get(2); local_get(1); i32_store(4);
          local_get(2); i32_const(0); i32_store(8);
          local_get(2); return_;
        end;
    });
}

/// Parse JSON number. Uses: 0=str_ptr, 1=pos, 2=result_ptr, 3=str_len, 4=ch,
/// 5=start, 6=value_ptr, 11=sign, 12=num_val(i64), 13=divisor(f64)
fn emit_parse_number(f: &mut Function, alloc: u32) {
    // Check if number
    wasm!(f, {
        local_get(4); i32_const(45); i32_eq;
        local_get(4); i32_const(48); i32_ge_u;
        local_get(4); i32_const(57); i32_le_u;
        i32_and; i32_or;
        if_empty;
          i32_const(1); local_set(11);
          i64_const(0); local_set(12);
          local_get(4); i32_const(45); i32_eq;
          if_empty;
            i32_const(-1); local_set(11);
            local_get(1); i32_const(1); i32_add; local_set(1);
          end;
    });
    // Parse integer digits
    wasm!(f, {
          block_empty; loop_empty;
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(4);
            local_get(4); i32_const(48); i32_lt_u; br_if(1);
            local_get(4); i32_const(57); i32_gt_u; br_if(1);
            local_get(12); i64_const(10); i64_mul;
            local_get(4); i32_const(48); i32_sub; i64_extend_i32_u;
            i64_add; local_set(12);
            local_get(1); i32_const(1); i32_add; local_set(1);
            br(0);
          end; end;
    });
    // Check for decimal point
    wasm!(f, {
          local_get(1); local_get(3); i32_lt_u;
          if_empty;
            local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
            i32_load8_u(0); i32_const(46); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
              f64_const(1.0); local_set(13);
    });
    // Parse decimal digits
    wasm!(f, {
              block_empty; loop_empty;
                local_get(1); local_get(3); i32_ge_u; br_if(1);
                local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
                i32_load8_u(0); local_set(4);
                local_get(4); i32_const(48); i32_lt_u; br_if(1);
                local_get(4); i32_const(57); i32_gt_u; br_if(1);
                local_get(12); i64_const(10); i64_mul;
                local_get(4); i32_const(48); i32_sub; i64_extend_i32_u;
                i64_add; local_set(12);
                local_get(13); f64_const(10.0); f64_mul; local_set(13);
                local_get(1); i32_const(1); i32_add; local_set(1);
                br(0);
              end; end;
    });
    // Build float Value
    wasm!(f, {
              i32_const(12); call(alloc); local_set(6);
              local_get(6); i32_const(3); i32_store(0);
              local_get(6);
              local_get(11); i64_extend_i32_s; local_get(12); i64_mul;
              f64_convert_i64_s; local_get(13); f64_div;
              f64_store(4);
              local_get(2); local_get(6); i32_store(0);
              local_get(2); local_get(1); i32_store(4);
              local_get(2); i32_const(0); i32_store(8);
              local_get(2); return_;
            end;
          end;
    });
    // Build int Value
    wasm!(f, {
          i32_const(12); call(alloc); local_set(6);
          local_get(6); i32_const(2); i32_store(0);
          local_get(6);
          local_get(11); i64_extend_i32_s; local_get(12); i64_mul;
          i64_store(4);
          local_get(2); local_get(6); i32_store(0);
          local_get(2); local_get(1); i32_store(4);
          local_get(2); i32_const(0); i32_store(8);
          local_get(2); return_;
        end;
    });
}

/// Parse JSON array.
fn emit_parse_array(f: &mut Function, alloc: u32, parse_at_fn: u32) {
    wasm!(f, {
        local_get(4); i32_const(91); i32_eq;
        if_empty;
          local_get(1); i32_const(1); i32_add; local_set(1);
    });
    // Skip whitespace (inline)
    wasm!(f, {
          block_empty; loop_empty;
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(4);
            local_get(4); i32_const(32); i32_eq;
            local_get(4); i32_const(9); i32_eq; i32_or;
            local_get(4); i32_const(10); i32_eq; i32_or;
            local_get(4); i32_const(13); i32_eq; i32_or;
            i32_eqz; br_if(1);
            local_get(1); i32_const(1); i32_add; local_set(1);
            br(0);
          end; end;
    });
    // Check empty array
    wasm!(f, {
          local_get(1); local_get(3); i32_lt_u;
          if_empty;
            local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
            i32_load8_u(0); i32_const(93); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
              i32_const(4); call(alloc); local_set(8);
              local_get(8); i32_const(0); i32_store(0);
              i32_const(8); call(alloc); local_set(6);
              local_get(6); i32_const(5); i32_store(0);
              local_get(6); local_get(8); i32_store(4);
              local_get(2); local_get(6); i32_store(0);
              local_get(2); local_get(1); i32_store(4);
              local_get(2); i32_const(0); i32_store(8);
              local_get(2); return_;
            end;
          end;
    });
    // Parse elements
    wasm!(f, {
          i32_const(260); call(alloc); local_set(8);
          i32_const(0); local_set(9);
          block_empty; loop_empty;
            local_get(0); local_get(1);
            call(parse_at_fn); local_set(10);
            local_get(10); i32_load(8);
            if_empty;
              local_get(2); local_get(10); i32_load(0); i32_store(0);
              local_get(2); i32_const(0); i32_store(4);
              local_get(2); i32_const(1); i32_store(8);
              local_get(2); return_;
            end;
    });
    wasm!(f, {
            local_get(8); i32_const(4); i32_add;
            local_get(9); i32_const(4); i32_mul; i32_add;
            local_get(10); i32_load(0); i32_store(0);
            local_get(10); i32_load(4); local_set(1);
            local_get(9); i32_const(1); i32_add; local_set(9);
    });
    // Skip whitespace after element
    wasm!(f, {
            block_empty; loop_empty;
              local_get(1); local_get(3); i32_ge_u; br_if(1);
              local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
              i32_load8_u(0); local_set(4);
              local_get(4); i32_const(32); i32_eq;
              local_get(4); i32_const(9); i32_eq; i32_or;
              local_get(4); i32_const(10); i32_eq; i32_or;
              local_get(4); i32_const(13); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(0);
            end; end;
    });
    // Check ] or ,
    wasm!(f, {
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(4);
            local_get(4); i32_const(93); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(2);
            end;
            local_get(4); i32_const(44); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
            end;
            br(0);
          end; end;
    });
    // Build result
    wasm!(f, {
          local_get(8); local_get(9); i32_store(0);
          i32_const(8); call(alloc); local_set(6);
          local_get(6); i32_const(5); i32_store(0);
          local_get(6); local_get(8); i32_store(4);
          local_get(2); local_get(6); i32_store(0);
          local_get(2); local_get(1); i32_store(4);
          local_get(2); i32_const(0); i32_store(8);
          local_get(2); return_;
        end;
    });
}

/// Parse JSON object.
fn emit_parse_object(f: &mut Function, alloc: u32, parse_at_fn: u32) {
    wasm!(f, {
        local_get(4); i32_const(123); i32_eq;
        if_empty;
          local_get(1); i32_const(1); i32_add; local_set(1);
    });
    // Skip whitespace
    wasm!(f, {
          block_empty; loop_empty;
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(4);
            local_get(4); i32_const(32); i32_eq;
            local_get(4); i32_const(9); i32_eq; i32_or;
            local_get(4); i32_const(10); i32_eq; i32_or;
            local_get(4); i32_const(13); i32_eq; i32_or;
            i32_eqz; br_if(1);
            local_get(1); i32_const(1); i32_add; local_set(1);
            br(0);
          end; end;
    });
    // Check empty object
    wasm!(f, {
          local_get(1); local_get(3); i32_lt_u;
          if_empty;
            local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
            i32_load8_u(0); i32_const(125); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
              i32_const(4); call(alloc); local_set(8);
              local_get(8); i32_const(0); i32_store(0);
              i32_const(8); call(alloc); local_set(6);
              local_get(6); i32_const(6); i32_store(0);
              local_get(6); local_get(8); i32_store(4);
              local_get(2); local_get(6); i32_store(0);
              local_get(2); local_get(1); i32_store(4);
              local_get(2); i32_const(0); i32_store(8);
              local_get(2); return_;
            end;
          end;
    });
    // Parse key-value pairs
    wasm!(f, {
          i32_const(260); call(alloc); local_set(8);
          i32_const(0); local_set(9);
          block_empty; loop_empty;
    });
    // Skip whitespace before key
    wasm!(f, {
            block_empty; loop_empty;
              local_get(1); local_get(3); i32_ge_u; br_if(1);
              local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
              i32_load8_u(0); local_set(4);
              local_get(4); i32_const(32); i32_eq;
              local_get(4); i32_const(9); i32_eq; i32_or;
              local_get(4); i32_const(10); i32_eq; i32_or;
              local_get(4); i32_const(13); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(0);
            end; end;
    });
    // Parse key
    wasm!(f, {
            local_get(0); local_get(1);
            call(parse_at_fn); local_set(10);
            local_get(10); i32_load(8);
            if_empty;
              local_get(2); local_get(10); i32_load(0); i32_store(0);
              local_get(2); i32_const(0); i32_store(4);
              local_get(2); i32_const(1); i32_store(8);
              local_get(2); return_;
            end;
            local_get(10); i32_load(0); i32_load(4); local_set(7);
            local_get(10); i32_load(4); local_set(1);
    });
    // Skip whitespace and colon
    wasm!(f, {
            block_empty; loop_empty;
              local_get(1); local_get(3); i32_ge_u; br_if(1);
              local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
              i32_load8_u(0); local_set(4);
              local_get(4); i32_const(32); i32_eq;
              local_get(4); i32_const(9); i32_eq; i32_or;
              local_get(4); i32_const(10); i32_eq; i32_or;
              local_get(4); i32_const(13); i32_eq; i32_or;
              local_get(4); i32_const(58); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(0);
            end; end;
    });
    // Parse value
    wasm!(f, {
            local_get(0); local_get(1);
            call(parse_at_fn); local_set(10);
            local_get(10); i32_load(8);
            if_empty;
              local_get(2); local_get(10); i32_load(0); i32_store(0);
              local_get(2); i32_const(0); i32_store(4);
              local_get(2); i32_const(1); i32_store(8);
              local_get(2); return_;
            end;
    });
    // Allocate tuple (key_str_ptr, value_ptr) and store pointer in list
    wasm!(f, {
            // Allocate 8-byte tuple: [key_str_ptr: i32][value_ptr: i32]
            i32_const(8); call(alloc); local_set(5); // reuse local 5 as tuple_ptr
            local_get(5); local_get(7); i32_store(0); // key
            local_get(5); local_get(10); i32_load(0); i32_store(4); // value
            // Store tuple pointer in list at position count
            local_get(8); i32_const(4); i32_add;
            local_get(9); i32_const(4); i32_mul; i32_add;
            local_get(5); i32_store(0);
            local_get(10); i32_load(4); local_set(1);
            local_get(9); i32_const(1); i32_add; local_set(9);
    });
    // Skip whitespace after value
    wasm!(f, {
            block_empty; loop_empty;
              local_get(1); local_get(3); i32_ge_u; br_if(1);
              local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
              i32_load8_u(0); local_set(4);
              local_get(4); i32_const(32); i32_eq;
              local_get(4); i32_const(9); i32_eq; i32_or;
              local_get(4); i32_const(10); i32_eq; i32_or;
              local_get(4); i32_const(13); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(0);
            end; end;
    });
    // Check } or ,
    wasm!(f, {
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(4);
            local_get(4); i32_const(125); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(2);
            end;
            local_get(4); i32_const(44); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
            end;
            br(0);
          end; end;
    });
    // Build object result
    wasm!(f, {
          local_get(8); local_get(9); i32_store(0);
          i32_const(8); call(alloc); local_set(6);
          local_get(6); i32_const(6); i32_store(0);
          local_get(6); local_get(8); i32_store(4);
          local_get(2); local_get(6); i32_store(0);
          local_get(2); local_get(1); i32_store(4);
          local_get(2); i32_const(0); i32_store(8);
          local_get(2); return_;
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
        i32_const(0); local_set(2);     // seg_count = 0
        local_get(1); local_set(3);     // cur_path = path
        block_empty; loop_empty;
          local_get(3); i32_load(0);    // tag
          i32_eqz; br_if(1);           // tag==0 (root) → done
          local_get(2); i32_const(1); i32_add; local_set(2);
          local_get(3); i32_load(4); local_set(3); // cur_path = parent
          br(0);
        end; end;
    });

    // --- Phase 2: Allocate segments array and fill in reverse ---
    // segs_arr = alloc(seg_count * 4), each slot is a path node ptr.
    wasm!(f, {
        local_get(2); i32_eqz;
        if_empty;
          // Empty path → return some(value): alloc option box
          i32_const(4); call(alloc); local_set(13);
          local_get(13); local_get(0); i32_store(0);
          local_get(13);
          return_;
        end;
        local_get(2); i32_const(4); i32_mul; call(alloc); local_set(4); // segs_arr
        local_get(2); local_set(5); // i = seg_count (fill from end)
        local_get(1); local_set(3); // cur_path = path (start from leaf)
        block_empty; loop_empty;
          local_get(3); i32_load(0); i32_eqz; br_if(1); // root → done
          local_get(5); i32_const(1); i32_sub; local_set(5); // i--
          local_get(4); local_get(5); i32_const(4); i32_mul; i32_add;
          local_get(3); i32_store(0); // segs_arr[i] = cur_path
          local_get(3); i32_load(4); local_set(3); // cur_path = parent
          br(0);
        end; end;
    });

    // --- Phase 3: Walk value following segments ---
    // cur_val = value
    wasm!(f, {
        local_get(0); local_set(8); // cur_val = value
        i32_const(0); local_set(5); // i = 0
        block_empty; loop_empty;
          local_get(5); local_get(2); i32_ge_u; br_if(1); // i >= seg_count → done
          // Load segment
          local_get(4); local_get(5); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(6); // seg_ptr
          local_get(6); i32_load(0); local_set(7); // seg_tag
    });

    // --- Field segment (tag=1) ---
    wasm!(f, {
          local_get(7); i32_const(1); i32_eq;
          if_empty;
            // cur_val must be object (tag=6)
            local_get(8); i32_load(0); i32_const(6); i32_ne;
            if_empty; i32_const(0); return_; end; // not object → none
            local_get(8); i32_load(4); local_set(9); // list (pairs)
            local_get(9); i32_load(0); local_set(10); // len
            i32_const(0); local_set(11); // j = 0
            i32_const(0); local_set(13); // found = 0
            block_empty; loop_empty;
              local_get(11); local_get(10); i32_ge_u; br_if(1);
              local_get(9); i32_const(4); i32_add;
              local_get(11); i32_const(4); i32_mul; i32_add;
              i32_load(0); local_set(12); // pair_ptr
              local_get(12); i32_load(0); // pair key
              local_get(6); i32_load(8); // segment field name
              call(str_eq);
              if_empty;
                local_get(12); i32_load(4); local_set(8); // cur_val = pair value
                i32_const(1); local_set(13); // found = 1
                br(2);
              end;
              local_get(11); i32_const(1); i32_add; local_set(11);
              br(0);
            end; end;
            local_get(13); i32_eqz;
            if_empty; i32_const(0); return_; end; // key not found → none
          end;
    });

    // --- Index segment (tag=2) ---
    wasm!(f, {
          local_get(7); i32_const(2); i32_eq;
          if_empty;
            // cur_val must be array (tag=5)
            local_get(8); i32_load(0); i32_const(5); i32_ne;
            if_empty; i32_const(0); return_; end; // not array → none
            local_get(8); i32_load(4); local_set(9); // list
            local_get(9); i32_load(0); local_set(10); // len
            local_get(6); i32_load(8); local_set(11); // index value
            // Bounds check
            local_get(11); i32_const(0); i32_lt_s;
            local_get(11); local_get(10); i32_ge_s;
            i32_or;
            if_empty; i32_const(0); return_; end; // out of bounds → none
            // cur_val = list[index]
            local_get(9); i32_const(4); i32_add;
            local_get(11); i32_const(4); i32_mul; i32_add;
            i32_load(0); local_set(8);
          end;
    });

    // --- Next segment ---
    wasm!(f, {
          local_get(5); i32_const(1); i32_add; local_set(5);
          br(0);
        end; end;
    });

    // --- Return some(cur_val): alloc Option box ---
    wasm!(f, {
        i32_const(4); call(alloc); local_set(13);
        local_get(13); local_get(8); i32_store(0);
        local_get(13);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __json_set_path(value: i32, path: i32, new_val: i32) -> i32 (Result[Value, String])
///
/// Linearize path, then iteratively walk down saving values at each depth,
/// then rebuild from leaf to root replacing at the target path.
fn compile_json_set_path(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.json_set_path];
    let alloc = emitter.rt.alloc;
    let str_eq = emitter.rt.string.eq;

    let err_not_obj = emitter.intern_string("path error: expected object");
    let err_not_arr = emitter.intern_string("path error: expected array");
    let err_oob = emitter.intern_string("path error: index out of bounds");

    // Locals: param 0=value, param 1=path, param 2=new_val
    // 3=seg_count, 4=cur_path, 5=segs_arr, 6=depth, 7=seg_ptr, 8=seg_tag
    // 9=cur_val, 10=list, 11=len, 12=j, 13=pair_ptr, 14=result
    // 15=new_list, 16=val_stack, 17=found, 18=idx
    let mut f = Function::new([(16, ValType::I32)]);

    // --- Phase 1: Count segments ---
    wasm!(f, {
        i32_const(0); local_set(3);
        local_get(1); local_set(4);
        block_empty; loop_empty;
          local_get(4); i32_load(0); i32_eqz; br_if(1);
          local_get(3); i32_const(1); i32_add; local_set(3);
          local_get(4); i32_load(4); local_set(4);
          br(0);
        end; end;
    });

    // --- Phase 2: Allocate and fill segments array ---
    wasm!(f, {
        local_get(3); i32_eqz;
        if_empty;
          // Empty path → ok(new_val)
          i32_const(8); call(alloc); local_set(14);
          local_get(14); i32_const(0); i32_store(0);
          local_get(14); local_get(2); i32_store(4);
          local_get(14);
          return_;
        end;
        local_get(3); i32_const(4); i32_mul; call(alloc); local_set(5);
        local_get(3); local_set(6);
        local_get(1); local_set(4);
        block_empty; loop_empty;
          local_get(4); i32_load(0); i32_eqz; br_if(1);
          local_get(6); i32_const(1); i32_sub; local_set(6);
          local_get(5); local_get(6); i32_const(4); i32_mul; i32_add;
          local_get(4); i32_store(0);
          local_get(4); i32_load(4); local_set(4);
          br(0);
        end; end;
    });

    // --- Phase 3: Walk forward saving values at each depth ---
    // val_stack = alloc((seg_count+1) * 4): val_stack[d] = value at depth d
    wasm!(f, {
        local_get(3); i32_const(1); i32_add; i32_const(4); i32_mul;
        call(alloc); local_set(16); // val_stack
        local_get(16); local_get(0); i32_store(0); // val_stack[0] = value
        i32_const(0); local_set(6); // depth = 0
        block_empty; loop_empty;
          local_get(6); local_get(3); i32_const(1); i32_sub; i32_ge_u; br_if(1); // depth >= seg_count-1 → done
          local_get(5); local_get(6); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(7); // seg_ptr = segs_arr[depth]
          local_get(7); i32_load(0); local_set(8); // seg_tag
          // Load cur_val from val_stack[depth]
          local_get(16); local_get(6); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(9);
    });

    // Navigate field during forward walk
    wasm!(f, {
          local_get(8); i32_const(1); i32_eq;
          if_empty;
            local_get(9); i32_load(0); i32_const(6); i32_ne;
            if_empty;
              i32_const(8); call(alloc); local_set(14);
              local_get(14); i32_const(1); i32_store(0);
              local_get(14); i32_const(err_not_obj as i32); i32_store(4);
              local_get(14); return_;
            end;
            local_get(9); i32_load(4); local_set(10); // pairs list
            local_get(10); i32_load(0); local_set(11); // len
            i32_const(0); local_set(12);
            i32_const(0); local_set(17); // found = 0
            block_empty; loop_empty;
              local_get(12); local_get(11); i32_ge_u; br_if(1);
              local_get(10); i32_const(4); i32_add;
              local_get(12); i32_const(4); i32_mul; i32_add;
              i32_load(0); local_set(13); // pair
              local_get(13); i32_load(0);
              local_get(7); i32_load(8);
              call(str_eq);
              if_empty;
                local_get(16); local_get(6); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
                local_get(13); i32_load(4); i32_store(0);
                i32_const(1); local_set(17);
                br(2);
              end;
              local_get(12); i32_const(1); i32_add; local_set(12);
              br(0);
            end; end;
            // If key not found, store null placeholder for the next level
            local_get(17); i32_eqz;
            if_empty;
              i32_const(4); call(alloc); local_set(17);
              local_get(17); i32_const(0); i32_store(0); // null value
              local_get(16); local_get(6); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
              local_get(17); i32_store(0);
            end;
          end;
    });

    // Navigate index during forward walk
    wasm!(f, {
          local_get(8); i32_const(2); i32_eq;
          if_empty;
            local_get(9); i32_load(0); i32_const(5); i32_ne;
            if_empty;
              i32_const(8); call(alloc); local_set(14);
              local_get(14); i32_const(1); i32_store(0);
              local_get(14); i32_const(err_not_arr as i32); i32_store(4);
              local_get(14); return_;
            end;
            local_get(9); i32_load(4); local_set(10); // list
            local_get(10); i32_load(0); local_set(11); // len
            local_get(7); i32_load(8); local_set(18); // idx
            local_get(18); i32_const(0); i32_lt_s;
            local_get(18); local_get(11); i32_ge_s;
            i32_or;
            if_empty;
              i32_const(8); call(alloc); local_set(14);
              local_get(14); i32_const(1); i32_store(0);
              local_get(14); i32_const(err_oob as i32); i32_store(4);
              local_get(14); return_;
            end;
            local_get(16); local_get(6); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
            local_get(10); i32_const(4); i32_add;
            local_get(18); i32_const(4); i32_mul; i32_add;
            i32_load(0); i32_store(0);
          end;
    });

    // Next depth in forward walk
    wasm!(f, {
          local_get(6); i32_const(1); i32_add; local_set(6);
          br(0);
        end; end;
    });

    // --- Phase 4: Rebuild from leaf to root ---
    // cur_built starts as new_val, then we wrap it at each level going backwards.
    wasm!(f, {
        local_get(2); local_set(9); // cur_built = new_val
        local_get(3); i32_const(1); i32_sub; local_set(6); // depth = seg_count - 1
        block_empty; loop_empty;
          local_get(6); i32_const(0); i32_lt_s; br_if(1);
          local_get(5); local_get(6); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(7); // seg
          local_get(7); i32_load(0); local_set(8); // seg_tag
          // orig_val at this depth
          local_get(16); local_get(6); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(14);
    });

    // Rebuild for field segment
    wasm!(f, {
          local_get(8); i32_const(1); i32_eq;
          if_empty;
            local_get(14); i32_load(0); i32_const(6); i32_eq;
            if_empty;
              // Clone pairs, replacing matching key
              local_get(14); i32_load(4); local_set(10); // old pairs
              local_get(10); i32_load(0); local_set(11); // old len
              // Check if key exists
              i32_const(0); local_set(17);
              i32_const(0); local_set(12);
              block_empty; loop_empty;
                local_get(12); local_get(11); i32_ge_u; br_if(1);
                local_get(10); i32_const(4); i32_add;
                local_get(12); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(13);
                local_get(13); i32_load(0);
                local_get(7); i32_load(8);
                call(str_eq);
                if_empty; i32_const(1); local_set(17); end;
                local_get(12); i32_const(1); i32_add; local_set(12);
                br(0);
              end; end;
              // new_len = old_len + (found ? 0 : 1)
              local_get(11); local_get(17); i32_eqz; i32_add; local_set(18);
              // Alloc new pairs list
              i32_const(4); local_get(18); i32_const(4); i32_mul; i32_add;
              call(alloc); local_set(15);
              local_get(15); local_get(18); i32_store(0);
              // Copy, replacing match
              i32_const(0); local_set(12);
              block_empty; loop_empty;
                local_get(12); local_get(11); i32_ge_u; br_if(1);
                local_get(10); i32_const(4); i32_add;
                local_get(12); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(13);
                local_get(13); i32_load(0);
                local_get(7); i32_load(8);
                call(str_eq);
                if_empty;
                  // Replace value
                  i32_const(8); call(alloc); local_set(17);
                  local_get(17); local_get(13); i32_load(0); i32_store(0);
                  local_get(17); local_get(9); i32_store(4);
                  local_get(15); i32_const(4); i32_add;
                  local_get(12); i32_const(4); i32_mul; i32_add;
                  local_get(17); i32_store(0);
                else_;
                  local_get(15); i32_const(4); i32_add;
                  local_get(12); i32_const(4); i32_mul; i32_add;
                  local_get(13); i32_store(0);
                end;
                local_get(12); i32_const(1); i32_add; local_set(12);
                br(0);
              end; end;
              // Append new pair if key was not found
              local_get(18); local_get(11); i32_gt_u;
              if_empty;
                i32_const(8); call(alloc); local_set(17);
                local_get(17); local_get(7); i32_load(8); i32_store(0);
                local_get(17); local_get(9); i32_store(4);
                local_get(15); i32_const(4); i32_add;
                local_get(11); i32_const(4); i32_mul; i32_add;
                local_get(17); i32_store(0);
              end;
              // Build object
              i32_const(8); call(alloc); local_set(9);
              local_get(9); i32_const(6); i32_store(0);
              local_get(9); local_get(15); i32_store(4);
            else_;
              // Not an object: create single-key object
              i32_const(8); call(alloc); local_set(15); // list (1 slot + header)
              local_get(15); i32_const(1); i32_store(0);
              i32_const(8); call(alloc); local_set(17); // pair
              local_get(17); local_get(7); i32_load(8); i32_store(0);
              local_get(17); local_get(9); i32_store(4);
              local_get(15); i32_const(4); i32_add; local_get(17); i32_store(0);
              i32_const(8); call(alloc); local_set(9);
              local_get(9); i32_const(6); i32_store(0);
              local_get(9); local_get(15); i32_store(4);
            end;
          end;
    });

    // Rebuild for index segment
    wasm!(f, {
          local_get(8); i32_const(2); i32_eq;
          if_empty;
            local_get(14); i32_load(4); local_set(10); // old list
            local_get(10); i32_load(0); local_set(11); // len
            local_get(7); i32_load(8); local_set(18); // idx
            // Clone list replacing at idx
            i32_const(4); local_get(11); i32_const(4); i32_mul; i32_add;
            call(alloc); local_set(15);
            local_get(15); local_get(11); i32_store(0);
            i32_const(0); local_set(12);
            block_empty; loop_empty;
              local_get(12); local_get(11); i32_ge_u; br_if(1);
              local_get(15); i32_const(4); i32_add;
              local_get(12); i32_const(4); i32_mul; i32_add;
              local_get(12); local_get(18); i32_eq;
              if_i32; local_get(9);
              else_;
                local_get(10); i32_const(4); i32_add;
                local_get(12); i32_const(4); i32_mul; i32_add;
                i32_load(0);
              end;
              i32_store(0);
              local_get(12); i32_const(1); i32_add; local_set(12);
              br(0);
            end; end;
            i32_const(8); call(alloc); local_set(9);
            local_get(9); i32_const(5); i32_store(0);
            local_get(9); local_get(15); i32_store(4);
          end;
    });

    // Next depth upward
    wasm!(f, {
          local_get(6); i32_const(1); i32_sub; local_set(6);
          br(0);
        end; end;
    });

    // Return ok(result)
    wasm!(f, {
        i32_const(8); call(alloc); local_set(14);
        local_get(14); i32_const(0); i32_store(0);
        local_get(14); local_get(9); i32_store(4);
        local_get(14);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
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
        i32_const(0); local_set(2);
        local_get(1); local_set(3);
        block_empty; loop_empty;
          local_get(3); i32_load(0); i32_eqz; br_if(1);
          local_get(2); i32_const(1); i32_add; local_set(2);
          local_get(3); i32_load(4); local_set(3);
          br(0);
        end; end;
    });

    // --- Phase 2: Allocate and fill segments array ---
    wasm!(f, {
        local_get(2); i32_eqz;
        if_empty;
          // Empty path → return null (removing root itself)
          i32_const(4); call(alloc); local_set(16);
          local_get(16); i32_const(0); i32_store(0);
          local_get(16);
          return_;
        end;
        local_get(2); i32_const(4); i32_mul; call(alloc); local_set(4);
        local_get(2); local_set(5);
        local_get(1); local_set(3);
        block_empty; loop_empty;
          local_get(3); i32_load(0); i32_eqz; br_if(1);
          local_get(5); i32_const(1); i32_sub; local_set(5);
          local_get(4); local_get(5); i32_const(4); i32_mul; i32_add;
          local_get(3); i32_store(0);
          local_get(3); i32_load(4); local_set(3);
          br(0);
        end; end;
    });

    // --- Phase 3: Walk forward saving values at each depth (all but last) ---
    wasm!(f, {
        local_get(2); i32_const(4); i32_mul; call(alloc); local_set(14); // val_stack
        local_get(14); local_get(0); i32_store(0); // val_stack[0] = value
        i32_const(0); local_set(5); // depth = 0
        block_empty; loop_empty;
          local_get(5); local_get(2); i32_const(1); i32_sub; i32_ge_u; br_if(1);
          local_get(4); local_get(5); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(6);
          local_get(6); i32_load(0); local_set(7);
          local_get(14); local_get(5); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(8);
    });

    // Navigate field for walk
    wasm!(f, {
          local_get(7); i32_const(1); i32_eq;
          if_empty;
            local_get(8); i32_load(0); i32_const(6); i32_ne;
            if_empty; local_get(0); return_; end;
            local_get(8); i32_load(4); local_set(9);
            local_get(9); i32_load(0); local_set(10);
            i32_const(0); local_set(11);
            i32_const(0); local_set(13);
            block_empty; loop_empty;
              local_get(11); local_get(10); i32_ge_u; br_if(1);
              local_get(9); i32_const(4); i32_add;
              local_get(11); i32_const(4); i32_mul; i32_add;
              i32_load(0); local_set(12);
              local_get(12); i32_load(0);
              local_get(6); i32_load(8);
              call(str_eq);
              if_empty;
                local_get(14); local_get(5); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
                local_get(12); i32_load(4); i32_store(0);
                i32_const(1); local_set(13);
                br(2);
              end;
              local_get(11); i32_const(1); i32_add; local_set(11);
              br(0);
            end; end;
            local_get(13); i32_eqz;
            if_empty; local_get(0); return_; end;
          end;
    });

    // Navigate index for walk
    wasm!(f, {
          local_get(7); i32_const(2); i32_eq;
          if_empty;
            local_get(8); i32_load(0); i32_const(5); i32_ne;
            if_empty; local_get(0); return_; end;
            local_get(8); i32_load(4); local_set(9);
            local_get(9); i32_load(0); local_set(10);
            local_get(6); i32_load(8); local_set(17);
            local_get(17); i32_const(0); i32_lt_s;
            local_get(17); local_get(10); i32_ge_s;
            i32_or;
            if_empty; local_get(0); return_; end;
            local_get(14); local_get(5); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
            local_get(9); i32_const(4); i32_add;
            local_get(17); i32_const(4); i32_mul; i32_add;
            i32_load(0); i32_store(0);
          end;
    });

    wasm!(f, {
          local_get(5); i32_const(1); i32_add; local_set(5);
          br(0);
        end; end;
    });

    // --- Phase 4: Remove at the last segment ---
    // Load last segment and value at that depth
    wasm!(f, {
        local_get(4); local_get(2); i32_const(1); i32_sub; i32_const(4); i32_mul; i32_add;
        i32_load(0); local_set(6);
        local_get(6); i32_load(0); local_set(7);
        local_get(14); local_get(2); i32_const(1); i32_sub; i32_const(4); i32_mul; i32_add;
        i32_load(0); local_set(8);
    });

    // Remove field from object
    wasm!(f, {
        local_get(7); i32_const(1); i32_eq;
        if_empty;
          local_get(8); i32_load(0); i32_const(6); i32_ne;
          if_empty; local_get(0); return_; end;
          local_get(8); i32_load(4); local_set(9);
          local_get(9); i32_load(0); local_set(10);
          // Alloc new list (worst case same size)
          i32_const(4); local_get(10); i32_const(4); i32_mul; i32_add;
          call(alloc); local_set(15);
          i32_const(0); local_set(11); // src
          i32_const(0); local_set(18); // dst
          block_empty; loop_empty;
            local_get(11); local_get(10); i32_ge_u; br_if(1);
            local_get(9); i32_const(4); i32_add;
            local_get(11); i32_const(4); i32_mul; i32_add;
            i32_load(0); local_set(12);
            local_get(12); i32_load(0);
            local_get(6); i32_load(8);
            call(str_eq);
            i32_eqz;
            if_empty;
              local_get(15); i32_const(4); i32_add;
              local_get(18); i32_const(4); i32_mul; i32_add;
              local_get(12); i32_store(0);
              local_get(18); i32_const(1); i32_add; local_set(18);
            end;
            local_get(11); i32_const(1); i32_add; local_set(11);
            br(0);
          end; end;
          local_get(15); local_get(18); i32_store(0); // set actual len
          i32_const(8); call(alloc); local_set(16);
          local_get(16); i32_const(6); i32_store(0);
          local_get(16); local_get(15); i32_store(4);
        end;
    });

    // Remove index from array
    wasm!(f, {
        local_get(7); i32_const(2); i32_eq;
        if_empty;
          local_get(8); i32_load(0); i32_const(5); i32_ne;
          if_empty; local_get(0); return_; end;
          local_get(8); i32_load(4); local_set(9);
          local_get(9); i32_load(0); local_set(10);
          local_get(6); i32_load(8); local_set(17);
          local_get(17); i32_const(0); i32_lt_s;
          local_get(17); local_get(10); i32_ge_s;
          i32_or;
          if_empty; local_get(0); return_; end;
          // Alloc new list (len - 1)
          local_get(10); i32_const(1); i32_sub; local_set(13);
          i32_const(4); local_get(13); i32_const(4); i32_mul; i32_add;
          call(alloc); local_set(15);
          local_get(15); local_get(13); i32_store(0);
          i32_const(0); local_set(11); // src
          i32_const(0); local_set(18); // dst
          block_empty; loop_empty;
            local_get(11); local_get(10); i32_ge_u; br_if(1);
            local_get(11); local_get(17); i32_ne;
            if_empty;
              local_get(15); i32_const(4); i32_add;
              local_get(18); i32_const(4); i32_mul; i32_add;
              local_get(9); i32_const(4); i32_add;
              local_get(11); i32_const(4); i32_mul; i32_add;
              i32_load(0); i32_store(0);
              local_get(18); i32_const(1); i32_add; local_set(18);
            end;
            local_get(11); i32_const(1); i32_add; local_set(11);
            br(0);
          end; end;
          i32_const(8); call(alloc); local_set(16);
          local_get(16); i32_const(5); i32_store(0);
          local_get(16); local_get(15); i32_store(4);
        end;
    });

    // --- Phase 5: Rebuild upward from seg_count-2 to 0 ---
    // cur_built is in local 16
    wasm!(f, {
        local_get(2); i32_const(2); i32_sub; local_set(5); // depth = seg_count - 2
        block_empty; loop_empty;
          local_get(5); i32_const(0); i32_lt_s; br_if(1);
          local_get(4); local_get(5); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(6);
          local_get(6); i32_load(0); local_set(7);
          local_get(14); local_get(5); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(8); // orig val
    });

    // Rebuild field
    wasm!(f, {
          local_get(7); i32_const(1); i32_eq;
          if_empty;
            local_get(8); i32_load(4); local_set(9);
            local_get(9); i32_load(0); local_set(10);
            i32_const(4); local_get(10); i32_const(4); i32_mul; i32_add;
            call(alloc); local_set(15);
            local_get(15); local_get(10); i32_store(0);
            i32_const(0); local_set(11);
            block_empty; loop_empty;
              local_get(11); local_get(10); i32_ge_u; br_if(1);
              local_get(9); i32_const(4); i32_add;
              local_get(11); i32_const(4); i32_mul; i32_add;
              i32_load(0); local_set(12);
              local_get(12); i32_load(0);
              local_get(6); i32_load(8);
              call(str_eq);
              if_empty;
                i32_const(8); call(alloc); local_set(13);
                local_get(13); local_get(12); i32_load(0); i32_store(0);
                local_get(13); local_get(16); i32_store(4);
                local_get(15); i32_const(4); i32_add;
                local_get(11); i32_const(4); i32_mul; i32_add;
                local_get(13); i32_store(0);
              else_;
                local_get(15); i32_const(4); i32_add;
                local_get(11); i32_const(4); i32_mul; i32_add;
                local_get(12); i32_store(0);
              end;
              local_get(11); i32_const(1); i32_add; local_set(11);
              br(0);
            end; end;
            i32_const(8); call(alloc); local_set(16);
            local_get(16); i32_const(6); i32_store(0);
            local_get(16); local_get(15); i32_store(4);
          end;
    });

    // Rebuild index
    wasm!(f, {
          local_get(7); i32_const(2); i32_eq;
          if_empty;
            local_get(8); i32_load(4); local_set(9);
            local_get(9); i32_load(0); local_set(10);
            local_get(6); i32_load(8); local_set(17);
            i32_const(4); local_get(10); i32_const(4); i32_mul; i32_add;
            call(alloc); local_set(15);
            local_get(15); local_get(10); i32_store(0);
            i32_const(0); local_set(11);
            block_empty; loop_empty;
              local_get(11); local_get(10); i32_ge_u; br_if(1);
              local_get(15); i32_const(4); i32_add;
              local_get(11); i32_const(4); i32_mul; i32_add;
              local_get(11); local_get(17); i32_eq;
              if_i32; local_get(16);
              else_;
                local_get(9); i32_const(4); i32_add;
                local_get(11); i32_const(4); i32_mul; i32_add;
                i32_load(0);
              end;
              i32_store(0);
              local_get(11); i32_const(1); i32_add; local_set(11);
              br(0);
            end; end;
            i32_const(8); call(alloc); local_set(16);
            local_get(16); i32_const(5); i32_store(0);
            local_get(16); local_get(15); i32_store(4);
          end;
    });

    wasm!(f, {
          local_get(5); i32_const(1); i32_sub; local_set(5);
          br(0);
        end; end;
    });

    // Return cur_built
    wasm!(f, {
        local_get(16);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}
