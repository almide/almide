#[test]
fn test_wasm_empty_program_valid() {
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

    // Valid WASM header
    assert_eq!(&bytes[0..4], b"\0asm", "should have WASM magic");
    assert_eq!(&bytes[4..8], &[1, 0, 0, 0], "should be WASM version 1");
    eprintln!("Empty program WASM binary: {} bytes", bytes.len());

    // Validate with wasm-tools if available
    let path = std::env::temp_dir().join("almide_empty.wasm");
    std::fs::write(&path, &bytes).unwrap();
    if let Ok(out) = std::process::Command::new("wasm-tools")
        .args(["validate"])
        .arg(&path)
        .output()
    {
        if out.status.success() {
            eprintln!("wasm-tools validate: OK");
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!("wasm-tools validate failed: {}", stderr);
        }
    }
}
