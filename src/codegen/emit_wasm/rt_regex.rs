//! WASM runtime: regex matching engine.
//!
//! Implements a backtracking regex interpreter via WASM functions.
//!
//! Runtime functions:
//!   __regex_match_atom(pat, text, pat_pos, text_pos) -> i32
//!     Matches a single atom at text_pos. Returns new text_pos or -1.
//!
//!   __regex_atom_len(pat, pat_pos) -> i32
//!     Returns the length in pattern bytes of the atom starting at pat_pos.
//!     For '[...]' returns bytes through ']', for '\x' returns 2, else 1.
//!
//!   __regex_match_anchored(pat, text, pat_pos, text_pos) -> i32
//!     Core recursive-descent matcher. Returns text end position or -1.
//!
//!   __regex_match_search(pat, text, start) -> i32
//!     Search for first match. Sets match_start_global. Returns end pos or -1.
//!
//!   __regex_captures_inner(pat, text, pat_pos, text_pos, cap_buf, cap_count) -> i32
//!     Like match_anchored but records capture groups.
//!
//!   __regex_captures_search(pat, text, start) -> i32
//!     Search with captures. Returns Option[List[String]].
//!
//! String layout: [len:i32][bytes:u8...]

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::{Function, ValType};

/// Regex-related runtime function indices.
#[derive(Default, Clone)]
pub struct RegexRuntime {
    pub match_anchored: u32,
    pub match_search: u32,
    pub match_start_global: u32,
    pub captures_inner: u32,
    pub captures_search: u32,
    pub skip_class: u32,
    pub match_class: u32,
    pub match_atom: u32,
    pub atom_len: u32,
    pub skip_group: u32,
}

pub(super) fn register(emitter: &mut WasmEmitter) {
    let ty4 = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    let ty3 = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    let ty2 = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );

    emitter.rt.regex.skip_class = emitter.register_func("__regex_skip_class", ty2);
    emitter.rt.regex.match_class = emitter.register_func("__regex_match_class", ty3);
    emitter.rt.regex.match_atom = emitter.register_func("__regex_match_atom", ty4);
    emitter.rt.regex.atom_len = emitter.register_func("__regex_atom_len", ty2);
    emitter.rt.regex.skip_group = emitter.register_func("__regex_skip_group", ty2);
    emitter.rt.regex.match_anchored = emitter.register_func("__regex_match_anchored", ty4);
    emitter.rt.regex.match_search = emitter.register_func("__regex_match_search", ty3);

    let ty6 = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.regex.captures_inner = emitter.register_func("__regex_captures_inner", ty6);
    emitter.rt.regex.captures_search = emitter.register_func("__regex_captures_search", ty3);

    // Global for match start position
    emitter.rt.regex.match_start_global = emitter.next_global;
    emitter.next_global += 1;
    emitter.top_let_init.push((emitter.rt.regex.match_start_global, ValType::I32, 0));
}

pub(super) fn compile(emitter: &mut WasmEmitter) {
    compile_skip_class(emitter);
    compile_match_class(emitter);
    compile_match_atom(emitter);
    compile_atom_len(emitter);
    compile_skip_group(emitter);
    compile_match_anchored(emitter);
    compile_match_search(emitter);
    compile_captures_inner(emitter);
    compile_captures_search(emitter);
}

// ─── skip_class ───
fn compile_skip_class(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.skip_class];
    let mut f = Function::new([(3, ValType::I32)]);
    // params: 0=pat, 1=pat_pos (at '[')
    // locals: 2=pat_len, 3=pos, 4=ch
    wasm!(f, { local_get(0); i32_load(0); local_set(2); });
    wasm!(f, { local_get(1); i32_const(1); i32_add; local_set(3); }); // skip '['
    // Skip '^' if present
    wasm!(f, {
        local_get(3); local_get(2); i32_lt_u;
        if_empty;
            local_get(0); i32_const(4); i32_add; local_get(3); i32_add; i32_load8_u(0);
            i32_const(94); i32_eq;
            if_empty; local_get(3); i32_const(1); i32_add; local_set(3); end;
        end;
    });
    // Skip ']' if first char
    wasm!(f, {
        local_get(3); local_get(2); i32_lt_u;
        if_empty;
            local_get(0); i32_const(4); i32_add; local_get(3); i32_add; i32_load8_u(0);
            i32_const(93); i32_eq;
            if_empty; local_get(3); i32_const(1); i32_add; local_set(3); end;
        end;
    });
    // Loop until ']'
    wasm!(f, {
        block_empty; loop_empty;
            local_get(3); local_get(2); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(3); i32_add; i32_load8_u(0);
            i32_const(93); i32_eq;
            if_empty; local_get(3); i32_const(1); i32_add; return_; end;
            local_get(3); i32_const(1); i32_add; local_set(3);
            br(0);
        end; end;
    });
    wasm!(f, { local_get(2); end; });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ─── match_class(pat, pat_pos_at_bracket, byte) -> i32 ───
fn compile_match_class(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_class];
    // params: 0=pat, 1=pat_pos (at '['), 2=byte
    // locals: 3=pos, 4=negated, 5=pat_len, 6=ch, 7=matched, 8=range_end
    let mut f = Function::new([(6, ValType::I32)]);

    wasm!(f, { local_get(0); i32_load(0); local_set(5); });
    wasm!(f, { local_get(1); i32_const(1); i32_add; local_set(3); }); // pos = after '['
    wasm!(f, { i32_const(0); local_set(4); i32_const(0); local_set(7); });

    // Check negation
    wasm!(f, {
        local_get(3); local_get(5); i32_lt_u;
        if_empty;
            local_get(0); i32_const(4); i32_add; local_get(3); i32_add; i32_load8_u(0);
            i32_const(94); i32_eq;
            if_empty; i32_const(1); local_set(4); local_get(3); i32_const(1); i32_add; local_set(3); end;
        end;
    });

    // Loop over class chars
    wasm!(f, { block_empty; loop_empty; });
    wasm!(f, { local_get(3); local_get(5); i32_ge_u; br_if(1); });
    wasm!(f, { local_get(0); i32_const(4); i32_add; local_get(3); i32_add; i32_load8_u(0); local_set(6); });
    wasm!(f, { local_get(6); i32_const(93); i32_eq; br_if(1); }); // ']' ends class

    // Check range: pos+2 < pat_len && pat[pos+1] == '-' && pat[pos+2] != ']'
    wasm!(f, {
        local_get(3); i32_const(2); i32_add; local_get(5); i32_lt_u;
        if_empty;
            local_get(0); i32_const(4); i32_add; local_get(3); i32_const(1); i32_add; i32_add; i32_load8_u(0);
            i32_const(45); i32_eq;
            if_empty;
                local_get(0); i32_const(4); i32_add; local_get(3); i32_const(2); i32_add; i32_add; i32_load8_u(0);
                local_set(8);
                local_get(8); i32_const(93); i32_ne;
                if_empty;
                    local_get(2); local_get(6); i32_ge_u;
                    local_get(2); local_get(8); i32_le_u;
                    i32_and;
                    if_empty; i32_const(1); local_set(7); end;
                    local_get(3); i32_const(3); i32_add; local_set(3);
                    br(3);
                end;
            end;
        end;
    });

    // Single char match
    wasm!(f, {
        local_get(2); local_get(6); i32_eq;
        if_empty; i32_const(1); local_set(7); end;
        local_get(3); i32_const(1); i32_add; local_set(3);
        br(0);
    });
    wasm!(f, { end; end; }); // end loop, end block

    // Return: negated XOR matched
    wasm!(f, {
        local_get(4);
        if_i32; local_get(7); i32_eqz;
        else_; local_get(7);
        end;
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ─── match_atom(pat, text, pat_pos, text_pos) -> i32 ───
// Returns new text_pos after matching one atom, or -1 if no match.
// Does NOT handle quantifiers; only matches exactly one instance.
fn compile_match_atom(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_atom];
    // params: 0=pat, 1=text, 2=pat_pos, 3=text_pos
    // locals: 4=pat_len, 5=text_len, 6=ch, 7=text_ch, 8=esc_ch, 9=matched
    let mut f = Function::new([(6, ValType::I32)]);

    let match_class_fn = emitter.rt.regex.match_class;

    wasm!(f, { local_get(0); i32_load(0); local_set(4); });
    wasm!(f, { local_get(1); i32_load(0); local_set(5); });
    // Need text char
    wasm!(f, {
        local_get(3); local_get(5); i32_ge_u;
        if_empty; i32_const(-1); return_; end;
    });
    wasm!(f, { local_get(1); i32_const(4); i32_add; local_get(3); i32_add; i32_load8_u(0); local_set(7); });
    wasm!(f, { local_get(0); i32_const(4); i32_add; local_get(2); i32_add; i32_load8_u(0); local_set(6); });

    // Dot: match any
    wasm!(f, {
        local_get(6); i32_const(46); i32_eq;
        if_empty; local_get(3); i32_const(1); i32_add; return_; end;
    });

    // Escape sequence
    wasm!(f, {
        local_get(6); i32_const(92); i32_eq;
        if_empty;
            local_get(2); i32_const(1); i32_add; local_get(4); i32_ge_u;
            if_empty; i32_const(-1); return_; end;
            local_get(0); i32_const(4); i32_add; local_get(2); i32_const(1); i32_add; i32_add; i32_load8_u(0);
            local_set(8);
            i32_const(0); local_set(9);
    });
    // \d
    wasm!(f, {
            local_get(8); i32_const(100); i32_eq;
            if_empty;
                local_get(7); i32_const(48); i32_ge_u; local_get(7); i32_const(57); i32_le_u; i32_and; local_set(9);
            end;
    });
    // \w
    wasm!(f, {
            local_get(8); i32_const(119); i32_eq;
            if_empty;
                local_get(7); i32_const(48); i32_ge_u; local_get(7); i32_const(57); i32_le_u; i32_and;
                local_get(7); i32_const(65); i32_ge_u; local_get(7); i32_const(90); i32_le_u; i32_and; i32_or;
                local_get(7); i32_const(97); i32_ge_u; local_get(7); i32_const(122); i32_le_u; i32_and; i32_or;
                local_get(7); i32_const(95); i32_eq; i32_or;
                local_set(9);
            end;
    });
    // \s
    wasm!(f, {
            local_get(8); i32_const(115); i32_eq;
            if_empty;
                local_get(7); i32_const(32); i32_eq;
                local_get(7); i32_const(9); i32_eq; i32_or;
                local_get(7); i32_const(10); i32_eq; i32_or;
                local_get(7); i32_const(13); i32_eq; i32_or;
                local_set(9);
            end;
    });
    wasm!(f, {
            local_get(9);
            if_i32; local_get(3); i32_const(1); i32_add;
            else_; i32_const(-1);
            end;
            return_;
        end;
    });

    // Character class
    wasm!(f, {
        local_get(6); i32_const(91); i32_eq;
        if_empty;
            local_get(0); local_get(2); local_get(7);
            call(match_class_fn);
            if_i32; local_get(3); i32_const(1); i32_add;
            else_; i32_const(-1);
            end;
            return_;
        end;
    });

    // Literal char
    wasm!(f, {
        local_get(7); local_get(6); i32_eq;
        if_i32; local_get(3); i32_const(1); i32_add;
        else_; i32_const(-1);
        end;
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ─── atom_len(pat, pat_pos) -> i32 ───
// Returns number of pattern bytes consumed by the atom at pat_pos.
fn compile_atom_len(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.atom_len];
    // params: 0=pat, 1=pat_pos
    // locals: 2=ch, 3=pat_len
    let mut f = Function::new([(2, ValType::I32)]);

    let skip_class_fn = emitter.rt.regex.skip_class;

    wasm!(f, { local_get(0); i32_load(0); local_set(3); });
    wasm!(f, {
        local_get(1); local_get(3); i32_ge_u;
        if_empty; i32_const(0); return_; end;
    });
    wasm!(f, { local_get(0); i32_const(4); i32_add; local_get(1); i32_add; i32_load8_u(0); local_set(2); });

    // Escape: 2 bytes
    wasm!(f, {
        local_get(2); i32_const(92); i32_eq;
        if_empty; i32_const(2); return_; end;
    });
    // Class: skip to ']'
    wasm!(f, {
        local_get(2); i32_const(91); i32_eq;
        if_empty;
            local_get(0); local_get(1); call(skip_class_fn);
            local_get(1); i32_sub;
            return_;
        end;
    });
    // Everything else (literal, dot): 1 byte
    wasm!(f, { i32_const(1); end; });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ─── skip_group(pat, pat_pos) -> i32 ───
fn compile_skip_group(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.skip_group];
    let mut f = Function::new([(4, ValType::I32)]);
    // params: 0=pat, 1=pat_pos (at '(')
    // locals: 2=pos, 3=depth, 4=pat_len, 5=ch

    wasm!(f, { local_get(0); i32_load(0); local_set(4); });
    wasm!(f, { local_get(1); i32_const(1); i32_add; local_set(2); });
    wasm!(f, { i32_const(1); local_set(3); });

    wasm!(f, { block_empty; loop_empty; });
    wasm!(f, { local_get(2); local_get(4); i32_ge_u; br_if(1); });
    wasm!(f, { local_get(3); i32_eqz; br_if(1); });
    wasm!(f, { local_get(0); i32_const(4); i32_add; local_get(2); i32_add; i32_load8_u(0); local_set(5); });

    // Skip [...]
    wasm!(f, {
        local_get(5); i32_const(91); i32_eq;
        if_empty;
            local_get(0); local_get(2); call(emitter.rt.regex.skip_class);
            local_set(2); br(1);
        end;
    });
    // Skip escape
    wasm!(f, {
        local_get(5); i32_const(92); i32_eq;
        if_empty; local_get(2); i32_const(2); i32_add; local_set(2); br(1); end;
    });
    // ( and )
    wasm!(f, {
        local_get(5); i32_const(40); i32_eq;
        if_empty; local_get(3); i32_const(1); i32_add; local_set(3); end;
    });
    wasm!(f, {
        local_get(5); i32_const(41); i32_eq;
        if_empty; local_get(3); i32_const(1); i32_sub; local_set(3); end;
    });
    wasm!(f, { local_get(2); i32_const(1); i32_add; local_set(2); br(0); });
    wasm!(f, { end; end; }); // end loop, block

    wasm!(f, { local_get(2); end; });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ─── match_anchored(pat, text, pat_pos, text_pos) -> i32 ───
// Core backtracking matcher using match_atom and atom_len helpers.
fn compile_match_anchored(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_anchored];
    // params: 0=pat, 1=text, 2=pat_pos, 3=text_pos
    // locals: 4=pat_len, 5=text_len, 6=ch, 7=atom_size, 8=quant, 9=result, 10=try_pos, 11=next_pat
    let mut f = Function::new([(8, ValType::I32)]);

    let match_fn = emitter.rt.regex.match_anchored;
    let match_atom_fn = emitter.rt.regex.match_atom;
    let atom_len_fn = emitter.rt.regex.atom_len;

    wasm!(f, { local_get(0); i32_load(0); local_set(4); });
    wasm!(f, { local_get(1); i32_load(0); local_set(5); });

    // Main loop
    wasm!(f, { block_empty; loop_empty; });

    // End of pattern => success
    wasm!(f, {
        local_get(2); local_get(4); i32_ge_u;
        if_empty; local_get(3); return_; end;
    });

    // Load current pattern char
    wasm!(f, { local_get(0); i32_const(4); i32_add; local_get(2); i32_add; i32_load8_u(0); local_set(6); });

    // Handle '$' at end of pattern
    wasm!(f, {
        local_get(6); i32_const(36); i32_eq;
        local_get(2); i32_const(1); i32_add; local_get(4); i32_ge_u;
        i32_and;
        if_empty;
            local_get(3); local_get(5); i32_eq;
            if_i32; local_get(3); else_; i32_const(-1); end;
            return_;
        end;
    });

    // Handle '(' — just skip it (captures handled separately)
    wasm!(f, {
        local_get(6); i32_const(40); i32_eq;
        if_empty;
            local_get(0); local_get(1); local_get(2); i32_const(1); i32_add; local_get(3);
            call(match_fn); return_;
        end;
    });
    // Handle ')'
    wasm!(f, {
        local_get(6); i32_const(41); i32_eq;
        if_empty;
            local_get(0); local_get(1); local_get(2); i32_const(1); i32_add; local_get(3);
            call(match_fn); return_;
        end;
    });

    // Get atom length
    wasm!(f, {
        local_get(0); local_get(2); call(atom_len_fn); local_set(7);
    });

    // next_pat = pat_pos + atom_size
    wasm!(f, { local_get(2); local_get(7); i32_add; local_set(11); });

    // Check for quantifier
    wasm!(f, {
        local_get(11); local_get(4); i32_lt_u;
        if_empty;
            local_get(0); i32_const(4); i32_add; local_get(11); i32_add; i32_load8_u(0);
            local_set(8);
        else_;
            i32_const(0); local_set(8);
        end;
    });

    // === '+' quantifier (greedy one-or-more) ===
    wasm!(f, {
        local_get(8); i32_const(43); i32_eq;
        if_empty;
            // Must match at least once
            local_get(0); local_get(1); local_get(2); local_get(3);
            call(match_atom_fn);
            local_set(9);
            local_get(9); i32_const(-1); i32_eq;
            if_empty; i32_const(-1); return_; end;
    });
    // Greedily consume
    wasm!(f, {
            local_get(9); local_set(10); // try_pos = first match end
            block_empty; loop_empty;
                local_get(10); local_get(5); i32_ge_u; br_if(1);
                local_get(0); local_get(1); local_get(2); local_get(10);
                call(match_atom_fn);
                local_set(9);
                local_get(9); i32_const(-1); i32_eq; br_if(1);
                local_get(9); local_set(10);
                br(0);
            end; end;
    });
    // Backtrack from try_pos down to first_match+1
    wasm!(f, {
            block_empty; loop_empty;
                local_get(10); local_get(3); i32_lt_u; br_if(1);
                local_get(0); local_get(1);
                local_get(11); i32_const(1); i32_add; // pat after quantifier
                local_get(10);
                call(match_fn);
                local_set(9);
                local_get(9); i32_const(-1); i32_ne;
                if_empty; local_get(9); return_; end;
                local_get(10); i32_const(1); i32_sub; local_set(10);
                br(0);
            end; end;
            i32_const(-1); return_;
        end;
    });

    // === '*' quantifier (greedy zero-or-more) ===
    wasm!(f, {
        local_get(8); i32_const(42); i32_eq;
        if_empty;
            local_get(3); local_set(10);
            block_empty; loop_empty;
                local_get(10); local_get(5); i32_ge_u; br_if(1);
                local_get(0); local_get(1); local_get(2); local_get(10);
                call(match_atom_fn);
                local_set(9);
                local_get(9); i32_const(-1); i32_eq; br_if(1);
                local_get(9); local_set(10);
                br(0);
            end; end;
    });
    wasm!(f, {
            block_empty; loop_empty;
                local_get(10); local_get(3); i32_lt_s; br_if(1);
                local_get(0); local_get(1);
                local_get(11); i32_const(1); i32_add;
                local_get(10);
                call(match_fn);
                local_set(9);
                local_get(9); i32_const(-1); i32_ne;
                if_empty; local_get(9); return_; end;
                local_get(10); i32_const(1); i32_sub; local_set(10);
                br(0);
            end; end;
            i32_const(-1); return_;
        end;
    });

    // === '?' quantifier (greedy optional) ===
    wasm!(f, {
        local_get(8); i32_const(63); i32_eq;
        if_empty;
            // Try with match
            local_get(0); local_get(1); local_get(2); local_get(3);
            call(match_atom_fn);
            local_set(9);
            local_get(9); i32_const(-1); i32_ne;
            if_empty;
                local_get(0); local_get(1);
                local_get(11); i32_const(1); i32_add;
                local_get(9);
                call(match_fn);
                local_set(9);
                local_get(9); i32_const(-1); i32_ne;
                if_empty; local_get(9); return_; end;
            end;
            // Try without match
            local_get(0); local_get(1);
            local_get(11); i32_const(1); i32_add;
            local_get(3);
            call(match_fn);
            return_;
        end;
    });

    // === No quantifier: match single atom ===
    wasm!(f, {
        local_get(0); local_get(1); local_get(2); local_get(3);
        call(match_atom_fn);
        local_set(9);
        local_get(9); i32_const(-1); i32_eq;
        if_empty; i32_const(-1); return_; end;
        local_get(11); local_set(2);
        local_get(9); local_set(3);
        br(0);
    });

    wasm!(f, { end; end; }); // end loop, block
    wasm!(f, { local_get(3); end; }); // success

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ─── match_search(pat, text, start) -> i32 ───
fn compile_match_search(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_search];
    // params: 0=pat, 1=text, 2=start
    // locals: 3=text_len, 4=pos, 5=result, 6=pat_len, 7=anchored
    let mut f = Function::new([(5, ValType::I32)]);

    let match_fn = emitter.rt.regex.match_anchored;
    let match_start_global = emitter.rt.regex.match_start_global;

    wasm!(f, { local_get(1); i32_load(0); local_set(3); });
    wasm!(f, { local_get(0); i32_load(0); local_set(6); });

    // Check for '^' anchor
    wasm!(f, {
        i32_const(0); local_set(7);
        local_get(6); i32_const(0); i32_gt_u;
        if_empty;
            local_get(0); i32_const(4); i32_add; i32_load8_u(0);
            i32_const(94); i32_eq;
            if_empty; i32_const(1); local_set(7); end;
        end;
    });

    wasm!(f, {
        local_get(7);
        if_i32;
            // Anchored: only try pos 0, pat_pos=1
            local_get(0); local_get(1); i32_const(1); i32_const(0);
            call(match_fn);
            local_set(5);
            local_get(5); i32_const(-1); i32_ne;
            if_i32;
                i32_const(0); global_set(match_start_global);
                local_get(5);
            else_;
                i32_const(-1);
            end;
        else_;
    });
    // Search loop
    wasm!(f, {
            local_get(2); local_set(4);
            block_empty; loop_empty;
                local_get(4); local_get(3); i32_gt_u; br_if(1);
                local_get(0); local_get(1); i32_const(0); local_get(4);
                call(match_fn);
                local_set(5);
                local_get(5); i32_const(-1); i32_ne;
                if_empty;
                    local_get(4); global_set(match_start_global);
                    local_get(5); return_;
                end;
                local_get(4); i32_const(1); i32_add; local_set(4);
                br(0);
            end; end;
            i32_const(-1);
        end;
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ─── captures_inner(pat, text, pat_pos, text_pos, cap_buf, cap_count) -> i32 ───
fn compile_captures_inner(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.captures_inner];
    // params: 0=pat, 1=text, 2=pat_pos, 3=text_pos, 4=cap_buf, 5=cap_count
    // locals: 6=pat_len, 7=text_len, 8=ch, 9=atom_size, 10=quant, 11=result,
    //         12=try_pos, 13=next_pat, 14=tmp
    let mut f = Function::new([(9, ValType::I32)]);

    let captures_fn = emitter.rt.regex.captures_inner;
    let match_atom_fn = emitter.rt.regex.match_atom;
    let atom_len_fn = emitter.rt.regex.atom_len;
    let _skip_group_fn = emitter.rt.regex.skip_group;

    wasm!(f, { local_get(0); i32_load(0); local_set(6); });
    wasm!(f, { local_get(1); i32_load(0); local_set(7); });

    // Main loop
    wasm!(f, { block_empty; loop_empty; });

    // End of pattern => success
    wasm!(f, {
        local_get(2); local_get(6); i32_ge_u;
        if_empty; local_get(3); return_; end;
    });

    wasm!(f, { local_get(0); i32_const(4); i32_add; local_get(2); i32_add; i32_load8_u(0); local_set(8); });

    // Handle '$'
    wasm!(f, {
        local_get(8); i32_const(36); i32_eq;
        local_get(2); i32_const(1); i32_add; local_get(6); i32_ge_u;
        i32_and;
        if_empty;
            local_get(3); local_get(7); i32_eq;
            if_i32; local_get(3); else_; i32_const(-1); end;
            return_;
        end;
    });

    // Handle '(' — capture group start
    wasm!(f, {
        local_get(8); i32_const(40); i32_eq;
        if_empty;
            // Record start: cap_buf[cap_count*8] = text_pos
            local_get(4); local_get(5); i32_const(8); i32_mul; i32_add;
            local_get(3); i32_store(0);
            // Recurse with pat_pos+1, cap_count+1
            local_get(0); local_get(1);
            local_get(2); i32_const(1); i32_add;
            local_get(3); local_get(4);
            local_get(5); i32_const(1); i32_add;
            call(captures_fn);
            return_;
        end;
    });

    // Handle ')' — capture group end
    wasm!(f, {
        local_get(8); i32_const(41); i32_eq;
        if_empty;
            // Record end: cap_buf[(cap_count-1)*8+4] = text_pos
            local_get(4); local_get(5); i32_const(1); i32_sub; i32_const(8); i32_mul; i32_add;
            local_get(3); i32_store(4);
            // Continue with pat_pos+1, cap_count-1
            local_get(0); local_get(1);
            local_get(2); i32_const(1); i32_add;
            local_get(3); local_get(4);
            local_get(5); i32_const(1); i32_sub;
            call(captures_fn);
            return_;
        end;
    });

    // Get atom length and check quantifier
    wasm!(f, { local_get(0); local_get(2); call(atom_len_fn); local_set(9); });
    wasm!(f, { local_get(2); local_get(9); i32_add; local_set(13); });
    wasm!(f, {
        local_get(13); local_get(6); i32_lt_u;
        if_empty;
            local_get(0); i32_const(4); i32_add; local_get(13); i32_add; i32_load8_u(0);
            local_set(10);
        else_;
            i32_const(0); local_set(10);
        end;
    });

    // '+' quantifier
    wasm!(f, {
        local_get(10); i32_const(43); i32_eq;
        if_empty;
            local_get(0); local_get(1); local_get(2); local_get(3);
            call(match_atom_fn);
            local_set(11);
            local_get(11); i32_const(-1); i32_eq;
            if_empty; i32_const(-1); return_; end;
            local_get(11); local_set(12);
    });
    wasm!(f, {
            block_empty; loop_empty;
                local_get(12); local_get(7); i32_ge_u; br_if(1);
                local_get(0); local_get(1); local_get(2); local_get(12);
                call(match_atom_fn);
                local_set(11);
                local_get(11); i32_const(-1); i32_eq; br_if(1);
                local_get(11); local_set(12);
                br(0);
            end; end;
    });
    wasm!(f, {
            block_empty; loop_empty;
                local_get(12); local_get(3); i32_lt_u; br_if(1);
                local_get(0); local_get(1);
                local_get(13); i32_const(1); i32_add;
                local_get(12); local_get(4); local_get(5);
                call(captures_fn);
                local_set(11);
                local_get(11); i32_const(-1); i32_ne;
                if_empty; local_get(11); return_; end;
                local_get(12); i32_const(1); i32_sub; local_set(12);
                br(0);
            end; end;
            i32_const(-1); return_;
        end;
    });

    // No quantifier: single atom match
    wasm!(f, {
        local_get(0); local_get(1); local_get(2); local_get(3);
        call(match_atom_fn);
        local_set(11);
        local_get(11); i32_const(-1); i32_eq;
        if_empty; i32_const(-1); return_; end;
        local_get(13); local_set(2);
        local_get(11); local_set(3);
        br(0);
    });

    wasm!(f, { end; end; }); // end loop, block
    wasm!(f, { local_get(3); end; });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ─── captures_search(pat, text, start) -> i32 ───
fn compile_captures_search(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.captures_search];
    // params: 0=pat, 1=text, 2=start
    // locals: 3=text_len, 4=pos, 5=result, 6=cap_buf, 7=num_groups,
    //         8=end_pos, 9=list_ptr, 10=i, 11=grp_start, 12=grp_end,
    //         13=str_ptr, 14=pat_len, 15=opt_ptr, 16=ch
    let mut f = Function::new([(14, ValType::I32)]);

    let captures_fn = emitter.rt.regex.captures_inner;
    let alloc = emitter.rt.alloc;
    let _slice_fn = emitter.rt.string.slice;

    wasm!(f, { local_get(1); i32_load(0); local_set(3); });
    wasm!(f, { local_get(0); i32_load(0); local_set(14); });

    // Count '(' in pattern
    wasm!(f, { i32_const(0); local_set(7); i32_const(0); local_set(10); });
    wasm!(f, { block_empty; loop_empty; });
    wasm!(f, { local_get(10); local_get(14); i32_ge_u; br_if(1); });
    wasm!(f, { local_get(0); i32_const(4); i32_add; local_get(10); i32_add; i32_load8_u(0); local_set(16); });
    // Skip escaped chars
    wasm!(f, {
        local_get(16); i32_const(92); i32_eq;
        if_empty; local_get(10); i32_const(2); i32_add; local_set(10); br(1); end;
    });
    wasm!(f, {
        local_get(16); i32_const(40); i32_eq;
        if_empty; local_get(7); i32_const(1); i32_add; local_set(7); end;
    });
    wasm!(f, { local_get(10); i32_const(1); i32_add; local_set(10); br(0); });
    wasm!(f, { end; end; }); // end loop, block

    // Allocate cap_buf
    wasm!(f, {
        local_get(7); i32_const(8); i32_mul; i32_const(8); i32_add;
        call(alloc); local_set(6);
    });

    // Handle '^' anchor
    wasm!(f, {
        local_get(14); i32_const(0); i32_gt_u;
        if_empty;
            local_get(0); i32_const(4); i32_add; i32_load8_u(0);
            i32_const(94); i32_eq;
            if_empty;
    });
    // Init cap_buf for anchored
    emit_init_cap_buf(&mut f, 6, 7, 10);
    wasm!(f, {
                local_get(0); local_get(1); i32_const(1); i32_const(0);
                local_get(6); i32_const(0);
                call(captures_fn);
                local_set(8);
                local_get(8); i32_const(-1); i32_ne;
                if_empty;
    });
    emit_build_captures_list(&mut f, emitter, 6, 7, 9, 10, 11, 12, 13, 15);
    wasm!(f, {
                    return_;
                end;
                i32_const(0); return_;
            end;
        end;
    });

    // Search loop
    wasm!(f, { local_get(2); local_set(4); });
    wasm!(f, { block_empty; loop_empty; });
    wasm!(f, { local_get(4); local_get(3); i32_gt_u; br_if(1); });

    // Init cap_buf
    emit_init_cap_buf(&mut f, 6, 7, 10);

    wasm!(f, {
        local_get(0); local_get(1); i32_const(0); local_get(4);
        local_get(6); i32_const(0);
        call(captures_fn);
        local_set(8);
        local_get(8); i32_const(-1); i32_ne;
        if_empty;
    });

    emit_build_captures_list(&mut f, emitter, 6, 7, 9, 10, 11, 12, 13, 15);

    wasm!(f, {
            return_;
        end;
        local_get(4); i32_const(1); i32_add; local_set(4);
        br(0);
    });
    wasm!(f, { end; end; }); // end loop, block

    wasm!(f, { i32_const(0); end; }); // no match

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// Helper: initialize cap_buf entries to -1
fn emit_init_cap_buf(f: &mut Function, cap_buf: u32, num_groups: u32, i: u32) {
    wasm!(f, { i32_const(0); local_set(i); });
    wasm!(f, { block_empty; loop_empty; });
    wasm!(f, { local_get(i); local_get(num_groups); i32_ge_u; br_if(1); });
    wasm!(f, {
        local_get(cap_buf); local_get(i); i32_const(8); i32_mul; i32_add;
        i32_const(-1); i32_store(0);
    });
    wasm!(f, {
        local_get(cap_buf); local_get(i); i32_const(8); i32_mul; i32_add;
        i32_const(-1); i32_store(4);
    });
    wasm!(f, { local_get(i); i32_const(1); i32_add; local_set(i); br(0); });
    wasm!(f, { end; end; });
}

/// Helper: build captures list from cap_buf and return it (wraps in Option some)
fn emit_build_captures_list(
    f: &mut Function, emitter: &mut WasmEmitter,
    cap_buf: u32, num_groups: u32, list_ptr: u32, i: u32,
    grp_start: u32, grp_end: u32, str_ptr: u32, opt_ptr: u32,
) {
    let alloc = emitter.rt.alloc;
    let slice_fn = emitter.rt.string.slice;

    // Allocate list
    wasm!(f, {
        local_get(num_groups); i32_const(4); i32_mul; i32_const(4); i32_add;
        call(alloc); local_set(list_ptr);
    });
    wasm!(f, { local_get(list_ptr); local_get(num_groups); i32_store(0); });

    // Fill list
    wasm!(f, { i32_const(0); local_set(i); });
    wasm!(f, { block_empty; loop_empty; });
    wasm!(f, { local_get(i); local_get(num_groups); i32_ge_u; br_if(1); });

    wasm!(f, {
        local_get(cap_buf); local_get(i); i32_const(8); i32_mul; i32_add; i32_load(0);
        local_set(grp_start);
    });
    wasm!(f, {
        local_get(cap_buf); local_get(i); i32_const(8); i32_mul; i32_add; i32_load(4);
        local_set(grp_end);
    });

    // If grp_start == -1, empty string
    wasm!(f, {
        local_get(grp_start); i32_const(-1); i32_eq;
        if_i32;
            i32_const(4); call(alloc); local_set(str_ptr);
            local_get(str_ptr); i32_const(0); i32_store(0);
            local_get(str_ptr);
        else_;
    });
    // param 1 = text (still at local 1)
    wasm!(f, {
            local_get(1); local_get(grp_start); local_get(grp_end);
            call(slice_fn);
        end;
    });

    wasm!(f, {
        local_set(str_ptr);
        local_get(list_ptr); i32_const(4); i32_add;
        local_get(i); i32_const(4); i32_mul; i32_add;
        local_get(str_ptr); i32_store(0);
    });
    wasm!(f, { local_get(i); i32_const(1); i32_add; local_set(i); br(0); });
    wasm!(f, { end; end; }); // end loop, block

    // Wrap in Option some
    wasm!(f, {
        i32_const(4); call(alloc); local_set(opt_ptr);
        local_get(opt_ptr); local_get(list_ptr); i32_store(0);
        local_get(opt_ptr);
    });
}
