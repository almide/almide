//! Direct WASM emission — Phase 0: PoC
//!
//! Two modes:
//! - **WASI**: Uses `wasi_snapshot_preview1.fd_write` for CLI (wasmtime)
//! - **Embed**: Uses `env.print` for browser/Playground (minimal size)

use wasm_encoder::{
    CodeSection, DataSection, ExportSection, Function, FunctionSection,
    ImportSection, Instruction, MemorySection, MemoryType, Module,
    TypeSection, ValType,
};

use crate::ir::IrProgram;

/// WASM output mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WasmMode {
    /// WASI-compatible (wasmtime, wasmer). Uses fd_write for I/O.
    Wasi,
    /// Embed mode (browser, Playground). Uses short `env.print` import.
    Embed,
}

/// Emit a WASM binary from an IR program.
/// Phase 0: ignores the IR and emits a hardcoded hello world.
pub fn emit(_program: &IrProgram) -> Vec<u8> {
    emit_hello(WasmMode::Wasi)
}

/// Emit with explicit mode selection.
pub fn emit_with_mode(_program: &IrProgram, mode: WasmMode) -> Vec<u8> {
    emit_hello(mode)
}

/// Generate a minimal "Hello, Almide!\n" binary.
fn emit_hello(mode: WasmMode) -> Vec<u8> {
    let mut module = Module::new();

    // ── Type section ──
    // type 0: fd_write(fd: i32, iovs: i32, iovs_len: i32, nwritten: i32) -> i32
    // type 1: _start() -> ()
    let mut types = TypeSection::new();
    types.ty().function(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    types.ty().function(vec![], vec![]);
    module.section(&types);

    // ── Import section ──
    // import fd_write from WASI
    let mut imports = ImportSection::new();
    imports.import(
        "wasi_snapshot_preview1",
        "fd_write",
        wasm_encoder::EntityType::Function(0), // type index 0
    );
    module.section(&imports);

    // ── Function section ──
    // function 1: _start (type index 1)
    let mut functions = FunctionSection::new();
    functions.function(1); // type index 1
    module.section(&functions);

    // ── Memory section ──
    // 1 page = 64KB
    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memory);

    // ── Export section ──
    let mut exports = ExportSection::new();
    exports.export("memory", wasm_encoder::ExportKind::Memory, 0);
    exports.export("_start", wasm_encoder::ExportKind::Func, 1); // func index 1 (_start)
    module.section(&exports);

    let message = b"Hello, Almide!\n";
    let data_offset = 16u32; // leave room for iov struct at offset 0

    // ── Code section ──
    // _start function:
    //   1. Write iov struct at offset 0: { buf_ptr: data_offset, buf_len: message.len() }
    //   2. Call fd_write(fd=1, iovs=0, iovs_len=1, nwritten=8)
    let mut codes = CodeSection::new();
    let mut f = Function::new(vec![]);

    // Store iov.buf_ptr = data_offset at memory[0]
    f.instruction(&Instruction::I32Const(0)); // address
    f.instruction(&Instruction::I32Const(data_offset as i32)); // value
    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    }));

    // Store iov.buf_len = message.len() at memory[4]
    f.instruction(&Instruction::I32Const(4)); // address
    f.instruction(&Instruction::I32Const(message.len() as i32)); // value
    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    }));

    // Call fd_write(fd=1, iovs_ptr=0, iovs_len=1, nwritten_ptr=8)
    f.instruction(&Instruction::I32Const(1));  // fd: stdout
    f.instruction(&Instruction::I32Const(0));  // iovs pointer
    f.instruction(&Instruction::I32Const(1));  // iovs count
    f.instruction(&Instruction::I32Const(8));  // nwritten pointer
    f.instruction(&Instruction::Call(0));       // call fd_write (import index 0)
    f.instruction(&Instruction::Drop);         // discard return value

    f.instruction(&Instruction::End);
    codes.function(&f);
    module.section(&codes);

    // ── Data section (must be after Code) ──
    let mut data = DataSection::new();
    data.active(
        0,
        &wasm_encoder::ConstExpr::i32_const(data_offset as i32),
        message.iter().copied(),
    );
    module.section(&data);

    module.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emit_hello_is_valid_wasm() {
        let bytes = emit_hello(WasmMode::Wasi);
        // WASM magic number: \0asm
        assert_eq!(&bytes[0..4], b"\0asm");
        // WASM version 1
        assert_eq!(&bytes[4..8], &[1, 0, 0, 0]);
        // Should be small
        assert!(bytes.len() < 200, "WASM binary too large: {} bytes", bytes.len());
    }
}

/// Embed mode: minimal WASM with `env.putchar` import.
/// No WASI, no memory management, no data section for strings.
/// Target: sub-50 bytes for trivial programs.
fn emit_hello_embed() -> Vec<u8> {
    let mut module = Module::new();

    // type 0: putchar(ch: i32) -> ()
    let mut types = TypeSection::new();
    types.ty().function(vec![ValType::I32], vec![]);
    // type 1: _start() -> ()
    types.ty().function(vec![], vec![]);
    module.section(&types);

    // import env.putchar
    let mut imports = ImportSection::new();
    imports.import("e", "p", wasm_encoder::EntityType::Function(0));
    module.section(&imports);

    // function 1 = _start
    let mut functions = FunctionSection::new();
    functions.function(1);
    module.section(&functions);

    // export _start as "s" (1 byte)
    let mut exports = ExportSection::new();
    exports.export("s", wasm_encoder::ExportKind::Func, 1);
    module.section(&exports);

    // code: call putchar for each byte of "Hello, Almide!"
    let mut codes = CodeSection::new();
    let mut f = Function::new(vec![]);
    for &ch in b"Hello, Almide!" {
        f.instruction(&Instruction::I32Const(ch as i32));
        f.instruction(&Instruction::Call(0));
    }
    f.instruction(&Instruction::End);
    codes.function(&f);
    module.section(&codes);

    module.finish()
}

#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn test_wasi_size() {
        let bytes = emit_hello(WasmMode::Wasi);
        eprintln!("WASI mode: {} bytes", bytes.len());
    }

    #[test]
    fn test_embed_size() {
        let bytes = emit_hello_embed();
        eprintln!("Embed mode: {} bytes", bytes.len());
        assert!(bytes.len() < 120, "Embed should be under 120 bytes, got {}", bytes.len());
    }
}
