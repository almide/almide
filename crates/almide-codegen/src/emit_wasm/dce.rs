//! Dead Code Elimination for WASM codegen.
//!
//! After all functions are compiled, scans call instructions to build a call graph,
//! computes the reachable set from entry points, and replaces unreachable function
//! bodies with minimal stubs. This preserves function indices (no remapping needed)
//! while dramatically reducing binary size.

use std::collections::{HashSet, VecDeque};
use wasm_encoder::Function;

use super::WasmEmitter;

/// Perform dead code elimination on compiled functions.
/// Replaces unreachable function bodies with `unreachable` stubs.
/// Returns the number of functions eliminated.
pub fn eliminate_dead_code(emitter: &mut WasmEmitter) -> usize {
    let num_imports = emitter.num_imports;
    let num_compiled = emitter.compiled.len();
    if num_compiled == 0 {
        return 0;
    }

    // Step 1: Find entry points
    let mut entry_points: HashSet<u32> = HashSet::new();

    // _start / main / __test_runner
    for (name, &idx) in &emitter.func_map {
        if name == "main" || name == "__test_runner" || name == "__init_globals" {
            entry_points.insert(idx);
        }
    }

    // Also include __alloc as always-needed (called by many stubs indirectly)
    entry_points.insert(emitter.rt.alloc);
    // __heap_save / __heap_restore are JS-callable arena-cleanup helpers;
    // they have no callers inside the wasm but the JS side relies on them.
    entry_points.insert(emitter.rt.heap_save);
    entry_points.insert(emitter.rt.heap_restore);
    // __init_preopen_dirs and __resolve_path are called from main at startup
    entry_points.insert(emitter.rt.init_preopen_dirs);
    entry_points.insert(emitter.rt.resolve_path);

    // Functions in the element table (called via call_indirect)
    for &func_idx in &emitter.func_table {
        entry_points.insert(func_idx);
    }

    // Step 2: Build call graph by scanning compiled function bytes
    // call_graph[i] = set of func_idx called by compiled function i
    let mut call_graph: Vec<Vec<u32>> = Vec::with_capacity(num_compiled);

    for cf in &emitter.compiled {
        let calls = extract_call_targets(&cf.func);
        call_graph.push(calls);
    }

    // Step 3: BFS from entry points to find reachable set
    let mut reachable: HashSet<u32> = HashSet::new();
    let mut queue: VecDeque<u32> = VecDeque::new();

    // All imports are always reachable (they're external)
    for i in 0..num_imports {
        reachable.insert(i);
    }

    for &entry in &entry_points {
        if reachable.insert(entry) {
            queue.push_back(entry);
        }
    }

    while let Some(func_idx) = queue.pop_front() {
        // Only compiled (non-import) functions have call graphs
        if func_idx >= num_imports {
            let compiled_idx = (func_idx - num_imports) as usize;
            if compiled_idx < call_graph.len() {
                for &callee in &call_graph[compiled_idx] {
                    if reachable.insert(callee) {
                        queue.push_back(callee);
                    }
                }
            }
        }
    }

    // Step 4: Replace unreachable function bodies with stubs
    let mut eliminated = 0;
    for (i, cf) in emitter.compiled.iter_mut().enumerate() {
        let func_idx = num_imports + i as u32;
        if !reachable.contains(&func_idx) {
            // Create minimal stub: no locals, just `unreachable` + `end`
            let mut stub = Function::new([]);
            stub.instruction(&wasm_encoder::Instruction::Unreachable);
            stub.instruction(&wasm_encoder::Instruction::End);
            cf.func = stub;
            eliminated += 1;
        }
    }

    eliminated
}

/// Extract all `call` instruction targets from a compiled Function.
/// Scans the raw bytecode for call opcode (0x10) followed by LEB128 func_idx.
fn extract_call_targets(func: &Function) -> Vec<u32> {
    let mut targets = Vec::new();

    // Clone the function to get its raw bytes via into_raw_body
    let bytes = func.clone().into_raw_body();

    let mut pos = 0;
    // Skip locals declaration at the start
    // Format: count of local groups, then (count, type) pairs
    if pos < bytes.len() {
        let (num_local_groups, consumed) = read_leb128_u32(&bytes[pos..]);
        pos += consumed;
        for _ in 0..num_local_groups {
            if pos >= bytes.len() { break; }
            let (_count, consumed) = read_leb128_u32(&bytes[pos..]);
            pos += consumed;
            if pos < bytes.len() {
                pos += 1; // skip type byte
            }
        }
    }

    // Scan instructions
    while pos < bytes.len() {
        let opcode = bytes[pos];
        pos += 1;

        match opcode {
            0x10 => {
                // call: followed by LEB128 func_idx
                if pos < bytes.len() {
                    let (func_idx, consumed) = read_leb128_u32(&bytes[pos..]);
                    pos += consumed;
                    targets.push(func_idx);
                }
            }
            0x11 => {
                // call_indirect: type_idx (LEB128) + table_idx (LEB128)
                if pos < bytes.len() {
                    let (_, consumed) = read_leb128_u32(&bytes[pos..]);
                    pos += consumed;
                    if pos < bytes.len() {
                        let (_, consumed) = read_leb128_u32(&bytes[pos..]);
                        pos += consumed;
                    }
                }
            }
            // Block-type instructions with block type
            0x02 | 0x03 | 0x04 | 0x06 => {
                // block, loop, if, try: followed by block type
                if pos < bytes.len() {
                    pos += block_type_size(&bytes[pos..]);
                }
            }
            // Instructions with LEB128 immediate(s)
            0x0C | 0x0D => {
                // br, br_if: label_idx
                if pos < bytes.len() {
                    let (_, consumed) = read_leb128_u32(&bytes[pos..]);
                    pos += consumed;
                }
            }
            0x0E => {
                // br_table: vec(label_idx) + default
                if pos < bytes.len() {
                    let (count, consumed) = read_leb128_u32(&bytes[pos..]);
                    pos += consumed;
                    for _ in 0..=count {
                        if pos >= bytes.len() { break; }
                        let (_, consumed) = read_leb128_u32(&bytes[pos..]);
                        pos += consumed;
                    }
                }
            }
            // local.get/set/tee, global.get/set
            0x20 | 0x21 | 0x22 | 0x23 | 0x24 => {
                if pos < bytes.len() {
                    let (_, consumed) = read_leb128_u32(&bytes[pos..]);
                    pos += consumed;
                }
            }
            // Memory instructions: load/store with memarg (align + offset)
            0x28..=0x3E => {
                // memarg: align (LEB128) + offset (LEB128)
                if pos < bytes.len() {
                    let (_, consumed) = read_leb128_u32(&bytes[pos..]);
                    pos += consumed;
                    if pos < bytes.len() {
                        let (_, consumed) = read_leb128_u32(&bytes[pos..]);
                        pos += consumed;
                    }
                }
            }
            // memory.size, memory.grow
            0x3F | 0x40 => {
                if pos < bytes.len() {
                    let (_, consumed) = read_leb128_u32(&bytes[pos..]);
                    pos += consumed;
                }
            }
            // Constants
            0x41 => {
                // i32.const: signed LEB128
                if pos < bytes.len() {
                    let (_, consumed) = read_leb128_i32(&bytes[pos..]);
                    pos += consumed;
                }
            }
            0x42 => {
                // i64.const: signed LEB128
                if pos < bytes.len() {
                    let (_, consumed) = read_leb128_i64(&bytes[pos..]);
                    pos += consumed;
                }
            }
            0x43 => {
                // f32.const: 4 bytes
                pos += 4;
            }
            0x44 => {
                // f64.const: 8 bytes
                pos += 8;
            }
            // ref.null, ref.func, ref.is_null
            0xD0 => { pos += 1; } // heaptype
            0xD2 => {
                if pos < bytes.len() {
                    let (_, consumed) = read_leb128_u32(&bytes[pos..]);
                    pos += consumed;
                }
            }
            // All other single-byte instructions (no immediate)
            // 0x00 unreachable, 0x01 nop, 0x05 else, 0x0B end, 0x0F return,
            // 0x1A drop, 0x1B select, 0x45-0xC4 numeric ops, etc.
            _ => {}
        }
    }

    targets
}

/// Decode an unsigned LEB128 value, returning (value, bytes_consumed).
fn read_leb128_u32(bytes: &[u8]) -> (u32, usize) {
    let mut result: u32 = 0;
    let mut shift = 0;
    let mut pos = 0;
    loop {
        if pos >= bytes.len() { return (result, pos); }
        let byte = bytes[pos];
        pos += 1;
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 { break; }
        shift += 7;
        if shift >= 35 { break; }
    }
    (result, pos)
}

/// Decode a signed LEB128 i32 value, returning (value, bytes_consumed).
fn read_leb128_i32(bytes: &[u8]) -> (i32, usize) {
    let mut result: i32 = 0;
    let mut shift = 0;
    let mut pos = 0;
    loop {
        if pos >= bytes.len() { return (result, pos); }
        let byte = bytes[pos];
        pos += 1;
        result |= ((byte & 0x7F) as i32) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            if shift < 32 && (byte & 0x40) != 0 {
                result |= !0 << shift;
            }
            break;
        }
        if shift >= 35 { break; }
    }
    (result, pos)
}

/// Decode a signed LEB128 i64 value, returning (value, bytes_consumed).
fn read_leb128_i64(bytes: &[u8]) -> (i64, usize) {
    let mut result: i64 = 0;
    let mut shift = 0;
    let mut pos = 0;
    loop {
        if pos >= bytes.len() { return (result, pos); }
        let byte = bytes[pos];
        pos += 1;
        result |= ((byte & 0x7F) as i64) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            if shift < 64 && (byte & 0x40) != 0 {
                result |= !0i64 << shift;
            }
            break;
        }
        if shift >= 70 { break; }
    }
    (result, pos)
}

/// Determine the size of a block type encoding.
fn block_type_size(bytes: &[u8]) -> usize {
    if bytes.is_empty() { return 0; }
    let byte = bytes[0];
    if byte == 0x40 {
        // empty block type
        1
    } else if byte & 0x80 != 0 || (byte >= 0x60 && byte <= 0x7F) {
        // value type (single byte: i32=0x7F, i64=0x7E, f32=0x7D, f64=0x7C, etc.)
        1
    } else {
        // type index (LEB128 s33, but in practice a small positive number)
        let (_, consumed) = read_leb128_u32(bytes);
        consumed
    }
}
