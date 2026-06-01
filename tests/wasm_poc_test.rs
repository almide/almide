#[test]
fn test_wasm_empty_program_valid() {
    let mut empty_ir = almide::ir::IrProgram {
        functions: vec![],
        top_lets: vec![],
        type_decls: vec![],
        var_table: almide::ir::VarTable::new(),
        def_table: Default::default(),
        modules: vec![],
        type_registry: almide::types::TypeConstructorRegistry::new(),
        effect_fn_names: Default::default(),
        effect_map: Default::default(),
        codegen_annotations: Default::default(),
        used_stdlib_modules: Default::default(),
    };
    // Go through the public certified entry — `emit` is `pub(crate)` now, the
    // sole door being `codegen` → Verified → Canonical → emit. This also
    // exercises the full pipeline incl. the terminal CanonicalizePass.
    let bytes = match almide::codegen::codegen(&mut empty_ir, almide::codegen::pass::Target::Wasm) {
        almide::codegen::CodegenOutput::Binary(b) => b,
        almide::codegen::CodegenOutput::Source(_) => panic!("wasm codegen must return Binary"),
    };

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
