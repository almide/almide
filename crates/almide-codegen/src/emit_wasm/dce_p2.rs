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
            Instruction::F64Const(3.14159_f64.into()),
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
        f.instruction(&Instruction::F64Const(3.14_f64.into()));
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
        tf.instruction(&Instruction::F64Const(f64::MAX.into()));
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
