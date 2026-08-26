//! The product runner: `almide-wasm-run <file.almd> [--emit out.wasm]
//! [--emit-wasi out.wasm]` — the WASI form runs on STOCK runtimes.
//! — front + emit through the greenfield spine, then execute on the
//! SAME host the conformance gates verify. Stdout/stderr pass through;
//! the process exits with the module's exit code.

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut file = None;
    let mut emit_to = None;
    let mut emit_wasi_to = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--emit" => emit_to = it.next().cloned(),
            "--emit-wasi" => emit_wasi_to = it.next().cloned(),
            _ => file = Some(a.clone()),
        }
    }
    let Some(file) = file else {
        eprintln!("usage: almide-wasm-run <file.almd> [--emit out.wasm] [--emit-wasi out.wasm]");
        return ExitCode::from(2);
    };
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {file}: {e}");
            return ExitCode::from(2);
        }
    };
    let ir = match almide_spine::s5::lower_to_ir(&file, &text) {
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
    if let Some(out) = emit_to
        && let Err(e) = std::fs::write(&out, &bytes)
    {
        eprintln!("error: {out}: {e}");
        return ExitCode::from(2);
    }
    if let Some(out) = emit_wasi_to {
        match almide_wasm_run::wasi::to_wasi(&bytes) {
            Ok(w) => {
                if let Err(e) = std::fs::write(&out, &w) {
                    eprintln!("error: {out}: {e}");
                    return ExitCode::from(2);
                }
            }
            Err(e) => {
                eprintln!("error: to_wasi: {e}");
                return ExitCode::from(2);
            }
        }
    }
    match almide_wasm_run::run_wasm_real_stdin(&bytes) {
        Ok(r) => {
            print!("{}", r.stdout);
            eprint!("{}", r.stderr);
            let _ = std::io::stdout().flush();
            ExitCode::from(u8::try_from(r.exit.max(0)).unwrap_or(1))
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
