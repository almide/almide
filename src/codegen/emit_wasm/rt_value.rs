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
}

/// Compile all runtime function bodies.
pub(super) fn compile(emitter: &mut WasmEmitter) {
    compile_value_stringify(emitter);
    compile_json_parse(emitter);
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

    // Tag 4: string -> "\"" + s + "\""
    wasm!(f, {
        local_get(1); i32_const(4); i32_eq;
        if_empty;
          i32_const(quote_str as i32);
          local_get(0); i32_load(4);
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
