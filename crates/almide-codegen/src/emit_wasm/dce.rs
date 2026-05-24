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

    // Step 1: Collect all i32.const values referencing the data section
    let mut referenced_offsets: HashSet<u32> = HashSet::new();
    // Always keep the newline byte
    referenced_offsets.insert(data_start);

    for cf in &emitter.compiled {
        let bytes = cf.func.clone().into_raw_body();
        let mut pos = 0;
        // Skip local declarations
        if pos < bytes.len() {
            let (num_groups, consumed) = read_leb128_u32(&bytes[pos..]);
            pos += consumed;
            for _ in 0..num_groups {
                if pos >= bytes.len() { break; }
                let (_, consumed) = read_leb128_u32(&bytes[pos..]);
                pos += consumed;
                if pos < bytes.len() { pos += 1; } // type byte
            }
        }
        // Scan for i32.const (0x41)
        while pos < bytes.len() {
            if bytes[pos] == 0x41 {
                pos += 1;
                let (val, consumed) = read_leb128_i32(&bytes[pos..]);
                pos += consumed;
                let uval = val as u32;
                if uval >= data_start && uval < data_end {
                    referenced_offsets.insert(uval);
                }
            } else {
                pos += 1;
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

    let mut read_pos = 1usize; // skip newline
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

    // Step 3: Patch i32.const values in live functions
    for cf in &mut emitter.compiled {
        let bytes = cf.func.clone().into_raw_body();
        let mut patched = bytes.clone();
        let mut pos = 0;
        // Skip local declarations
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
        let mut did_patch = false;
        while pos < bytes.len() {
            if bytes[pos] == 0x41 {
                let const_start = pos + 1;
                pos += 1;
                let (val, consumed) = read_leb128_i32(&bytes[pos..]);
                pos += consumed;
                let uval = val as u32;
                if let Some(&new_offset) = old_to_new.get(&uval) {
                    if new_offset != uval {
                        // Re-encode with same byte count (pad with LEB128 continuation)
                        encode_i32_leb128_fixed(&mut patched[const_start..const_start + consumed], new_offset as i32);
                        did_patch = true;
                    }
                }
            } else {
                pos += 1;
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

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_encoder::Instruction;

    /// Build a Function with given instructions and extract its call targets.
    fn calls_in(instrs: &[Instruction]) -> Vec<u32> {
        let mut f = Function::new([]);
        for i in instrs { f.instruction(i); }
        f.instruction(&Instruction::End);
        extract_call_targets(&f)
    }

    #[test]
    fn simple_call() {
        let targets = calls_in(&[Instruction::Call(42)]);
        assert_eq!(targets, vec![42]);
    }

    #[test]
    fn multiple_calls() {
        let targets = calls_in(&[
            Instruction::Call(1),
            Instruction::Call(2),
            Instruction::Call(3),
        ]);
        assert_eq!(targets, vec![1, 2, 3]);
    }

    #[test]
    fn no_calls() {
        let targets = calls_in(&[
            Instruction::I32Const(0),
            Instruction::Drop,
        ]);
        assert!(targets.is_empty());
    }

    // ── 0xFC prefix: bulk memory ops must not desync the scanner ──

    #[test]
    fn call_after_memory_copy() {
        let targets = calls_in(&[
            Instruction::I32Const(0),
            Instruction::I32Const(0),
            Instruction::I32Const(8),
            Instruction::MemoryCopy { src_mem: 0, dst_mem: 0 },
            Instruction::Call(99),
        ]);
        assert_eq!(targets, vec![99]);
    }

    #[test]
    fn call_after_memory_fill() {
        let targets = calls_in(&[
            Instruction::I32Const(0),
            Instruction::I32Const(0),
            Instruction::I32Const(8),
            Instruction::MemoryFill(0),
            Instruction::Call(77),
        ]);
        assert_eq!(targets, vec![77]);
    }

    #[test]
    fn calls_around_memory_copy() {
        let targets = calls_in(&[
            Instruction::Call(10),
            Instruction::I32Const(0),
            Instruction::I32Const(0),
            Instruction::I32Const(4),
            Instruction::MemoryCopy { src_mem: 0, dst_mem: 0 },
            Instruction::Call(20),
            Instruction::I32Const(0),
            Instruction::I32Const(0),
            Instruction::I32Const(4),
            Instruction::MemoryFill(0),
            Instruction::Call(30),
        ]);
        assert_eq!(targets, vec![10, 20, 30]);
    }

    // ── Regression: multiple memory ops in sequence (init_globals pattern) ──

    #[test]
    fn many_memory_ops_then_call() {
        let mut instrs = Vec::new();
        for _ in 0..10 {
            instrs.push(Instruction::I32Const(0));
            instrs.push(Instruction::I32Const(0));
            instrs.push(Instruction::I32Const(16));
            instrs.push(Instruction::MemoryCopy { src_mem: 0, dst_mem: 0 });
        }
        instrs.push(Instruction::Call(55));
        let targets = calls_in(&instrs);
        assert_eq!(targets, vec![55]);
    }

    // ── Other multi-byte instructions that must not confuse the scanner ──

    #[test]
    fn call_after_block_and_loop() {
        let targets = calls_in(&[
            Instruction::Block(wasm_encoder::BlockType::Empty),
            Instruction::Call(1),
            Instruction::End,
            Instruction::Loop(wasm_encoder::BlockType::Empty),
            Instruction::Call(2),
            Instruction::End,
        ]);
        assert_eq!(targets, vec![1, 2]);
    }

    #[test]
    fn call_after_br_table() {
        let targets = calls_in(&[
            Instruction::I32Const(0),
            Instruction::BrTable(
                std::borrow::Cow::Borrowed(&[0, 1]),
                2,
            ),
            Instruction::Call(42),
        ]);
        assert_eq!(targets, vec![42]);
    }

    #[test]
    fn call_after_i64_const() {
        let targets = calls_in(&[
            Instruction::I64Const(0x7FFF_FFFF_FFFF),
            Instruction::Drop,
            Instruction::Call(88),
        ]);
        assert_eq!(targets, vec![88]);
    }

    #[test]
    fn call_after_f64_const() {
        let targets = calls_in(&[
            Instruction::F64Const(3.14159),
            Instruction::Drop,
            Instruction::Call(66),
        ]);
        assert_eq!(targets, vec![66]);
    }

    #[test]
    fn call_after_global_set() {
        let targets = calls_in(&[
            Instruction::I32Const(0),
            Instruction::GlobalSet(5),
            Instruction::Call(33),
        ]);
        assert_eq!(targets, vec![33]);
    }

    #[test]
    fn call_after_memory_load_store() {
        let targets = calls_in(&[
            Instruction::I32Const(0),
            Instruction::I32Load(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 }),
            Instruction::I32Const(0),
            Instruction::I32Store(wasm_encoder::MemArg { offset: 4, align: 2, memory_index: 0 }),
            Instruction::Call(44),
        ]);
        assert_eq!(targets, vec![44]);
    }

    // ── Exhaustive coverage: every instruction type the emitter uses ──

    /// Verify the scanner stays in sync through a function body that
    /// exercises every instruction family the WASM emitter produces.
    /// If a new instruction type is added without updating the scanner,
    /// the call at the end will be missed and this test will fail.
    #[test]
    fn exhaustive_instruction_coverage() {
        let mut f = Function::new([(1, wasm_encoder::ValType::I32)]);
        // Numeric constants
        f.instruction(&Instruction::I32Const(999));
        f.instruction(&Instruction::I64Const(0x7FFF_FFFF_FFFF));
        f.instruction(&Instruction::F64Const(3.14));
        f.instruction(&Instruction::Drop);
        f.instruction(&Instruction::Drop);
        // Local/global access
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::LocalSet(0));
        f.instruction(&Instruction::LocalTee(0));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::GlobalGet(0));
        f.instruction(&Instruction::GlobalSet(0));
        // Memory load/store
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Load(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 }));
        f.instruction(&Instruction::Drop);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Store(wasm_encoder::MemArg { offset: 4, align: 2, memory_index: 0 }));
        f.instruction(&Instruction::MemorySize(0));
        f.instruction(&Instruction::Drop);
        // 0xFC prefix: bulk memory
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::MemoryCopy { src_mem: 0, dst_mem: 0 });
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::MemoryFill(0));
        // Control flow: block, br, br_if, if
        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Br(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::BrIf(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::End);
        // br_table
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::BrTable(std::borrow::Cow::Borrowed(&[0]), 0));
        // call_indirect
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::CallIndirect { type_index: 0, table_index: 0 });
        f.instruction(&Instruction::Drop);
        // THE call — must be found after all the above
        f.instruction(&Instruction::Call(777));
        f.instruction(&Instruction::End);
        let targets = extract_call_targets(&f);
        assert!(targets.contains(&777),
            "scanner lost sync after exhaustive instruction mix: call(777) not found in {:?}", targets);
    }

    /// Cross-validation: build the same function via instructions AND check
    /// that the byte-count is exactly what we expect. This catches the case
    /// where wasm_encoder changes its encoding and the scanner drifts.
    #[test]
    fn memory_copy_encoding_size() {
        let mut f = Function::new([]);
        let before_len = f.clone().into_raw_body().len();
        f.instruction(&Instruction::MemoryCopy { src_mem: 0, dst_mem: 0 });
        let after_len = f.clone().into_raw_body().len();
        // memory.copy encodes as: 0xFC 0x0A 0x00 0x00 = 4 bytes
        assert_eq!(after_len - before_len, 4,
            "memory.copy encoding changed — update DCE scanner's 0xFC handler");
    }

    #[test]
    fn memory_fill_encoding_size() {
        let mut f = Function::new([]);
        let before_len = f.clone().into_raw_body().len();
        f.instruction(&Instruction::MemoryFill(0));
        let after_len = f.clone().into_raw_body().len();
        // memory.fill encodes as: 0xFC 0x0B 0x00 = 3 bytes
        assert_eq!(after_len - before_len, 3,
            "memory.fill encoding changed — update DCE scanner's 0xFC handler");
    }

    // ══════════════════════════════════════════════════════════════════
    // Cross-validation: TrackedFunction vs wasmparser (reference impl)
    // ══════════════════════════════════════════════════════════════════
    //
    // These tests build a TrackedFunction, then independently parse its
    // bytecode with `wasmparser` to extract call targets. If the two
    // disagree, TrackedFunction has a recording bug.

    use super::super::TrackedFunction;

    /// Extract call targets from raw function bytes using wasmparser.
    /// This is the "ground truth" — wasmparser is battle-tested across
    /// the entire WASM ecosystem.
    fn wasmparser_call_targets(tf: &TrackedFunction) -> Vec<u32> {
        use wasmparser::{Parser, Payload, Operator};
        // Build a minimal valid WASM module containing just this function
        let mut module = wasm_encoder::Module::new();
        // Type section: () -> ()
        let mut types = wasm_encoder::TypeSection::new();
        types.ty().function([], []);
        module.section(&types);
        // Function section
        let mut funcs = wasm_encoder::FunctionSection::new();
        funcs.function(0);
        module.section(&funcs);
        // Code section
        let mut code = wasm_encoder::CodeSection::new();
        code.function(&tf.inner);
        module.section(&code);
        let wasm_bytes = module.finish();

        let mut targets = Vec::new();
        for payload in Parser::new(0).parse_all(&wasm_bytes) {
            if let Ok(Payload::CodeSectionEntry(body)) = payload {
                let ops = body.get_operators_reader().expect("valid body");
                for op in ops {
                    match op {
                        Ok(Operator::Call { function_index }) => targets.push(function_index),
                        Ok(Operator::ReturnCall { function_index }) => targets.push(function_index),
                        _ => {}
                    }
                }
            }
        }
        targets
    }

    /// Cross-validate: TrackedFunction recording == wasmparser scan
    fn assert_tracked_matches_wasmparser(tf: &TrackedFunction) {
        let tracked = &tf.call_targets;
        let parsed = wasmparser_call_targets(tf);
        assert_eq!(tracked, &parsed,
            "TrackedFunction disagrees with wasmparser!\n  tracked: {:?}\n  wasmparser: {:?}",
            tracked, parsed);
    }

    #[test]
    fn cross_validate_simple() {
        let mut tf = TrackedFunction::new([]);
        tf.instruction(&Instruction::Call(5));
        tf.instruction(&Instruction::Call(10));
        tf.instruction(&Instruction::End);
        assert_tracked_matches_wasmparser(&tf);
    }

    #[test]
    fn cross_validate_no_calls() {
        let mut tf = TrackedFunction::new([]);
        tf.instruction(&Instruction::I32Const(42));
        tf.instruction(&Instruction::Drop);
        tf.instruction(&Instruction::End);
        assert_tracked_matches_wasmparser(&tf);
    }

    #[test]
    fn cross_validate_memory_ops() {
        let mut tf = TrackedFunction::new([]);
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::I32Const(8));
        tf.instruction(&Instruction::MemoryCopy { src_mem: 0, dst_mem: 0 });
        tf.instruction(&Instruction::Call(99));
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::I32Const(4));
        tf.instruction(&Instruction::MemoryFill(0));
        tf.instruction(&Instruction::Call(100));
        tf.instruction(&Instruction::End);
        assert_tracked_matches_wasmparser(&tf);
    }

    #[test]
    fn cross_validate_complex_control_flow() {
        let mut tf = TrackedFunction::new([(1, wasm_encoder::ValType::I32)]);
        tf.instruction(&Instruction::Call(1));
        tf.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        tf.instruction(&Instruction::Call(2));
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::BrIf(0));
        tf.instruction(&Instruction::Call(3));
        tf.instruction(&Instruction::End);
        tf.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        tf.instruction(&Instruction::Call(4));
        tf.instruction(&Instruction::I32Const(1));
        tf.instruction(&Instruction::BrIf(0));
        tf.instruction(&Instruction::End);
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        tf.instruction(&Instruction::Call(5));
        tf.instruction(&Instruction::Else);
        tf.instruction(&Instruction::Call(6));
        tf.instruction(&Instruction::End);
        tf.instruction(&Instruction::Call(7));
        tf.instruction(&Instruction::End);
        assert_tracked_matches_wasmparser(&tf);
    }

    #[test]
    fn cross_validate_all_instruction_families() {
        let mut tf = TrackedFunction::new([(1, wasm_encoder::ValType::I32)]);
        // Constants
        tf.instruction(&Instruction::I32Const(0x7FFFFFFF));
        tf.instruction(&Instruction::Drop);
        tf.instruction(&Instruction::I64Const(0x7FFFFFFFFFFFFFFF));
        tf.instruction(&Instruction::Drop);
        tf.instruction(&Instruction::F64Const(f64::MAX));
        tf.instruction(&Instruction::Drop);
        // Local/global
        tf.instruction(&Instruction::LocalGet(0));
        tf.instruction(&Instruction::LocalSet(0));
        tf.instruction(&Instruction::LocalTee(0));
        tf.instruction(&Instruction::Drop);
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::GlobalSet(0));
        tf.instruction(&Instruction::GlobalGet(0));
        tf.instruction(&Instruction::Drop);
        // Memory
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::I32Load(wasm_encoder::MemArg { offset: 100, align: 2, memory_index: 0 }));
        tf.instruction(&Instruction::Drop);
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::I64Load(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 }));
        tf.instruction(&Instruction::Drop);
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::F64Load(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 }));
        tf.instruction(&Instruction::Drop);
        // Bulk memory (0xFC)
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::I32Const(16));
        tf.instruction(&Instruction::MemoryCopy { src_mem: 0, dst_mem: 0 });
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::I32Const(16));
        tf.instruction(&Instruction::MemoryFill(0));
        // br_table
        tf.instruction(&Instruction::I32Const(0));
        tf.instruction(&Instruction::BrTable(std::borrow::Cow::Borrowed(&[0, 0, 0]), 0));
        // Numeric ops
        tf.instruction(&Instruction::I32Const(1));
        tf.instruction(&Instruction::I32Const(2));
        tf.instruction(&Instruction::I32Add);
        tf.instruction(&Instruction::Drop);
        // Call — THE target we must find
        tf.instruction(&Instruction::Call(42));
        tf.instruction(&Instruction::Call(999));
        tf.instruction(&Instruction::End);
        assert_tracked_matches_wasmparser(&tf);
    }

    /// Stress test: many calls interleaved with diverse instructions.
    #[test]
    fn cross_validate_stress() {
        let mut tf = TrackedFunction::new([(1, wasm_encoder::ValType::I32)]);
        for i in 0..50u32 {
            tf.instruction(&Instruction::I32Const(i as i32));
            tf.instruction(&Instruction::LocalSet(0));
            tf.instruction(&Instruction::Call(i));
            tf.instruction(&Instruction::I32Const(0));
            tf.instruction(&Instruction::I32Const(0));
            tf.instruction(&Instruction::I32Const(4));
            tf.instruction(&Instruction::MemoryCopy { src_mem: 0, dst_mem: 0 });
        }
        tf.instruction(&Instruction::End);
        assert_tracked_matches_wasmparser(&tf);
    }
}
