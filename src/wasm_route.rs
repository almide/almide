//! The wasm ROUTING decision as a library entry (#2554): ONE implementation
//! that both the CLI (`almide run/build/check --target wasm`,
//! src/cli/build.rs) and a library consumer (the playground) call, so the
//! rules never live in two places (the #2397 bug class).
//!
//! Since #2752 there is one leg: the structural emitter (`crates/almide-wasm`).
//! A program it declines is a WALL — a hard, named error ([`RouteError::Wall`],
//! E082 at the CLI) — never a silent hand-over to another renderer. The
//! incumbent WAT renderer this module used to fall back to is no longer
//! reachable from any route (#2739, #1696 step 5).
//!
//! The env probe switches (`ALMIDE_WASM_STRUCTURAL` / `ALMIDE_COMPONENT_P3`
//! / `ALMIDE_VERIFIED_DEBUG`) are read by the CLI and arrive here as
//! [`RouteOptions`] fields; this module reads no environment, prints
//! nothing, and exits nowhere. Every refusal is a [`RouteError`] the caller
//! renders; the debug narration goes through the `trace` callback.
//!
//! Builds for `wasm32-unknown-unknown`: nothing here touches the embedded
//! host (`almide-wasm-run`, wasmtime). The stock-WASI form a browser runner
//! needs comes from [`RoutedWasm::stock_wasi`] (`almide_wasi::to_wasi`).

use crate::ir::IrProgram;
pub use crate::wasm_leg::ModuleSource;

/// The switches the route honours — the CLI fills them from the environment
/// ([`RouteOptions::from_env`]); a library consumer sets them explicitly.
#[derive(Clone, Copy, Debug, Default)]
pub struct RouteOptions {
    /// `almide build`'s LIBRARY form (#881): a main-less `pub fn` module is
    /// admitted, and the emitted host ops are audited against the stock-p1
    /// service set (an unserved op is a wall: the artifact would be refused
    /// by a stock runtime). `false` is the `run` form: `main` required,
    /// every host op served by the embedded host.
    pub library: bool,
    /// `ALMIDE_WASM_STRUCTURAL=1`: skip the stock-p1 op audit — the probe
    /// switch measures the EMITTER frontier, not stock service.
    pub force_structural: bool,
    /// `ALMIDE_COMPONENT_P3=1`: the build ships through the p3 transform,
    /// whose shim carries the fs surface — the stock-p1 op audit is skipped.
    pub component_p3: bool,
    /// `ALMIDE_VERIFIED_DEBUG=1`: the route narration through `trace`.
    pub debug: bool,
}

impl RouteOptions {
    /// The CLI's reading of the probe switches — the ONE place the route's
    /// environment is read.
    pub fn from_env(library: bool) -> Self {
        RouteOptions {
            library,
            force_structural: almide_base::env::flag("ALMIDE_WASM_STRUCTURAL"),
            component_p3: almide_base::env::flag("ALMIDE_COMPONENT_P3"),
            debug: almide_base::env::flag("ALMIDE_VERIFIED_DEBUG"),
        }
    }
}

/// The program facts the route decides on — read from the IR, never from a
/// failure. The CLI reads them from the v0 IR its own gates already built;
/// [`route_wasm`] reads them from the structural front's IR when the caller
/// has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteInputs {
    pub has_main: bool,
}

impl RouteInputs {
    /// The single rule for reading the routing facts off a lowered program.
    pub fn of_ir(ir: &IrProgram) -> Self {
        RouteInputs { has_main: ir.functions.iter().any(|f| f.name.as_str() == "main") }
    }

    /// The fallback when the structural front could not lower the program
    /// (a parse, resolve or type failure): `main` is read off the
    /// declarations.
    fn of_ast(program: &crate::ast::Program) -> Self {
        let has_main = program
            .decls
            .iter()
            .any(|d| matches!(d, crate::ast::Decl::Fn { name, .. } if name.as_str() == "main"));
        RouteInputs { has_main }
    }
}

/// A routed module: the structural leg's bytes, importing `almide.*` (the
/// embedded host's surface).
#[derive(Clone, Debug)]
pub struct RoutedWasm {
    pub bytes: Vec<u8>,
    /// The emitted host-op set: what `to_wasi` shims and what the p2/p3
    /// transforms audit.
    pub host_ops: Vec<i32>,
}

impl RoutedWasm {
    /// The module in the form a STOCK WASI preview-1 runtime (wasmtime,
    /// wasmer, a browser p1 shim) loads: the bytes through the `to_wasi`
    /// transform.
    pub fn stock_wasi(&self) -> Result<Vec<u8>, String> {
        almide_wasi::to_wasi(&self.bytes, &self.host_ops).map_err(|e| e.to_string())
    }
}

/// Why the route produced no module.
#[derive(Clone, Debug)]
pub enum RouteError {
    /// The entry program did not parse or resolve.
    Front(String),
    /// The `run` form on a program without `main`: there is nothing to run.
    /// (The library form, `almide build`, admits a main-less module.)
    NoMain,
    /// The structural leg declined the program (`why`, a census key without
    /// a location — `almide_wasm::decline_site` carries the site). There is
    /// no second leg: this is the wasm verdict (E082 at the CLI).
    Wall { why: String },
    /// E083 (#1996): a compiler ownership defect on the structural leg.
    OwnershipLowering(almide_wasm::OwnDefect),
}

/// Route one program through the structural leg. `inputs` are the routing
/// facts the caller already knows (the CLI's v0 IR); `None` reads them off
/// the structural front's own lowering. `trace` receives the debug
/// narration lines (the CLI prints them to stderr under
/// `ALMIDE_VERIFIED_DEBUG`), in order.
pub fn route_wasm(
    file: &str,
    source_text: &str,
    modules: ModuleSource<'_>,
    inputs: Option<RouteInputs>,
    opts: RouteOptions,
    trace: &mut dyn FnMut(&str),
) -> Result<RoutedWasm, RouteError> {
    let program = crate::wasm_leg::parse_entry(file, source_text).map_err(RouteError::Front)?;
    let resolved = crate::wasm_leg::resolve_modules(file, &program, modules).map_err(RouteError::Front)?;
    let ast_inputs = RouteInputs::of_ast(&program);
    let lowered = crate::wasm_leg::lower_resolved(file, source_text, program, resolved, None);
    let RouteInputs { has_main } = match (inputs, &lowered) {
        (Some(i), _) => i,
        (None, Ok(ir)) => RouteInputs::of_ir(ir),
        (None, Err(_)) => ast_inputs,
    };
    let library = opts.library;
    // `almide run --target wasm` on a main-less file: nothing to run, and
    // no leg is needed to say so (#2752).
    if !has_main && !library {
        return Err(RouteError::NoMain);
    }
    let wall = |why: String, trace: &mut dyn FnMut(&str)| -> RouteError {
        if opts.debug {
            trace(&format!("[almide] structural leg declined ({why})"));
        }
        RouteError::Wall { why }
    };
    let ir = match &lowered {
        Ok(ir) => ir,
        Err(e) => return Err(wall(format!("front: {e}"), trace)),
    };
    let emitted = if has_main || !library {
        almide_wasm::emit_program_with_ops(ir)
    } else {
        almide_wasm::emit_library_with_ops(ir)
    };
    let (bytes, host_ops) = match emitted {
        Ok(x) => x,
        Err(almide_wasm::EmitError::Unsupported(reason)) => return Err(wall(reason, trace)),
        Err(almide_wasm::EmitError::OwnershipLowering(d)) => return Err(RouteError::OwnershipLowering(d)),
    };
    // Emit-time validation: never ship bytes wasmtime would refuse at load.
    if let Err(e) = wasmparser::validate(&bytes) {
        return Err(wall(format!("validation: {}", e.message()), trace));
    }
    // BUILD artifacts run on stock runtimes through to_wasi: an emitted host
    // op the p1 shim cannot serve would be a RUNTIME refusal there (the
    // env.set lesson) — refuse at build time. Not under force_structural (the
    // switch probes the emitter frontier, not stock service), and not under
    // component_p3 (the p3 transform's shim carries the fs surface the p1 set
    // does not; an op it cannot map still fails loudly there).
    if library
        && !opts.force_structural
        && !opts.component_p3
        && let Some(op) = host_ops.iter().find(|op| !almide_wasi::P1_SERVED_OPS.contains(op))
    {
        return Err(wall(
            format!("host op {op} has no stock-WASI service (the embedded host — `almide run --target wasm` — serves it)"),
            trace,
        ));
    }
    if opts.debug {
        trace(&format!("[almide] structural leg emitted the module ({} bytes)", bytes.len()));
    }
    Ok(RoutedWasm { bytes, host_ops: host_ops.iter().copied().collect() })
}

/// The library form: [`route_wasm`] with the routing facts read off the
/// program itself and no narration — what a consumer without the CLI's v0
/// front (the playground) calls.
pub fn render_wasm_routed(
    file: &str,
    source_text: &str,
    modules: ModuleSource<'_>,
    opts: RouteOptions,
) -> Result<RoutedWasm, RouteError> {
    route_wasm(file, source_text, modules, None, opts, &mut |_| {})
}
