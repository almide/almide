#[test]
fn test_wasm_hello_world() {
    // Phase 0 PoC: emit a hardcoded hello world WASM
    let empty_ir = almide::ir::IrProgram {
        functions: vec![],
        top_lets: vec![],
        type_decls: vec![],
        var_table: almide::ir::VarTable::new(),
        modules: vec![],
        type_registry: almide::types::TypeConstructorRegistry::new(),
        effect_map: Default::default(),
    };
    let bytes = almide::codegen::emit_wasm::emit(&empty_ir);

    // Verify valid WASM
    assert_eq!(&bytes[0..4], b"\0asm", "should have WASM magic");
    assert_eq!(&bytes[4..8], &[1, 0, 0, 0], "should be WASM version 1");
    eprintln!("WASM binary size: {} bytes", bytes.len());

    // Write to temp and run with wasmtime
    let path = std::env::temp_dir().join("almide_hello.wasm");
    std::fs::write(&path, &bytes).unwrap();
    eprintln!("Written to {}", path.display());

    match std::process::Command::new("wasmtime").arg(&path).output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert_eq!(stdout.trim(), "Hello, Almide!", "WASM output mismatch");
            eprintln!("wasmtime output: {}", stdout.trim());
        }
        Err(e) => {
            eprintln!("wasmtime not available ({}), skipping runtime test", e);
        }
    }
}
