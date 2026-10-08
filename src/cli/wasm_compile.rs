//! The direct-WASM compile pipeline shared by `almide build`, `run`, `check`,
//! `bench` and `verify` with `--target wasm`: parse and resolve, type-check,
//! lower and link, the availability gates, and the routed render.

use crate::{parse_file, canonicalize, check, diagnostic, resolve, project, project_fetch, err};

/// Type-check and lower every user module import resolution found, in
/// resolution order, appending each one's IR directly onto `ir_program`;
/// returns the modules' own diagnostics. Shared by `compile_to_wasm_bytes`
/// and the test lane's wasm leg. (Sibling of
/// `compile_driver::lower_user_modules`, which additionally tracks a
/// per-module `module_irs` map the WASM path doesn't need.)
pub(super) fn lower_wasm_modules(
    checker: &mut check::Checker,
    resolved: &mut resolve::ResolvedModules,
    ir_program: &mut almide::ir::IrProgram,
) -> Vec<(String, String, Vec<crate::diagnostic::Diagnostic>)> {
    let mut module_diags = Vec::new();
    let sources = std::mem::take(&mut resolved.sources);
    for module in &mut resolved.modules {
        lower_one_wasm_module(checker, module, ir_program, &sources, &mut module_diags);
    }
    resolved.sources = sources;
    module_diags
}

/// Type-check and lower one user module for the WASM path, appending its IR
/// directly onto `ir_program` — same checker/env mutation order as the loop
/// body it was.
fn lower_one_wasm_module(
    checker: &mut check::Checker,
    module: &mut (String, almide::ast::Program, Option<project::PkgId>, bool),
    ir_program: &mut almide::ir::IrProgram,
    sources: &std::collections::HashMap<String, (String, String)>,
    module_diags: &mut Vec<(String, String, Vec<crate::diagnostic::Diagnostic>)>,
) {
    let (name, mod_prog, pkg_id, _) = module;
    if almide::stdlib::is_stdlib_module(name) && !almide::stdlib::is_bundled_module(name) { return; }
    let saved_self = checker.env.self_module_name;
    if let Some(pid) = pkg_id.as_ref() {
        checker.env.self_module_name = Some(almide::intern::sym(&pid.name));
    }
    crate::compile_driver::infer_module_capturing(checker, name, mod_prog, sources, module_diags);
    let versioned = pkg_id.as_ref().map(|pid| {
        let base = pid.mod_name();
        if let Some(suffix) = name.strip_prefix(&pid.name) {
            format!("{}{}", base, suffix)
        } else {
            base
        }
    });
    if let Some(ref v) = versioned {
        checker.env.module_versioned_names.insert(almide::intern::sym(name), almide::intern::sym(v));
    }
    let self_name = checker.env.self_module_name.map(|s| s.to_string());
    let import_table_name = self_name.as_deref().unwrap_or(name);
    let (mod_table, _) = almide::import_table::build_import_table(mod_prog, Some(import_table_name), &checker.env.user_modules);
    let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
    let mod_ir_module = almide::lower::lower_module(name, mod_prog, &checker.env, &checker.type_map, versioned);
    checker.env.import_table = saved_table;
    checker.env.self_module_name = saved_self;
    ir_program.modules.push(mod_ir_module);
}

/// `compile_to_wasm_bytes`'s parse + dependency-fetch + import-resolution
/// phase. Extracted verbatim — prints diagnostics and returns `Err(())` on
/// any parse/fetch/resolve failure, mirroring the original early returns.
/// Also `almide verify`'s front half (#2152): the certificate producer lowers
/// the same resolved module set the wasm leg renders.
#[allow(clippy::type_complexity)]
pub(crate) fn parse_and_resolve_wasm(file: &str) -> Result<(almide::ast::Program, String, resolve::ResolvedModules, Vec<(project::PkgId, std::path::PathBuf)>), ()> {
    let (program, source_text, parse_errors) = parse_file(file);

    if !parse_errors.is_empty() {
        for e in &parse_errors {
            err(&format!("{}", crate::diagnostic_render::display_with_source(e, &source_text)));
        }
        return Err(());
    }

    // Resolve dependencies
    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            match project_fetch::fetch_all_deps(&proj) {
                Ok(deps) => deps.into_iter().map(|fd| (fd.pkg_id, fd.source_dir)).collect(),
                Err(e) => { err(&format!("{}", e)); return Err(()); }
            }
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let resolved = match resolve::resolve_imports_with_deps(file, &program, &dep_paths) {
        Ok(r) => r,
        Err(e) => { err(&format!("{}", e)); return Err(()); }
    };

    Ok((program, source_text, resolved, dep_paths))
}

/// `compile_to_wasm_bytes`'s type-check phase: canonicalize, build the
/// `Checker`, refresh module top-let types (#785), and infer the entry
/// program. Extracted verbatim — prints diagnostics and returns `Err(())`
/// on any type error.
fn typecheck_wasm_program(file: &str, source_text: &str, program: &mut almide::ast::Program, resolved: &resolve::ResolvedModules) -> Result<check::Checker, ()> {
    let canon = canonicalize::canonicalize_program(
        program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = check::Checker::from_env(canon.env);
    checker.set_source(file, source_text);
    checker.diagnostics = canon.diagnostics;
    // #785: module top-let types must be fully inferred before the entry
    // program reads them (drivers infer the entry FIRST; without this the
    // readers see the registration seed — Unknown for non-literal inits).
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(program);
    if diagnostics.iter().any(|d| d.level == diagnostic::Level::Error) {
        for d in &diagnostics {
            err(&format!("{}", crate::diagnostic_render::display_with_source(d, source_text)));
        }
        return Err(());
    }
    // Warnings reach this leg too (#1911): the native driver prints every
    // checker warning before the program runs (compile_driver), and
    // `run --target wasm` printed none — an E052 deprecation a program
    // carried was visible on one target and silent on the other.
    if !crate::warnings_suppressed() {
        for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
            err(&format!("{}", crate::diagnostic_render::display_with_source(d, source_text)));
        }
    }
    Ok(checker)
}

/// `compile_to_wasm_bytes`'s IR construction phase: pre-register versioned
/// module names, lower the entry program, lower each resolved user module
/// (bundled stdlib included so `@inline_rust` fns reach the bundled-dispatch
/// path), link, optimize, and monomorphize. Extracted verbatim.
fn lower_and_link_wasm_ir(program: &almide::ast::Program, checker: &mut check::Checker, resolved: &mut resolve::ResolvedModules) -> Result<almide::ir::IrProgram, ()> {
    // Pre-register versioned names before root lowering
    almide::wasm_leg::register_versioned_module_names(checker, &resolved.modules);
    let mut ir_program = almide::lower::lower_program(program, &checker.env, &checker.type_map);

    // Lower user modules to IR. Bundled stdlib modules (stdlib/<m>.almd) are
    // included so their fns can be invoked through the bundled-dispatch path;
    // colliding TOML-runtime fns are pruned to avoid duplicate definitions.
    let module_diags = lower_wasm_modules(checker, resolved, &mut ir_program);
    // An imported module's own type errors abort the wasm build too (#862).
    crate::compile_driver::report_module_diagnostics(&module_diags).map_err(|_| ())?;

    // The ONE driver (crates/almide-driver). This site used to spell the order itself, and
    // spelled it DIFFERENTLY from the shipped wasm path: ir_link FIRST here, ir_link LAST in
    // `almide_mir::pipeline`. Both were green, so the cross-target equivalence claim was
    // resting on "the position of ir_link never matters" rather than on a shared driver
    // (#925, and #785 is a recorded bug from exactly that divergence).
    //
    // #3275: through the build routes' shared halves, so the `[permissions]`
    // gate judges this route exactly as it does the native build, on the same
    // post-optimize, pre-mono IR. This route's integrity check is
    // `verify_wasm_ir`, after the link.
    let proj = crate::compile_driver::cwd_project();
    crate::compile_driver::optimize_gate_and_link(&mut ir_program, proj.as_ref(), |_| Ok(())).map_err(|_| ())?;

    Ok(ir_program)
}

/// `compile_to_wasm_bytes`'s IR-integrity gate — the same check the native
/// path (main.rs) enforces. Without this an invalid IR (e.g. an unresolved
/// closure-call result type) is emitted as a structurally-broken module that
/// `almide build` reports as success (rc 0) but wasmtime refuses to load.
/// Extracted verbatim.
fn verify_wasm_ir(ir_program: &almide::ir::IrProgram) -> Result<(), ()> {
    let verify_errors = almide::ir::verify_program(ir_program);
    if !verify_errors.is_empty() {
        for e in &verify_errors {
            err(&format!("internal compiler error: {}", e));
        }
        err(&format!("{} IR verification error(s) — no WASM emitted", verify_errors.len()));
        return Err(());
    }
    Ok(())
}

/// `compile_to_wasm_bytes`'s native-only-matrix-op guard: native-only matrix
/// ops (e.g. qwen3_block_q1_0_kv: a packed-GGUF block with no primitive
/// decomposition) have no WASM lowering. Reject at build time with a clear
/// message rather than letting the emitter ICE deep in codegen. Extracted
/// verbatim.
/// #1423 stage 3 — the check-time availability diagnostic. The declared
/// BOTH-LEGS wall set (proofs/target-availability.toml, measured with
/// default routing and gated four-directionally by
/// scripts/check-target-availability.sh) turns the late render wall into
/// an E081 at check time, naming the reason and — where one exists — the
/// portable alternative. The render wall stays as the backstop.
fn check_wasm_availability(
    ir_program: &almide::ir::IrProgram,
    package: &std::collections::HashSet<String>,
    embedded_leg: bool,
    serve_shape: &Result<(), String>,
) -> Result<bool, ()> {
    // The measurement escape: the availability PROBE builds through this
    // binary to measure the ground truth the table declares — with the
    // check armed it would measure its own declaration (circular).
    if almide_base::env::flag("ALMIDE_NO_AVAIL_CHECK") {
        return Ok(!embedded_leg && serve_shape.is_ok() && reaches_http_serve(ir_program));
    }
    use std::collections::BTreeMap;
    use std::sync::OnceLock;
    type Row = (String, Option<String>, Option<String>, Option<String>, Option<String>);
    static UNAVAILABLE: OnceLock<BTreeMap<String, Row>> = OnceLock::new();
    let table = UNAVAILABLE.get_or_init(|| {
        let toml = include_str!("../../proofs/target-availability.toml");
        let mut out = BTreeMap::new();
        // Schema 2 (#1710 increment 2): one `[[unavailable]]` row per fn
        // with a per-leg `legs = [..]` list. E081 is a PER-LEG verdict
        // (#1710 increment 3): the build path walls on the stock-p1 leg,
        // the run/bench path (the embedded host) walls on the embedded
        // leg — a stock wall alone no longer refuses the run route the
        // row's own reason says is served. Rows keep their raw legs list
        // and both per-leg reasons; the caller filters. Line-anchored
        // block split, as before (a substring split once ate the first
        // row through the header comment).
        for block in toml.split("\n[[unavailable]]\n").skip(1) {
            let field = |k: &str| {
                block.lines().find_map(|l| {
                    l.strip_prefix(&format!("{k} = \"")).and_then(|r| r.strip_suffix('"')).map(str::to_string)
                })
            };
            let legs = block
                .lines()
                .find_map(|l| l.strip_prefix("legs = ["))
                .unwrap_or("")
                .to_string();
            // A host-capability row (#2589, ADR-0025) is not a build wall:
            // the artifact ships with the capability's import and a host
            // without it refuses at load.
            if field("class").as_deref() == Some("host-capability") {
                continue;
            }
            if let Some(fn_name) = field("fn") {
                out.insert(
                    fn_name,
                    (legs, field("reason-stock-p1"), field("reason-embedded"), field("reason"), field("alt")),
                );
            }
        }
        out
    });
    let leg_lit = if embedded_leg { "\"embedded\"" } else { "\"stock-p1\"" };
    let mut hits: BTreeMap<String, &Row> = BTreeMap::new();
    use almide::ir::visit::IrVisitor;
    // #2865: a call into a module the package's own source declares is the
    // package's fn, whatever its key spells — `src/process.almd`'s `exec` is
    // not the stdlib's `process.exec`, so the stdlib's row does not bar it.
    let own: std::collections::HashSet<String> = ir_program
        .modules
        .iter()
        .filter(|m| package.contains(m.name.as_str()))
        .flat_map(|m| m.functions.iter().map(move |f| format!("{}.{}", m.name.as_str(), f.name.as_str())))
        .collect();
    struct Scan<'a> {
        table: &'a BTreeMap<String, Row>,
        own: &'a std::collections::HashSet<String>,
        leg_lit: &'static str,
        hits: BTreeMap<String, &'a Row>,
        /// A reachable `http.serve` call (#2659), row or no row.
        serves: bool,
    }
    impl<'a> IrVisitor for Scan<'a> {
        fn visit_expr(&mut self, e: &almide::ir::IrExpr) {
            if let almide::ir::IrExprKind::Call {
                target: almide::ir::CallTarget::Module { module, func, .. }, ..
            } = &e.kind
            {
                let key = format!("{}.{}", module.as_str(), func.as_str());
                self.serves |= key == "http.serve" && !self.own.contains(&key);
                if let Some(row) = self.table.get(&key)
                    && row.0.contains(self.leg_lit)
                    && !self.own.contains(&key)
                {
                    self.hits.entry(key).or_insert(row);
                }
            }
            almide::ir::visit::walk_expr(self, e);
        }
    }
    let mut scan = Scan { table, own: &own, leg_lit, hits: BTreeMap::new(), serves: false };
    // Only REACHABLE bodies are scanned — the same reachability the wasm
    // emitter prunes by (`reachability::reachable_fn_names`), so the
    // check-time diagnostic and the emit agree: a call the emitter never
    // lowers cannot fail the build (#644's pin, kept when the ledger grew
    // to the whole public surface in #1827/#1831). The render wall stays
    // the backstop for anything reachability lets through.
    let reachable = almide::codegen::reachability::reachable_fn_names(ir_program);
    let is_reachable = |module: Option<&str>, name: &str| {
        almide::codegen::reachability::registered_keys(module, name).iter().any(|k| reachable.contains(k))
    };
    for f in ir_program.functions.iter().filter(|f| is_reachable(None, f.name.as_str())) {
        scan.visit_expr(&f.body);
    }
    for m in &ir_program.modules {
        let mname = m.name.to_string();
        for f in m.functions.iter().filter(|f| is_reachable(Some(&mname), f.name.as_str())) {
            scan.visit_expr(&f.body);
        }
    }
    hits.extend(scan.hits);
    let serves = scan.serves;
    // The p3 component serves the http string family (#1710 PR B): under
    // ALMIDE_COMPONENT_P3 the ops-43..=50 fns ship through the to_p3 http
    // shim, so their stock-p1 rows do not bar THIS build path — the same
    // predicate that flips fs routing structural for p3.
    if almide_base::env::flag("ALMIDE_COMPONENT_P3") {
        for k in [
            "http.get",
            "http.post",
            "http.put",
            "http.patch",
            "http.delete",
            // The framed family rides the same shim (ops 48..=50, #1710).
            "http.request",
            "http.request_status",
            "http.get_status",
            "http.request_bytes",
            "http.get_bytes",
        ] {
            hits.remove(k);
        }
    }
    // The stock serve export (#2659, C-375): a program whose `main` is
    // serve-shaped builds as a `wasi:http/handler@0.3.0` component; any other
    // program that reaches `http.serve` is refused with the shape rule as the
    // reason.
    let serve_export = !embedded_leg && serves && serve_shape.is_ok();
    let shape_refusal = match serve_shape {
        Err(why) if !embedded_leg && serves => Some(why),
        _ => None,
    };
    if let Some(why) = shape_refusal {
        err(&format!(
            "error[E081]: `http.serve` is not available on --target wasm from this `main`\n  \
             reason: {why}\n  \
             note: `almide run --target wasm` and the native target serve it as written"
        ));
    }
    if hits.is_empty() {
        return if shape_refusal.is_some() { Err(()) } else { Ok(serve_export) };
    }
    for (key, (_, r_stock, r_emb, r_shared, alt)) in &hits {
        let reason = if embedded_leg { r_emb.as_ref() } else { r_stock.as_ref() }
            .or(r_shared.as_ref())
            .cloned()
            .unwrap_or_else(|| "declared unavailable on this leg".to_string());
        let alt_line = alt.as_ref().map(|a| format!("\n  try: {a}")).unwrap_or_default();
        err(&format!(
            "error[E081]: `{key}` is not available on --target wasm\n  \
             reason: {reason}{alt_line}\n  \
             note: the availability matrix is proofs/target-availability.toml (#1423); \
             the same program builds with --target rust"
        ));
    }
    Err(())
}

/// Whether any reachable body calls `http.serve` — the measurement escape's
/// stand-in for the availability scan's hit.
fn reaches_http_serve(ir_program: &almide::ir::IrProgram) -> bool {
    use almide::ir::visit::IrVisitor;
    struct Find(bool);
    impl IrVisitor for Find {
        fn visit_expr(&mut self, e: &almide::ir::IrExpr) {
            if let almide::ir::IrExprKind::Call { target: almide::ir::CallTarget::Module { module, func, .. }, .. } = &e.kind
                && module.as_str() == "http"
                && func.as_str() == "serve"
            {
                self.0 = true;
            }
            almide::ir::visit::walk_expr(self, e);
        }
    }
    let mut find = Find(false);
    for f in ir_program.functions.iter().chain(ir_program.modules.iter().flat_map(|m| m.functions.iter())) {
        find.visit_expr(&f.body);
    }
    find.0
}

fn check_no_native_only_matrix(ir_program: &almide::ir::IrProgram) -> Result<(), ()> {
    if let Some(op) = almide::codegen::program_uses_native_only_matrix_on_wasm(ir_program) {
        err(&format!(
            "error: matrix.{op} is native-only (a packed-GGUF fast path with no WASM \
             lowering) — not available on the WASM target. Use --target rust, or compose \
             the block from the primitive matrix ops."
        ));
        return Err(());
    }
    Ok(())
}

/// The wasm leg (#2752: the only one): the structural emitter
/// (`almide::wasm_leg` + `almide_wasm::emit_program`).
///
/// The routing rules live in `almide::wasm_route::route_wasm` (#2554) — ONE
/// implementation the CLI and library consumers (the playground) share; this
/// wrapper reads the probe switches off the environment
/// (`RouteOptions::from_env`), narrates under `ALMIDE_VERIFIED_DEBUG`, and
/// renders every refusal.
///
/// The bytes import `almide.*` and run on the embedded host; the build path
/// converts them with `to_wasi` for stock runtimes.
fn render_wasm_module_routed(
    file: &str,
    source_text: &str,
    library_ok: bool,
    serve_export: bool,
    inputs: almide::wasm_route::RouteInputs,
    dep_paths: &[(project::PkgId, std::path::PathBuf)],
) -> Result<(Vec<u8>, Vec<i32>), ()> {
    use almide::wasm_route::{route_wasm, ModuleSource, RouteOptions};
    let opts = RouteOptions { serve_export, ..RouteOptions::from_env(library_ok) };
    let mut trace = |line: &str| err(line);
    match route_wasm(file, source_text, ModuleSource::Disk { dep_paths }, Some(inputs), opts, &mut trace) {
        Ok(module) => Ok((module.bytes, module.host_ops)),
        Err(e) => {
            report_route_error(e, file, library_ok);
            Err(())
        }
    }
}

/// The CLI rendering of a route refusal.
fn report_route_error(e: almide::wasm_route::RouteError, file: &str, library_ok: bool) {
    use almide::wasm_route::RouteError;
    match e {
        RouteError::Front(why) => err(&format!("error: {why}")),
        // #2752: said by the route itself — no leg is needed to refuse it.
        RouteError::NoMain => {
            err(&format!("error: `almide run --target wasm` needs a `main` function, and {file} declares none"));
            err(&format!("  hint: `almide build {file} --target wasm` builds a main-less library module (its `pub fn`s become exports)"));
        }
        // E082 (#1922, one leg since #2752): the structural leg declined the
        // program and there is no second renderer to hand it to.
        RouteError::Wall { why } => {
            err(&format!("error[E082]: the wasm target cannot lower this program: {why}"));
            report_structural_wall_site();
            if library_ok {
                err("  note: this is the stock-WASI BUILD route; `almide run --target wasm` (the embedded host serves every op) may still run it");
            }
            err("  note: `almide check --target wasm` reports this verdict at check time; the native target (`--target rust`) is unaffected");
            // The one machine-readable line in a wall's stderr: `wall:
            // <reason>`, whitespace-flattened to stay a single line. The
            // nightly fuzzer's honest-wall classifier keys on
            // crate::WASM_WALL_MARKER (tests/wall_shape_rendering_test.rs pins
            // it); the human lines above may be reworked freely.
            let reason_one_line = why.split_whitespace().collect::<Vec<_>>().join(" ");
            err(&format!("{}{reason_one_line}", crate::WASM_WALL_MARKER));
        }
        // E083 (#1996): a compiler ownership defect — the message must never
        // tell the writer to change valid code.
        RouteError::OwnershipLowering(d) => err(&d.to_string()),
    }
}

/// #2807: the function (and source line) the structural leg's wall came from.
/// The reason string is a census key and stays location-free; without this
/// line a `ty-mismatch:…` wall in a 77-file package could not be acted on.
fn report_structural_wall_site() {
    if let Some(site) = almide_wasm::decline_site::last() {
        err(&format!("  --> in {site}"));
    }
}

/// Compile an `.almd` file to a raw wasm32-wasi module (no wasm-opt, no file IO).
///
/// This is the single source of truth for the direct-WASM pipeline, shared by
/// `almide build --target wasm` and `almide run --target wasm`, so both emit
/// the byte-identical module the cross-target equivalence guarantee promises.
/// Compile diagnostics are rendered to stderr here; on any error it returns
/// `Err(())` and the caller decides how to terminate.
/// Returns `(wasm_bytes, produced_by_v1)`. When the second field is `true`, the module IS the
/// PCC-verified v1 trust-spine output — the caller MUST NOT post-process it (wasm-opt would replace
/// the verified bytes with an unverified transform), so `--verified` ships exactly what was verified.
pub(crate) fn compile_to_wasm_bytes(file: &str, allow_unverified: bool, verified: bool, library_ok: bool, embedded_leg: bool) -> Result<(Vec<u8>, Vec<i32>), ()> {
    compile_to_wasm_bytes_surfaced(file, allow_unverified, verified, library_ok, embedded_leg).map(|(b, o, _)| (b, o))
}

/// [`compile_to_wasm_bytes`] plus the program's host-visible surface (the
/// `pub fn` exports, the `@extern(wasm, ..)` imports, whether `main` exists),
/// read from the IR before routing — what `--host js` marshals (#2265).
pub(crate) fn compile_to_wasm_bytes_surfaced(file: &str, allow_unverified: bool, verified: bool, library_ok: bool, embedded_leg: bool) -> Result<(Vec<u8>, Vec<i32>, crate::cli::js_host::HostSurface), ()> {
    let (mut program, source_text, mut resolved, dep_paths) = parse_and_resolve_wasm(file)?;
    // Read off the source before the checker desugars it (#2659).
    let serve_shape = almide::serve_export::check_serve_shape(&program);
    // ALMIDE_WASM_ALLOC_COUNT (#2407): arm the structural leg's allocation
    // counters for this emission — the wasm twin of `arm_alloc_count`. The
    // guard scopes the thread-local to this build; off, nothing is emitted.
    let _alloc_count = almide_base::env::flag("ALMIDE_WASM_ALLOC_COUNT")
        .then(almide_wasm::alloc_count::CountGuard::set);

    // The route resolves the module list itself (once, off disk, through the
    // same resolver) and hands the FRESH un-inferred programs to whichever
    // leg renders — the capture that used to live here (#782).
    let mut checker = typecheck_wasm_program(file, &source_text, &mut program, &resolved)?;
    let mut ir_program = lower_and_link_wasm_ir(&program, &mut checker, &mut resolved)?;
    verify_wasm_ir(&ir_program)?;
    check_no_native_only_matrix(&ir_program)?;
    let package: std::collections::HashSet<String> = resolved
        .modules
        .iter()
        .map(|(name, ..)| name.clone())
        .filter(|name| resolved.sources.contains_key(name))
        .collect();
    // The availability check runs on EVERY route (run, check, build); only the
    // serve-export verdict it answers is the build route's (#2659).
    let serves_export = check_wasm_availability(&ir_program, &package, embedded_leg, &serve_shape)?;
    let serve_export = library_ok && serves_export;
    // `[permissions]` (`allow`, and `proc` #2589 — statically, and as the
    // embedded host's run-time bound) was enforced in `lower_and_link_wasm_ir`.

    // Routing inputs (`RouteInputs::of_ir`, the one rule): project shape,
    // decided from what the v0 gates already computed — never from a
    // failure. `@export`-attributed fns survive as wasm exports under their
    // declared names (the DCE-root contract, wasm_export_dce_root_test).
    let inputs = almide::wasm_route::RouteInputs::of_ir(&ir_program);
    let surface = crate::cli::js_host::HostSurface::of(&ir_program);
    // #2276: the allocator/release exports ship only for a surface that
    // marshals a String — decided here, before the module renders.
    if almide_wasm::host_exports::js_host() {
        almide_wasm::host_exports::set_string_abi(surface.needs_string_abi());
    }
    // Host routing is decided from the EMITTED op set, never from import
    // names (#1921): `render_wasm_module_routed` audits the host ops the
    // module emits against the p1 shim's served set on the BUILD path
    // (`library_ok`) — an op the `to_wasi` transform cannot serve is a wall
    // there, while `almide run --target wasm` (the embedded host serves every
    // op) and the direct p3 component (its shim carries the fs surface) keep
    // the module. An unlinked stdlib fn walls at lowering (#1598), so every
    // newly linked fn flips its own verdict with no hand-mirrored list.
    let _ = (&mut ir_program, allow_unverified, verified);
    let (bytes, host_ops) = render_wasm_module_routed(file, &source_text, library_ok, serve_export, inputs, &dep_paths)?;
    // The export's world imports no wasi:filesystem (`wasmtime serve` links
    // none without a flag): an op the service shim cannot answer is refused
    // here, at check time as at build time.
    if serve_export && let Err(message) = almide_wasm_run::component_availability::check_service(&host_ops) {
        err(&message);
        return Err(());
    }
    Ok((bytes, host_ops, surface))
}
