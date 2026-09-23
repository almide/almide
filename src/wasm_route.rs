//! The wasm ROUTING decision as a library entry (#2554): structural leg
//! first, the incumbent renderer when it declines, the reverse handover when
//! the incumbent walls a shape-routed program — ONE implementation that both
//! the CLI (`almide run/build/check --target wasm`, src/cli/build.rs) and a
//! library consumer call. The playground used to call the incumbent alone
//! (`almide_mir::pipeline::try_render_wasm_source`) and so walled programs
//! the CLI runs; the rules lived only in the CLI, and a second copy of them
//! would be a second place to remember (the #2397 bug class).
//!
//! The env probe switches (`ALMIDE_WASM_STRUCTURAL` / `ALMIDE_WASM_INCUMBENT`
//! / `ALMIDE_FUEL_PROBE` / `ALMIDE_COMPONENT_P3` / `ALMIDE_VERIFIED_DEBUG`)
//! are read by the CLI and arrive here as [`RouteOptions`] fields; this
//! module reads no environment, prints nothing, and exits nowhere. Every
//! refusal is a [`RouteError`] the caller renders (the CLI keeps its exact
//! diagnostics, E082 included); the debug narration goes through the
//! `trace` callback in the order the CLI always printed it.
//!
//! Builds for `wasm32-unknown-unknown`: nothing here touches the embedded
//! host (`almide-wasm-run`, wasmtime). The stock-WASI form a browser runner
//! needs comes from [`RoutedWasm::stock_wasi`] (`almide_wasi::to_wasi`).

use crate::ir::IrProgram;
pub use crate::wasm_leg::ModuleSource;

/// Which renderer produced the module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leg {
    /// `crates/almide-wasm`: typed IR → wasm bytes importing `almide.*`.
    Structural,
    /// `almide_mir::pipeline`: the v1 trust-spine WAT renderer (WASI-shaped).
    Incumbent,
}

impl Leg {
    /// The CLI's word for the leg in its `Built …` line and debug narration.
    pub fn name(self) -> &'static str {
        match self {
            Leg::Structural => "structural leg",
            Leg::Incumbent => "incumbent v1 leg",
        }
    }
}

/// The switches the route honours — the CLI fills them from the environment
/// ([`RouteOptions::from_env`]); a library consumer sets them explicitly.
#[derive(Clone, Copy, Debug, Default)]
pub struct RouteOptions {
    /// `almide build`'s LIBRARY form (#881): a main-less `pub fn` module is
    /// admitted (synthesized `_start`), and the structural leg's emitted host
    /// ops are audited against the stock-p1 service set (an unserved op
    /// reroutes to the incumbent's WASI rendering). `false` is the `run`
    /// form: `main` required, every host op served by the embedded host.
    pub library: bool,
    /// `ALMIDE_WASM_STRUCTURAL=1`: force the structural leg on every shape and
    /// turn its wall into a hard error instead of the reroute.
    pub force_structural: bool,
    /// `ALMIDE_WASM_INCUMBENT=1` / `ALMIDE_FUEL_PROBE=1`: force the incumbent
    /// renderer; final (no reverse handover).
    pub force_incumbent: bool,
    /// `ALMIDE_COMPONENT_P3=1`: the build ships through the p3 transform,
    /// whose shim carries the fs surface — the stock-p1 op audit is skipped.
    pub component_p3: bool,
    /// `ALMIDE_VERIFIED_DEBUG=1`: the incumbent's verbose per-function
    /// diagnostics, and the leg narration through `trace`.
    pub debug: bool,
}

impl RouteOptions {
    /// The CLI's reading of the probe switches — the ONE place the route's
    /// environment is read.
    pub fn from_env(library: bool) -> Self {
        let force_structural = almide_base::env::flag("ALMIDE_WASM_STRUCTURAL");
        RouteOptions {
            library,
            force_structural,
            // ALMIDE_FUEL_PROBE → incumbent: the charge-trace probe line is
            // that leg's Σ-probe instrumentation, so contract evidence keeps
            // its measured meaning (the structural leg's C-320 conformance
            // has its own gates in crates/almide-wasm).
            force_incumbent: almide_base::env::flag("ALMIDE_WASM_INCUMBENT")
                || almide_base::env::flag("ALMIDE_FUEL_PROBE"),
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
    /// `@export`-attributed fns must survive as wasm exports (the DCE-root
    /// contract); the structural leg has no export mode yet, so those
    /// modules stay on the incumbent leg.
    pub has_exports: bool,
    /// #1997: a `scoped { … }` block was outlined into a marked entry fn; its
    /// region is an obligation only the structural leg honours.
    pub declared_region: bool,
}

impl RouteInputs {
    /// The single rule for reading the three facts off a lowered program.
    pub fn of_ir(ir: &IrProgram) -> Self {
        RouteInputs {
            has_main: ir.functions.iter().any(|f| f.name.as_str() == "main"),
            has_exports: ir.functions.iter().any(|f| !f.export_attrs.is_empty()),
            declared_region: ir.functions.iter().any(|f| f.is_scoped_block_entry()),
        }
    }

    /// The fallback when the structural front could not lower the program
    /// (a parse, resolve or type failure): `main` and the export attributes
    /// are read off the declarations. `declared_region` is left `false` —
    /// the front's failure means the incumbent (which runs the same front)
    /// fails too, so no build is shipped either way and the region refusal
    /// has nothing to protect.
    fn of_ast(program: &crate::ast::Program) -> Self {
        let fns = program.decls.iter().filter_map(|d| match d {
            crate::ast::Decl::Fn { name, export_attrs, .. } => Some((name, export_attrs)),
            _ => None,
        });
        let mut inputs = RouteInputs { has_main: false, has_exports: false, declared_region: false };
        for (name, export_attrs) in fns {
            inputs.has_main |= name.as_str() == "main";
            inputs.has_exports |= !export_attrs.is_empty();
        }
        inputs
    }
}

/// A routed module.
#[derive(Clone, Debug)]
pub struct RoutedWasm {
    /// The module as the leg produced it: the structural leg's imports
    /// `almide.*` (the embedded host's surface); the incumbent's is already
    /// WASI-shaped, validated, its name section trimmed to function names.
    pub bytes: Vec<u8>,
    pub leg: Leg,
    /// The structural leg's emitted host-op set (empty for the incumbent):
    /// what `to_wasi` shims and what the p2/p3 transforms audit.
    pub host_ops: Vec<i32>,
}

impl RoutedWasm {
    pub fn structural(&self) -> bool {
        self.leg == Leg::Structural
    }

    /// The module in the form a STOCK WASI preview-1 runtime (wasmtime,
    /// wasmer, a browser p1 shim) loads: the structural leg's bytes through
    /// the `to_wasi` transform, the incumbent's as they are.
    pub fn stock_wasi(&self) -> Result<Vec<u8>, String> {
        match self.leg {
            Leg::Structural => almide_wasi::to_wasi(&self.bytes, &self.host_ops).map_err(|e| e.to_string()),
            Leg::Incumbent => Ok(self.bytes.clone()),
        }
    }
}

/// Why the incumbent renderer produced no module.
#[derive(Clone, Debug)]
pub enum IncumbentError {
    /// An honest wall (#782): the shape is outside the v1 subset.
    Lower(almide_mir::lower::LowerError),
    /// The renderer's WAT assembled but failed structural validation — an
    /// Almide bug, never shipped (`wat` assembles without full stack-shape
    /// validation; almide#1431's i32/i64 mismatch would otherwise surface
    /// as a wasmtime load error wearing the runner's vocabulary).
    InvalidWasm { message: String, offset: u64, site: String },
    /// The renderer produced WAT `wat` cannot parse — an Almide bug.
    UnparsableWat(String),
}

/// Why the route produced no module.
#[derive(Clone, Debug)]
pub enum RouteError {
    /// The entry program did not parse or resolve — neither leg can be
    /// attempted without its modules.
    Front(String),
    /// The program declares a `scoped` region and the route would hand it to
    /// the incumbent, which has no declared-region lowering (#1997): refused
    /// rather than shipped without the boundary. `why` names the path.
    RegionCannotReroute { why: String },
    /// Under `force_structural`, the structural leg walled (`why`) — the
    /// probe switch turns the reroute into a hard error.
    StructuralForcedWall { why: String },
    /// The incumbent renderer refused, and it was the final leg. When the
    /// structural leg was tried first and declined, `structural_wall` is
    /// its reason — BOTH legs refused (E082, #1690/#1922).
    Incumbent { error: IncumbentError, structural_wall: Option<String> },
    /// E083 (#1996): a compiler ownership defect on the structural leg is
    /// NOT rerouted around — the incumbent would ship a program the checked
    /// plan says leaks.
    OwnershipLowering(almide_wasm::OwnDefect),
}

/// Route one program: the structural leg first, the incumbent when it
/// declines, the reverse handover when the incumbent walls a shape-routed
/// program (#1423 bucket A). `inputs` are the routing facts the caller
/// already knows (the CLI's v0 IR); `None` reads them off the structural
/// front's own lowering. `trace` receives the debug narration lines (the
/// CLI prints them to stderr under `ALMIDE_VERIFIED_DEBUG`), in order.
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
    // The incumbent takes the FRESH (un-inferred) module list — captured
    // before the structural front mutates its copy in place.
    let incumbent_modules: Vec<(String, crate::ast::Program, bool)> =
        resolved.modules.iter().map(|(n, p, _pkg, s)| (n.clone(), p.clone(), *s)).collect();
    let ast_inputs = RouteInputs::of_ast(&program);
    let mut front = Front { file, source_text, program: Some(program), resolved: Some(resolved), lowered: None };

    let inputs = match inputs {
        Some(i) => i,
        None => match front.lower() {
            Ok(ir) => RouteInputs::of_ir(ir),
            Err(_) => ast_inputs,
        },
    };
    let RouteInputs { has_main, has_exports, declared_region } = inputs;
    let library_ok = opts.library;

    let incumbent = !opts.force_structural
        && (opts.force_incumbent || (!has_main && !library_ok) || has_exports);
    let render_incumbent = |trace: &mut dyn FnMut(&str)| -> Result<RoutedWasm, IncumbentError> {
        let bytes = incumbent_render(source_text, &incumbent_modules, library_ok, opts.debug)?;
        if opts.debug {
            trace(&format!("[almide] v1 trust-spine emitted the module ({} bytes)", bytes.len()));
        }
        Ok(RoutedWasm { bytes, leg: Leg::Incumbent, host_ops: Vec::new() })
    };

    if incumbent {
        if declared_region {
            return Err(RouteError::RegionCannotReroute {
                why: "the program was routed to the incumbent renderer (a forced route, a main-less non-library module, or an @export surface)".to_string(),
            });
        }
        let error = match render_incumbent(trace) {
            Ok(module) => return Ok(module),
            Err(error) => error,
        };
        // REVERSE handover (#1423 bucket A, the env.sleep_ms build shape): a
        // SHAPE-routed host-variant program the incumbent walls gets one
        // structural attempt — the same verified-to-verified doctrine as the
        // forward reroute below, in the other direction. The forced routes
        // and the main-less library form stay final on this reverse path;
        // ordinary library builds already use the structural library
        // emitter below.
        let shape_routed = !opts.force_incumbent && has_main && !has_exports;
        if shape_routed
            && let Ok(ir) = front.lower()
            && let Ok((bytes, host_ops)) = almide_wasm::emit_program_with_ops(ir)
            && wasmparser::validate(&bytes).is_ok()
            && (!library_ok || host_ops.iter().all(|op| almide_wasi::P1_SERVED_OPS.contains(op)))
        {
            if opts.debug {
                trace(&format!("[almide] incumbent walled — structural leg took the build ({} bytes)", bytes.len()));
            }
            return Ok(RoutedWasm { bytes, leg: Leg::Structural, host_ops: host_ops.iter().copied().collect() });
        }
        return Err(RouteError::Incumbent { error, structural_wall: None });
    }

    // A structural WALL routes to the incumbent renderer. This is NOT the
    // retired v0 fallback (#782's sin was falling into UNVERIFIED codegen):
    // both legs here are verified renderers, so the product never regresses
    // on a shape only the incumbent lowers — and a program NEITHER leg can
    // lower still fails, carrying the incumbent's rich wall and the
    // structural reason (E082 at the CLI).
    let reroute = |why: String, trace: &mut dyn FnMut(&str)| -> Result<RoutedWasm, RouteError> {
        if opts.force_structural {
            return Err(RouteError::StructuralForcedWall { why });
        }
        if declared_region {
            return Err(RouteError::RegionCannotReroute { why: format!("the structural leg declined it ({why})") });
        }
        if opts.debug {
            trace(&format!("[almide] structural leg declined ({why}) — incumbent renderer"));
        }
        render_incumbent(trace).map_err(|error| RouteError::Incumbent { error, structural_wall: Some(why) })
    };
    let ir = match front.lower() {
        Ok(ir) => ir,
        Err(e) => return reroute(format!("front: {e}"), trace),
    };
    let emitted = if has_main || !library_ok {
        almide_wasm::emit_program_with_ops(ir)
    } else {
        almide_wasm::emit_library_with_ops(ir)
    };
    match emitted {
        Ok((bytes, host_ops)) => {
            // Same emit-time validation discipline as the incumbent leg:
            // never ship bytes wasmtime would refuse at load.
            if let Err(e) = wasmparser::validate(&bytes) {
                return reroute(format!("validation: {}", e.message()), trace);
            }
            // BUILD artifacts run on stock runtimes through to_wasi: an
            // emitted host op the p1 shim cannot serve would be a RUNTIME
            // refusal there (the env.set lesson) — refuse at build time,
            // through the same reroute (the incumbent may serve the fn with
            // real WASI imports; if not, its wall names the reason). Not
            // under force_structural: the force switch probes the EMITTER
            // frontier (its contract is walls-as-hard-errors on lowering,
            // not stock service); the audit gates what the DEFAULT path
            // ships. Not under component_p3 either: the p3-requested build
            // ships through `to_p3`, whose shim carries the fs surface the
            // p1 set does not — an op the p3 transform cannot map still
            // fails loudly there.
            if library_ok
                && !opts.force_structural
                && !opts.component_p3
                && let Some(op) = host_ops.iter().find(|op| !almide_wasi::P1_SERVED_OPS.contains(op))
            {
                return reroute(
                    format!("host op {op} has no stock-WASI service (the embedded host — `almide run --target wasm` — serves it)"),
                    trace,
                );
            }
            // With the debug env, ALWAYS name the winning leg — the
            // incumbent path and the reroute already speak, so a silent
            // structural success made the env an incomplete oracle.
            if opts.debug {
                trace(&format!("[almide] structural leg emitted the module ({} bytes)", bytes.len()));
            }
            Ok(RoutedWasm { bytes, leg: Leg::Structural, host_ops: host_ops.iter().copied().collect() })
        }
        Err(almide_wasm::EmitError::Unsupported(reason)) => reroute(reason, trace),
        Err(almide_wasm::EmitError::OwnershipLowering(d)) => Err(RouteError::OwnershipLowering(d)),
    }
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

/// The structural front, lowered at most once per route: the forward path
/// and the reverse handover never both run, and a caller without inputs
/// lowers first and hands the same IR to the forward path.
struct Front<'a> {
    file: &'a str,
    source_text: &'a str,
    program: Option<crate::ast::Program>,
    resolved: Option<crate::resolve::ResolvedModules>,
    lowered: Option<Result<IrProgram, String>>,
}

impl Front<'_> {
    fn lower(&mut self) -> Result<&IrProgram, String> {
        if self.lowered.is_none() {
            let program = self.program.take().expect("the front lowers once");
            let resolved = self.resolved.take().expect("the front lowers once");
            self.lowered = Some(crate::wasm_leg::lower_resolved(self.file, self.source_text, program, resolved, None));
        }
        match self.lowered.as_ref().expect("just set") {
            Ok(ir) => Ok(ir),
            Err(e) => Err(e.clone()),
        }
    }
}

/// The incumbent renderer, run to validated bytes: WAT → module →
/// `wasmparser::validate` → name section trimmed to function names.
fn incumbent_render(
    source_text: &str,
    self_modules: &[(String, crate::ast::Program, bool)],
    library_ok: bool,
    verbose: bool,
) -> Result<Vec<u8>, IncumbentError> {
    // `almide build` may produce a main-less LIBRARY module (pub-fn exports,
    // synthesized empty `_start` — #881); `almide run` must keep the no-main
    // wall so the wasm leg fails exactly where native compilation does.
    let render = if library_ok {
        almide_mir::pipeline::try_render_wasm_source_library
    } else {
        almide_mir::pipeline::try_render_wasm_source
    };
    let wat = render(source_text, self_modules, verbose).map_err(IncumbentError::Lower)?;
    let bytes = wat::parse_str(&wat).map_err(|e| IncumbentError::UnparsableWat(e.to_string()))?;
    // Unconditional emit-time validation (the grain pattern: Binaryen
    // `Module.validate` or die), before the name section is stripped, so
    // the wall can name the offending function.
    if let Err(e) = wasmparser::validate(&bytes) {
        let site = wasm_function_at(&bytes, e.offset()).unwrap_or_else(|| "an unidentified function".to_string());
        return Err(IncumbentError::InvalidWasm { message: e.message().to_string(), offset: e.offset(), site });
    }
    Ok(strip_wasm_name_section(bytes))
}

/// Best-effort map of a validation-error byte offset to the function that
/// contains it: which code-section body covers the offset, named through
/// the name section (still present — validation runs before
/// [`strip_wasm_name_section`]). `None` only if the module is too broken
/// to walk section-by-section; the caller still reports the offset.
fn wasm_function_at(bytes: &[u8], offset: u64) -> Option<String> {
    use wasmparser::{KnownCustom, Name, Parser, Payload, TypeRef};
    let mut imported_funcs: u32 = 0;
    let mut code_index: u32 = 0;
    let mut hit: Option<u32> = None;
    let mut names: Vec<(u32, String)> = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.ok()? {
            Payload::ImportSection(imports) => {
                // 0.255 groups imports by module; each group yields
                // `(byte_offset, Import)` items.
                for group in imports.into_iter().flatten() {
                    for (_, imp) in group.into_iter().flatten() {
                        if matches!(imp.ty, TypeRef::Func(_)) {
                            imported_funcs += 1;
                        }
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                if body.range().contains(&offset) {
                    hit = Some(imported_funcs + code_index);
                }
                code_index += 1;
            }
            Payload::CustomSection(c) => {
                if let KnownCustom::Name(name_reader) = c.as_known() {
                    for sub in name_reader.into_iter().flatten() {
                        if let Name::Function(map) = sub {
                            for naming in map.into_iter().flatten() {
                                names.push((naming.index, naming.name.to_string()));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let idx = hit?;
    Some(match names.iter().find(|(i, _)| *i == idx) {
        Some((_, name)) => format!("function `{name}` (index {idx})"),
        None => format!("function index {idx}"),
    })
}

/// Trim every "name"-id custom section down to its function-names
/// subsection, dropping local-names and any other subsection.
///
/// `wat::parse_str` always emits a name section recording every symbolic
/// `$name` the WAT source used — functions AND every per-function local
/// (`$v1`, `$v2`, ...). `docs/wasm/WASM-OUTPUT.md` commits to keeping function
/// names because they're what a wasmtime trap backtrace prints
/// (`<unknown>!funcname`) — the one piece of this metadata with real
/// diagnostic value. Local names carry none (wasmtime backtraces never
/// print them) and dominate the section's size: measured on
/// `closure.almd`, 251 named locals cost 1.6KB versus keeping only the 20
/// function names. The wasm spec defines custom sections — and every
/// subsection within the "name" section — as ignorable by any consumer
/// that doesn't recognize them (§2.5.9), so dropping subsections can never
/// change what the module computes; this is exactly as safe as the
/// preamble reachability DCE (`render_wasm_dce.rs`), just one level lower:
/// a format-legal removal, not a black-box "optimization".
fn strip_wasm_name_section(bytes: Vec<u8>) -> Vec<u8> {
    const HEADER_LEN: usize = 8; // b"\0asm" + version u32
    if bytes.len() < HEADER_LEN {
        return bytes;
    }
    let mut out = bytes[..HEADER_LEN].to_vec();
    let mut i = HEADER_LEN;
    while i < bytes.len() {
        let id = bytes[i];
        let Some((payload_len, len_bytes)) = read_leb128_u32(&bytes[i + 1..]) else {
            // Malformed length — bail out and keep everything from here on
            // verbatim rather than risk corrupting the module.
            out.extend_from_slice(&bytes[i..]);
            return out;
        };
        let payload_start = i + 1 + len_bytes;
        let payload_end = (payload_start + payload_len as usize).min(bytes.len());
        let is_name_section = id == 0 && custom_section_name(&bytes[payload_start..payload_end]) == Some("name");
        if is_name_section {
            if let Some(trimmed) = trim_name_section_to_function_names(&bytes[payload_start..payload_end]) {
                out.push(0);
                out.extend_from_slice(&write_leb128_u32(trimmed.len() as u32));
                out.extend_from_slice(&trimmed);
            }
            // Malformed name-section payload: drop it whole rather than risk
            // shipping a corrupt custom section — still format-legal (the
            // section is optional metadata, never load-bearing).
        } else {
            out.extend_from_slice(&bytes[i..payload_end]);
        }
        i = payload_end;
    }
    out
}

/// A custom section's payload starts with its own length-prefixed name string.
fn custom_section_name(payload: &[u8]) -> Option<&str> {
    let (name_len, len_bytes) = read_leb128_u32(payload)?;
    let name_bytes = payload.get(len_bytes..len_bytes + name_len as usize)?;
    std::str::from_utf8(name_bytes).ok()
}

/// A "name" custom section's payload is its own length-prefixed "name"
/// identifier string, followed by a sequence of subsections (id byte +
/// LEB128 length + payload) — id 1 is function names, the only one kept.
/// Returns `None` if the payload is too short to even contain the leading
/// identifier string (malformed).
fn trim_name_section_to_function_names(payload: &[u8]) -> Option<Vec<u8>> {
    let (name_len, len_bytes) = read_leb128_u32(payload)?;
    let prefix_end = len_bytes + name_len as usize;
    if prefix_end > payload.len() {
        return None;
    }
    let mut out = payload[..prefix_end].to_vec();
    let mut i = prefix_end;
    while i < payload.len() {
        let id = payload[i];
        let Some((sub_len, sub_len_bytes)) = read_leb128_u32(&payload[i + 1..]) else {
            return None;
        };
        let sub_start = i + 1 + sub_len_bytes;
        let sub_end = (sub_start + sub_len as usize).min(payload.len());
        if id == 1 {
            out.extend_from_slice(&payload[i..sub_end]);
        }
        i = sub_end;
    }
    Some(out)
}

/// Decode an unsigned LEB128 `u32` at the start of `bytes`. Returns the
/// decoded value and how many bytes it occupied, or `None` on overflow /
/// truncated input.
fn read_leb128_u32(bytes: &[u8]) -> Option<(u32, usize)> {
    let mut result: u32 = 0;
    let mut shift = 0u32;
    for (i, &byte) in bytes.iter().enumerate() {
        result |= ((byte & 0x7f) as u32).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some((result, i + 1));
        }
        shift += 7;
        if shift >= 32 {
            return None;
        }
    }
    None
}

/// Encode a `u32` as unsigned LEB128.
fn write_leb128_u32(mut v: u32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}
