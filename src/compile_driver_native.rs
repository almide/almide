// Pending code generation and the native frontend handoff. Included by compile_driver.rs.

type NativeInput = (almide::ir::IrProgram, check::Checker);

#[cfg(test)]
#[path = "compile_driver_native_tests.rs"]
mod native_frontend_tests;

/// A program checked, lowered and linked, whose standard-codegen Rust has not
/// been generated yet. The native route renders a program on the v1 trust
/// spine first and ships that render when it applies; the standard codegen is
/// then never read, so it runs only when something asks for it (#4595: it was
/// ~10% of a small program's `almide build`). Every step that can print a
/// diagnostic or refuse the program has already run: the codegen reports
/// nothing of its own.
pub(crate) struct PendingRust {
    ir: almide::ir::IrProgram,
    opts: codegen::CodegenOptions,
    pub(crate) source: String,
    pub(crate) native_input: Option<NativeInput>,
}

impl PendingRust {
    /// The standard codegen's Rust (Codegen v3: Nanopass + Templates).
    pub(crate) fn generate(mut self) -> String {
        let t = cli::PhaseTimer::start();
        let code = match codegen::codegen_with(&mut self.ir, codegen::pass::Target::Rust, &self.opts) {
            codegen::CodegenOutput::Source(s) => s,
            codegen::CodegenOutput::Binary(_) => unreachable!(),
        };
        t.lap("codegen");
        code
    }
}

/// [`try_compile_reporting`] up to, not including, the codegen.
pub(crate) fn try_compile_pending(file: &str, no_check: bool, codegen_opts: &codegen::CodegenOptions) -> Result<(PendingRust, Option<almide::ir::IrProgram>, Vec<String>), String> {
    let t = cli::PhaseTimer::start();
    let (ir_program, outside_guarantee, native_input, source) = compile_to_linked_ir_with_source(file, no_check, &t)?;
    let metered = ir_program.as_ref().filter(|ir| almide::ir::runtime_use::uses_metered_prims(ir)).cloned();
    let ir = ir_program.expect("IR required for codegen");
    Ok((PendingRust { ir, opts: codegen_opts.clone(), source, native_input }, metered, outside_guarantee))
}

