//! Emit WASM bytes for one `.almd` file via the playground's exact pipeline.
//! Usage: wasmgen-harness <input.almd> <output.wasm>
//!
//! Run natively and on wasm32-wasip1; the two outputs must match byte-for-byte
//! (host-architecture codegen determinism). See scripts/check-host-determinism.sh.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let inp = args.get(1).expect("usage: wasmgen-harness <in.almd> <out.wasm>");
    let outp = args.get(2).expect("usage: wasmgen-harness <in.almd> <out.wasm>");
    let source = std::fs::read_to_string(inp).expect("read input");

    let tokens = almide::lexer::Lexer::tokenize(&source);
    let mut parser = almide::parser::Parser::new(tokens);
    let mut program = parser.parse().expect("parse failed");
    let canon = almide::canonicalize::canonicalize_program(&program, std::iter::empty());
    let mut checker = almide::check::Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    let _ = checker.infer_program(&mut program);
    let mut ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
    almide::mono::monomorphize(&mut ir);
    match almide::codegen::codegen(&mut ir, almide::codegen::pass::Target::Wasm) {
        almide::codegen::CodegenOutput::Binary(bytes) => {
            std::fs::write(outp, &bytes).expect("write output");
        }
        almide::codegen::CodegenOutput::Source(_) => panic!("expected WASM binary output"),
    }
}
