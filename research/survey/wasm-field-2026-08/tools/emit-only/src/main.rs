//! `almide-emit-only <file.almd> <out.wasm>` — front + emit + WASI wrap,
//! byte-identical pipeline to `almide-wasm-run --emit-wasi`, minus execution.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file, out] = args.as_slice() else {
        eprintln!("usage: almide-emit-only <file.almd> <out.wasm>");
        return ExitCode::from(2);
    };
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {file}: {e}");
            return ExitCode::from(2);
        }
    };
    let ir = match almide_spine::s5::lower_to_ir(file, &text) {
        Ok(ir) => ir,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let bytes = match almide_wasm::emit_program(&ir) {
        Ok(b) => b,
        Err(almide_wasm::EmitError::Unsupported(r)) => {
            eprintln!("error: unsupported: {r}");
            return ExitCode::from(2);
        }
    };
    let w = match almide_wasm_run::wasi::to_wasi(&bytes) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("error: to_wasi: {e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = std::fs::write(out, &w) {
        eprintln!("error: {out}: {e}");
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}
