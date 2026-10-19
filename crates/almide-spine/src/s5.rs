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
    /// The run ended at the interpreter's own call-stack threshold
    /// (`RunStatus::StackExhausted`, C-196). `exit` is then 1 and `stderr`
    /// carries `Error: stack overflow` — the defined abort — but the threshold
    /// is the interpreter's, not the target's, so a harness counts it as a
    /// vote only where it matches the other leg, and otherwise as the C-196
    /// resource class (the abstain the old depth-as-fuel `-3` gave).
    pub stack_exhausted: bool,
}

// Commissioned (Stage 2): the driver moved to the root crate so the product
// wasm leg and these gates judge ONE implementation. Same pipeline, same
// name — `run_file` and every test keep calling s5::lower_to_ir.
pub use almide::wasm_leg::lower_to_ir;

pub fn run_file(path: &str, source_text: &str) -> Result<RunResult, String> {
    run_file_until(path, source_text, None)
}

/// [`run_file`], ending the interpreter run as `Unsupported` once `stop` is
/// set from another thread (#3809) — how a corpus sweep's per-fixture limit
/// stops a cut fixture instead of leaving its thread spinning.
pub fn run_file_until(
    path: &str,
    source_text: &str,
    stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<RunResult, String> {
    use almide::interp::{Interpreter, RunOutcome, RunStatus};
    let ir = lower_to_ir(path, source_text)?;
    // The front end polls nothing; everything after it does. A stop set
    // while the stdlib pool is first built is seen within 10 ms instead of
    // after the whole build, and one set earlier is seen at the pool wait or
    // at the interpreter's first step.
    let out = match stop {
        Some(stop) if !almide::interp::pool_ready_unless_stopped(&stop) => RunOutcome {
            status: RunStatus::Unsupported(almide::interp::STOPPED.to_string()),
            stdout: String::new(),
            stderr: String::new(),
            bridge_fallbacks: Vec::new(),
        },
        Some(stop) => Interpreter::new(&ir).with_stop(stop).run_main(),
        None => Interpreter::new(&ir).run_main(),
    };
    // Surface the distinguished-outcome reason (Unsupported carries it in the
    // status, not in stderr) so harnesses can report skip classes precisely.
    let stderr = match &out.status {
        almide::interp::RunStatus::Unsupported(r) => r.clone(),
        _ => out.stderr.clone(),
    };
    let stack_exhausted = matches!(out.status, almide::interp::RunStatus::StackExhausted);
    Ok(RunResult { exit: out.exit_code(), stdout: out.stdout, stderr, stack_exhausted })
}
