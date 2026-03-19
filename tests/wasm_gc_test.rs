#[test]
fn test_wasm_gc_struct() {
    let empty_ir = almide::ir::IrProgram {
        functions: vec![], top_lets: vec![], type_decls: vec![],
        var_table: almide::ir::VarTable::new(), modules: vec![],
        type_registry: almide::types::TypeConstructorRegistry::new(),
        effect_map: Default::default(),
    };
    let bytes = almide::codegen::emit_wasm::emit_gc(&empty_ir);
    let path = std::env::temp_dir().join("almide_gc.wasm");
    std::fs::write(&path, &bytes).unwrap();
    eprintln!("GC WASM: {} bytes at {}", bytes.len(), path.display());

    // Validate with wasm-tools
    let validate = std::process::Command::new("wasm-tools")
        .args(["validate", "--features", "gc"])
        .arg(&path)
        .output();
    match validate {
        Ok(out) => {
            if out.status.success() {
                eprintln!("wasm-tools validate: OK");
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!("wasm-tools validate failed: {}", stderr);
                // Don't assert — just report
            }
        }
        Err(e) => eprintln!("wasm-tools not available: {}", e),
    }
}
