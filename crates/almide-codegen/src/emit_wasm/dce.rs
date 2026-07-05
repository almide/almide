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

    // _start / main / __main_runner / __test_runner
    for (name, &idx) in &emitter.func_map {
        if name == "main" || name == "__main_runner" || name == "__test_runner" || name == "__init_globals" {
            entry_points.insert(idx);
        }
    }

    // Exported user `pub fn`s are roots: the host (JS/WASI caller) invokes them
    // directly, so they are reachable even when nothing inside the module — `main`
    // included — calls them. Without this their bodies are stubbed to `unreachable`
    // and the export traps on the first host call (#457). `user_exports` is
    // populated before this pass (see emit_wasm/mod.rs).
    for (_export_name, internal_name) in &emitter.user_exports {
        if let Some(&idx) = emitter.func_map.get(internal_name) {
            entry_points.insert(idx);
        }
    }

    // Also include __alloc as always-needed (called by many stubs indirectly)
    entry_points.insert(emitter.rt.alloc);
    // __heap_save / __heap_restore are JS-callable arena-cleanup helpers;
    // they have no callers inside the wasm but the JS side relies on them.
    entry_points.insert(emitter.rt.heap_save);
    entry_points.insert(emitter.rt.heap_restore);
    // __init_preopen_dirs and __resolve_path only needed if program uses filesystem
    if emitter.needs_fs {
        entry_points.insert(emitter.rt.init_preopen_dirs);
        entry_points.insert(emitter.rt.resolve_path);
    }

    // Functions in the element table (called via call_indirect)
    for &func_idx in &emitter.func_table {
        entry_points.insert(func_idx);
    }

    // Step 2: Build call graph by scanning compiled function bytes
    // call_graph[i] = set of func_idx called by compiled function i
    let mut call_graph: Vec<Vec<u32>> = Vec::with_capacity(num_compiled);

    for cf in &emitter.compiled {
        call_graph.push(cf.call_targets.clone());
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

/// Dead data elimination: remove unreferenced strings from the data section.
/// Scans live function bodies for i32.const references into the data region,
/// compacts data_bytes to only keep referenced strings, and patches all
/// i32.const values in live functions to use the new offsets.
pub fn eliminate_dead_data(emitter: &mut WasmEmitter) -> usize {
    let data_start = super::NEWLINE_OFFSET;
    let data_end = data_start + emitter.data_bytes.len() as u32;
    if emitter.data_bytes.len() <= 1 { return 0; } // only newline byte

    // Build set of known string offsets from the intern table.
    // ONLY these exact offsets are valid data references — prevents false
    // positives from integer constants that happen to fall in the data range.
    let known_string_offsets: HashSet<u32> = emitter.strings.values().copied().collect();

    // Step 1: Collect i32.const values that match known string offsets
    let mut referenced_offsets: HashSet<u32> = HashSet::new();
    // Always keep the newline byte
    referenced_offsets.insert(data_start);

    for cf in &emitter.compiled {
        for (val, _pos) in scan_i32_consts(&cf.func) {
            let uval = val as u32;
            if uval == data_start || known_string_offsets.contains(&uval) {
                referenced_offsets.insert(uval);
            }
        }
    }

    // Step 2: Determine which string entries to keep
    // Strings are stored as [len:i32][cap:i32][data...] at known offsets
    let mut old_to_new: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut new_data: Vec<u8> = Vec::new();

    // First byte is newline (0x0A)
    new_data.push(emitter.data_bytes[0]);
    old_to_new.insert(data_start, data_start); // newline stays at same offset

    // Preserve the embedded Unicode case-table region verbatim. It sits at the
    // FRONT of data_bytes (right after the newline byte) and must NOT be walked as
    // interned-string `[len][cap][data]` entries (its raw bytes would misparse and
    // corrupt the keep/compact loop + heap), nor shift when dead strings are
    // compacted (the case lookup functions bake its absolute addresses). Copy it
    // through unchanged and begin the interned-string walk after it.
    let table_bytes = emitter.case_table_bytes;
    if table_bytes > 0 {
        new_data.extend_from_slice(&emitter.data_bytes[1..1 + table_bytes]);
    }
    let mut read_pos = 1usize + table_bytes; // skip newline + case tables
    while read_pos + 8 <= emitter.data_bytes.len() {
        let old_offset = data_start + read_pos as u32;
        let slen = u32::from_le_bytes([
            emitter.data_bytes[read_pos],
            emitter.data_bytes[read_pos + 1],
            emitter.data_bytes[read_pos + 2],
            emitter.data_bytes[read_pos + 3],
        ]);
        let entry_size = 8 + slen as usize; // len + cap + data
        if read_pos + entry_size > emitter.data_bytes.len() { break; }

        if referenced_offsets.contains(&old_offset) {
            let new_offset = data_start + new_data.len() as u32;
            old_to_new.insert(old_offset, new_offset);
            new_data.extend_from_slice(&emitter.data_bytes[read_pos..read_pos + entry_size]);
        }
        read_pos += entry_size;
    }

    let removed = emitter.data_bytes.len() - new_data.len();
    if removed == 0 { return 0; }

    // Step 3: Patch i32.const values that are known string offsets
    for cf in &mut emitter.compiled {
        let consts = scan_i32_consts(&cf.func);
        let needs_patch = consts.iter().any(|(val, _)| {
            let uval = *val as u32;
            known_string_offsets.contains(&uval)
                && old_to_new.get(&uval).map_or(false, |&nv| nv != uval)
        });
        if !needs_patch { continue; }
        let bytes = cf.func.clone().into_raw_body();
        let mut patched = bytes.clone();
        let mut did_patch = false;
        for (val, byte_pos) in &consts {
            let uval = *val as u32;
            if !known_string_offsets.contains(&uval) { continue; }
            if let Some(&new_offset) = old_to_new.get(&uval) {
                if new_offset != uval {
                    let (_, consumed) = read_leb128_i32(&bytes[*byte_pos..]);
                    encode_i32_leb128_fixed(&mut patched[*byte_pos..*byte_pos + consumed], new_offset as i32);
                    did_patch = true;
                }
            }
        }
        if did_patch {
            cf.patched_body = Some(patched);
        }
    }

    // Step 4: Replace data_bytes
    emitter.data_bytes = new_data;

    // Update string offset table
    for (_key, offset) in emitter.strings.iter_mut() {
        if let Some(&new_off) = old_to_new.get(offset) {
            *offset = new_off;
        }
    }

    removed
}

/// Encode i32 as signed LEB128 into exactly `buf.len()` bytes (padded).
fn encode_i32_leb128_fixed(buf: &mut [u8], value: i32) {
    let mut val = value;
    let len = buf.len();
    for i in 0..len {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        if i < len - 1 {
            byte |= 0x80; // continuation bit
        }
        buf[i] = byte;
    }
}

/// Scan a compiled Function for i32.const instructions.
/// Returns `(value, byte_position_of_leb128_value)` for each i32.const found.
/// Uses proper instruction parsing (not naive byte scanning) to avoid false matches.
fn scan_i32_consts(func: &Function) -> Vec<(i32, usize)> {
    let bytes = func.clone().into_raw_body();
    let mut results = Vec::new();
    let mut pos = skip_locals(&bytes);
    while pos < bytes.len() {
        let opcode = bytes[pos];
        pos += 1;
        match opcode {
            0x41 => {
                // i32.const — this IS what we're looking for
                let value_start = pos;
                let (val, consumed) = read_leb128_i32(&bytes[pos..]);
                pos += consumed;
                results.push((val, value_start));
            }
            // All other opcodes: skip using the same logic as extract_call_targets
            0x10 => { let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c; }
            0x11 => { let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c; let (_, c2) = read_leb128_u32(&bytes[pos..]); pos += c2; }
            0x02 | 0x03 | 0x04 | 0x06 => { pos += block_type_size(&bytes[pos..]); }
            0x0C | 0x0D => { let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c; }
            0x0E => {
                let (count, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                for _ in 0..=count { if pos >= bytes.len() { break; } let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c; }
            }
            0x20 | 0x21 | 0x22 | 0x23 | 0x24 => { let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c; }
            0x28..=0x3E => { let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c; let (_, c2) = read_leb128_u32(&bytes[pos..]); pos += c2; }
            0x3F | 0x40 => { let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c; }
            0x42 => { let (_, c) = read_leb128_i64(&bytes[pos..]); pos += c; }
            0x43 => { pos += 4; }
            0x44 => { pos += 8; }
            0xD0 => { pos += 1; }
            0xD2 => { let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c; }
            0xFC => {
                let (sub, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                match sub {
                    0x08 | 0x0A | 0x0C | 0x0E => { let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c; let (_, c2) = read_leb128_u32(&bytes[pos..]); pos += c2; }
                    0x09 | 0x0B | 0x0D | 0x0F | 0x10 | 0x11 => { let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c; }
                    _ => {}
                }
            }
            0xFD => {
                let (sub, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                if sub <= 11 || (sub >= 84 && sub <= 95) {
                    let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c; let (_, c2) = read_leb128_u32(&bytes[pos..]); pos += c2;
                } else if sub == 12 { pos += 16; }
                else if sub == 13 { pos += 16; }
                else if sub >= 21 && sub <= 34 { pos += 1; }
            }
            _ => {}
        }
    }
    results
}

/// Skip local declarations at the start of a function body, return position after locals.
fn skip_locals(bytes: &[u8]) -> usize {
    let mut pos = 0;
    if pos < bytes.len() {
        let (num_groups, consumed) = read_leb128_u32(&bytes[pos..]);
        pos += consumed;
        for _ in 0..num_groups {
            if pos >= bytes.len() { break; }
            let (_, consumed) = read_leb128_u32(&bytes[pos..]);
            pos += consumed;
            if pos < bytes.len() { pos += 1; }
        }
    }
    pos
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
            // 0xFC prefix: multi-byte instructions (saturating trunc, bulk memory)
            0xFC => {
                if pos < bytes.len() {
                    let (sub_opcode, consumed) = read_leb128_u32(&bytes[pos..]);
                    pos += consumed;
                    match sub_opcode {
                        // memory.init: segment_idx + memory_idx
                        0x08 => {
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                        }
                        // data.drop: segment_idx
                        0x09 => {
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                        }
                        // memory.copy: dst_mem + src_mem
                        0x0A => {
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                        }
                        // memory.fill: memory_idx
                        0x0B => {
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                        }
                        // table.init: elem_idx + table_idx
                        0x0C => {
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                        }
                        // elem.drop: elem_idx
                        0x0D => {
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                        }
                        // table.copy: dst_table + src_table
                        0x0E => {
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                        }
                        // table.grow, table.size, table.fill: table_idx
                        0x0F | 0x10 | 0x11 => {
                            let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                        }
                        // 0x00-0x07: saturating truncation (no extra operands)
                        _ => {}
                    }
                }
            }
            // 0xFD prefix: SIMD instructions (V128)
            0xFD => {
                if pos < bytes.len() {
                    let (sub_opcode, consumed) = read_leb128_u32(&bytes[pos..]);
                    pos += consumed;
                    // v128.load/store variants have memarg (align + offset)
                    if sub_opcode <= 11 || (sub_opcode >= 84 && sub_opcode <= 91)
                        || (sub_opcode >= 92 && sub_opcode <= 95)
                    {
                        let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                        let (_, c) = read_leb128_u32(&bytes[pos..]); pos += c;
                    } else if sub_opcode == 12 {
                        // v128.const: 16 bytes
                        pos += 16;
                    } else if sub_opcode == 13 {
                        // i8x16.shuffle: 16 lane indices
                        pos += 16;
                    } else if sub_opcode >= 21 && sub_opcode <= 34 {
                        // extract/replace lane: 1 byte lane index
                        pos += 1;
                    }
                    // All other SIMD: no extra operands
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

include!("dce_p2.rs");
