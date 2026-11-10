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
    /// The `native/*.rs` text this program is built with (`cli::native_rust_text`).
    pub(crate) native_text: String,
}

impl PendingRust {
    /// The native test harness uses the same checked input as build/run.
    /// An unsupported body is a compilation failure, never an implicit v0 run.
    pub(crate) fn generate_tests(mut self) -> Result<String, String> {
        let input = self.native_input.take().ok_or_else(|| "native tests require checked IR".to_string())?;
        render_native_tests(&self.source, std::mem::take(&mut self.native_text), input)
    }

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

pub(crate) fn render_native_tests(source: &str, native_text: String, (ir, checker): NativeInput) -> Result<String, String> {
    let _callbacks = almide_mir::native_callbacks::Scope::enter(native_text);
    let (rust, _) = almide_mir::pipeline::try_render_rust_checked_tests_certified(
        source, ir, checker, almide_wasm_run::fs_scope(),
    ).map_err(|error| format!("native v1 test render: {error:?}"))?;
    if almide_base::env::flag("ALMIDE_VERIFIED_DEBUG") {
        eprintln!("native: v1 trust-spine render (test harness)");
    }
    Ok(rust)
}

/// [`try_compile_reporting`] up to, not including, the codegen.
pub(crate) fn try_compile_pending(file: &str, no_check: bool, codegen_opts: &codegen::CodegenOptions) -> Result<(PendingRust, Option<almide::ir::IrProgram>, Vec<String>), String> {
    let t = cli::PhaseTimer::start();
    let (ir_program, outside_guarantee, native_input, source) = compile_to_linked_ir_with_source(file, no_check, &t)?;
    let metered = ir_program.as_ref().filter(|ir| almide::ir::runtime_use::uses_metered_prims(ir)).cloned();
    let ir = ir_program.expect("IR required for codegen");
    let native_text = cli::native_rust_text(file);
    Ok((PendingRust { ir, opts: codegen_opts.clone(), source, native_input, native_text }, metered, outside_guarantee))
}

