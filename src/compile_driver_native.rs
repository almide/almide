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
    /// The `[permissions] fs` roots of this program's own project (#4833):
    /// carried with it, never read back from the process-wide copy another
    /// file's check may have set meanwhile.
    pub(crate) fs_scope: Option<Vec<String>>,
    /// The modules the program's own sources declare (#2865): a call into
    /// one is the package's fn, whatever its key spells.
    pub(crate) package: std::collections::HashSet<String>,
}

impl PendingRust {
    /// The native test harness uses the same checked input as build/run: the
    /// certified v1 render, or, when it walls, the standard codegen with the
    /// v1-rendered regions of a metered program linked in (`note_test_wall`).
    pub(crate) fn generate_tests(mut self, metered: Option<almide::ir::IrProgram>) -> Result<String, String> {
        // `ALMIDE_IR_FAULT` names a pass of the standard codegen, which this
        // route never runs. A fault that cannot be injected must not read as
        // a clean compile (it used to: the file was reported as passed), so
        // the route fails the way the injected violation would — a compiler
        // panic the harness counts (tests/test_harness_compiler_panic_test.rs).
        if let Some(pass) = almide_base::env::var("ALMIDE_IR_FAULT") {
            panic!("ALMIDE_IR_FAULT={pass}: the native test render runs no standard-codegen pass to fault");
        }
        let input = self.native_input.take().ok_or_else(|| "native tests require checked IR".to_string())?;
        let host = self.host_source();
        let why = match render_native_tests(&self.source, std::mem::take(&mut self.native_text), host, input, self.fs_scope.clone())? {
            TestRender::Rendered(rust) => return Ok(rust),
            TestRender::Walled(why) => why,
        };
        note_test_wall(&self.source, &why);
        let (source, opts) = (std::mem::take(&mut self.source), self.opts.clone());
        let rust = self.generate();
        Ok(match metered {
            Some(ir) => cli::link_metered_regions(&source, rust, &why, ir, &opts),
            None => rust,
        })
    }

    /// The standard codegen's Rust for this program, made when the native
    /// render asks for it: the adapter of a module fn that `native/*.rs`
    /// calls back with a non-scalar signature is declared with the signature
    /// this text holds for it (#4640).
    pub(crate) fn host_source(&self) -> HostSource {
        host_source(&self.ir, &self.opts, &self.native_text)
    }

    /// The checked, lowered and linked program.
    pub(crate) fn ir(&self) -> &almide::ir::IrProgram {
        &self.ir
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

pub(crate) type HostSource = Box<dyn FnOnce() -> String>;

/// [`PendingRust::host_source`] over a linked program. Native code that names
/// no linked fn calls nothing back, and its program is not kept.
pub(crate) fn host_source(ir: &almide::ir::IrProgram, opts: &codegen::CodegenOptions, native_text: &str) -> HostSource {
    if !native_text.contains("almide_rt_") {
        return Box::new(String::new);
    }
    let (mut ir, opts) = (ir.clone(), opts.clone());
    Box::new(move || match codegen::codegen_with(&mut ir, codegen::pass::Target::Rust, &opts) {
        codegen::CodegenOutput::Source(s) => s,
        codegen::CodegenOutput::Binary(_) => String::new(),
    })
}

/// What the native test harness's v1 render made of a program.
pub(crate) enum TestRender {
    /// The certified render, its certificates judged.
    Rendered(String),
    /// A wall, and why: the program is built through the standard codegen.
    Walled(String),
}

/// Owner ruling 2026-11-27 (#4956): a test program the verified native render
/// walls is built through the standard codegen, as build and run do (and as
/// v0.69.1's test lane did), with their note under `-v`. Making those shapes
/// verified is the native-verified-fallback known limit.
pub(crate) fn note_test_wall(source: &str, why: &str) {
    cli::record_native_wall("test", source, why);
    if almide_base::env::flag("ALMIDE_VERBOSE") || almide_base::env::flag("ALMIDE_VERIFIED_DEBUG") {
        crate::err(&format!("note: verified native render walled — building via the standard codegen\n  reason: {why}"));
    }
}

pub(crate) fn render_native_tests(source: &str, native_text: String, host: HostSource, (ir, checker): NativeInput, fs_scope: Option<Vec<String>>) -> Result<TestRender, String> {
    let _callbacks = almide_mir::native_callbacks::Scope::enter(native_text).with_host_source(host);
    let (rust, certs) = match almide_mir::pipeline::try_render_rust_checked_tests_certified(source, ir, checker, fs_scope) {
        Ok(rendered) => rendered,
        Err(error) => return Ok(TestRender::Walled(error.to_string())),
    };
    // Owner ruling 2026-11-24 (#4932): the test harness ships its render's
    // certificates to the same linked verifier as build/run/bench; a
    // rejection is a compiler bug and the test binary is not built.
    almide::certificate_gate_native::judge_native(
        &format!("almide {}", env!("ALMIDE_VERSION_LINE")), "native v1 test render", &rust, &certs,
    ).map_err(|e| format!(
        "error[certificate-rejected]: the native v1 test render's own certificate does not hold, which is a compiler bug — please report it\n  {e}"
    ))?;
    if almide_base::env::flag("ALMIDE_VERIFIED_DEBUG") {
        crate::err("native: v1 trust-spine render (test harness)");
    }
    Ok(TestRender::Rendered(rust))
}

/// [`try_compile_reporting`] up to, not including, the codegen.
pub(crate) fn try_compile_pending(file: &str, no_check: bool, codegen_opts: &codegen::CodegenOptions) -> Result<(PendingRust, Option<almide::ir::IrProgram>, Vec<String>), String> {
    let t = cli::PhaseTimer::start();
    let (ir_program, outside_guarantee, native_input, source, package) = compile_to_linked_ir_with_source(file, no_check, &t)?;
    let metered = ir_program.as_ref().filter(|ir| almide::ir::runtime_use::uses_metered_prims(ir)).cloned();
    let ir = ir_program.expect("IR required for codegen");
    let native_text = cli::native_rust_text(file);
    let fs_scope = cli::fs_scope_for(file);
    Ok((PendingRust { ir, opts: codegen_opts.clone(), source, native_input, native_text, fs_scope, package }, metered, outside_guarantee))
}

