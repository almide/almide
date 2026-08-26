//! Unit 3: execution via the ported reference interpreter (the executable
//! spec, L2 of ARCHITECTURE.md §2).
//!
//! `run_file` assembles the interpreter's canonical cut — the exact sequence
//! the crate's own eval_test pins (parse → canonicalize → check →
//! `lower_program` → `almide_driver::link_ir`, which owns the
//! optimize→mono→ir_link order) — but through the FULL resolve/canonicalize
//! path so fixtures with stdlib imports get the same env the checker parity
//! gates validated. Stdlib calls execute through the interpreter's
//! self-host-registry bridge, exactly as the incumbent's 3-way oracle runs.

pub struct RunResult {
    pub exit: i32,
    pub stdout: String,
    pub stderr: String,
}

// Commissioned (Stage 2): the driver moved to the root crate so the product
// wasm leg and these gates judge ONE implementation. Same pipeline, same
// name — `run_file` and every test keep calling s5::lower_to_ir.
pub use almide::wasm_leg::lower_to_ir;

pub fn run_file(path: &str, source_text: &str) -> Result<RunResult, String> {
    let ir = lower_to_ir(path, source_text)?;
    let out = almide::interp::Interpreter::new(&ir).run_main();
    // Surface the distinguished-outcome reason (Unsupported carries it in the
    // status, not in stderr) so harnesses can report skip classes precisely.
    let stderr = match &out.status {
        almide::interp::RunStatus::Unsupported(r) => r.clone(),
        _ => out.stderr.clone(),
    };
    Ok(RunResult { exit: out.exit_code(), stdout: out.stdout, stderr })
}
