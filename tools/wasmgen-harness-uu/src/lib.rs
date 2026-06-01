//! Browser-ABI determinism harness. Mirrors the playground's `compile_to_wasm`
//! (parse → check → lower → mono → codegen(Target::Wasm)) but built to
//! wasm32-unknown-unknown so the gate exercises the exact target the browser
//! playground runs the compiler on — catching wasm32-unknown-unknown-specific
//! failures (e.g. unconditional std::time, unsupported there) and host-pointer-
//! width codegen divergence that wasm32-wasip1 can mask.
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn compile_source_to_wasm(source: &str) -> Result<Vec<u8>, String> {
    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens);
    let mut program = parser.parse().map_err(|e| format!("parse: {e}"))?;
    let canon = almide::canonicalize::canonicalize_program(&program, std::iter::empty());
    let mut checker = almide::check::Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    let _ = checker.infer_program(&mut program);
    let mut ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
    almide::mono::monomorphize(&mut ir);
    match almide::codegen::codegen(&mut ir, almide::codegen::pass::Target::Wasm) {
        almide::codegen::CodegenOutput::Binary(bytes) => Ok(bytes),
        almide::codegen::CodegenOutput::Source(_) => Err("expected WASM binary".into()),
    }
}
