//! Direct WASM emission — Phase 0: PoC
//!
//! Two modes:
//! - **WASI**: Uses `wasi_snapshot_preview1.fd_write` for CLI (wasmtime)
//! - **Embed**: Uses `env.print` for browser/Playground (minimal size)

use wasm_encoder::{
    CodeSection, CompositeInnerType, CompositeType, DataSection, ExportSection,
    FieldType, Function, FunctionSection, ImportSection, Instruction,
    MemorySection, MemoryType, Module, StorageType, StructType, SubType,
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

/// Emit wasm-gc binary (no linear memory, host GC manages everything).
pub fn emit_gc(_program: &IrProgram) -> Vec<u8> {
    emit_gc_poc()
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

/// wasm-gc mode: uses struct/array GC types.
/// No linear memory, no allocator — the host GC manages everything.
/// This is the MoonBit-equivalent strategy.
fn emit_gc_poc() -> Vec<u8> {
    let mut module = Module::new();

    let mut types = TypeSection::new();

    // type 0: struct Point { x: i64, y: i64 }
    types.ty().subtype(&SubType {
        is_final: true,
        supertype_idx: None,
        composite_type: CompositeType {
            shared: false,
            inner: CompositeInnerType::Struct(StructType {
                fields: Box::new([
                    FieldType { element_type: StorageType::Val(ValType::I64), mutable: false },
                    FieldType { element_type: StorageType::Val(ValType::I64), mutable: false },
                ]),
            }),
        },
    });

    // type 1: print_i64(n: i64) -> ()
    types.ty().function(vec![ValType::I64], vec![]);

    // type 2: _start() -> ()
    types.ty().function(vec![], vec![]);

    module.section(&types);

    // import env.print_i64
    let mut imports = ImportSection::new();
    imports.import("e", "p", wasm_encoder::EntityType::Function(1));
    module.section(&imports);

    // function 1 = _start (type 2)
    let mut functions = FunctionSection::new();
    functions.function(2);
    module.section(&functions);

    // export
    let mut exports = ExportSection::new();
    exports.export("s", wasm_encoder::ExportKind::Func, 1);
    module.section(&exports);

    // code: create Point { x: 3, y: 4 }, print x + y
    let mut codes = CodeSection::new();
    let mut f = Function::new(vec![]);

    // Push x=3, y=4 on stack, then struct.new $Point
    f.instruction(&Instruction::I64Const(3));
    f.instruction(&Instruction::I64Const(4));
    f.instruction(&Instruction::StructNew(0)); // type 0 = Point

    // Duplicate: get x and y from the struct
    // We need local to store the struct ref
    // Actually, we need a local. Let's change approach.
    f.instruction(&Instruction::End);
    codes.function(&f);

    // Rewrite: use a local for the struct ref
    let mut codes = CodeSection::new();
    let point_ref = ValType::Ref(wasm_encoder::RefType {
        nullable: false,
        heap_type: wasm_encoder::HeapType::Concrete(0),
    });
    let mut f = Function::new(vec![(1, point_ref)]); // 1 local of type (ref $Point)

    // Create Point { x: 3, y: 4 }
    f.instruction(&Instruction::I64Const(3));
    f.instruction(&Instruction::I64Const(4));
    f.instruction(&Instruction::StructNew(0));
    f.instruction(&Instruction::LocalSet(0)); // store in local 0

    // Get x + y
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::StructGet { struct_type_index: 0, field_index: 0 }); // .x
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::StructGet { struct_type_index: 0, field_index: 1 }); // .y
    f.instruction(&Instruction::I64Add);

    // Call print_i64(x + y)
    f.instruction(&Instruction::Call(0)); // import index 0

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

    #[test]
    fn test_gc_poc_size() {
        let bytes = emit_gc_poc();
        assert_eq!(&bytes[0..4], b"\0asm");
        eprintln!("GC mode (Point struct + add): {} bytes", bytes.len());
    }
}
