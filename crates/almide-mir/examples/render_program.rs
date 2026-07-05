//! Render a REAL `.almd` program to a COMPLETE wasm module via the v1 MIR renderer
//! (`render_wasm_program`) — the EXECUTION-side counterpart to emit_cert_from_source
//! (the verification side). Goal: a real program runs through the v1 pipeline and
//! matches v0 byte-identical — ③ execution parity, the path to v0 replacement
//! (docs/roadmap/active/v1-kgi-kpi.md, Gap 3). Functions outside the MIR-lowering
//! subset are reported to stderr (the honest boundary), the rest rendered.
//!
//!   render_program <file.almd>   → emits the wat module to stdout

use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::ir_link;
use almide_frontend::lower::lower_program;
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;
use almide_mir::render_wasm::try_render_wasm_program;
use almide_mir::MirProgram;
use almide_optimize::{mono, optimize};
use std::collections::HashMap;

fn die(msg: String) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

/// Discover the input file's SIBLING `src/*.almd` modules so `import self.<submodule>`
/// resolves exactly as it does under `almide run`/`almide check` — reusing the CANONICAL
/// driver discovery (`almide::resolve::resolve_imports_with_deps`), never a divergent
/// re-implementation. Pure-local: `dep_paths` is empty, so only project-local `self.*`
/// siblings are loaded (no network). A NON-cross-module file (no `import self.*`, no
/// `almide.toml`) yields an empty module set — byte-identical to the prior single-file
/// behavior. If resolution fails (e.g. a sibling needs an unfetched external dep), fall
/// back to single-file mode: the file still lowers as before (its cross-module fns wall),
/// never aborting the harness. Returns the resolved `(name, Program, is_self)` triples.
/// The cached dep source dirs for the project owning `path` — walk up to its `almide.toml`,
/// parse it, and `fetch_all_deps` (cache-hit ⇒ fast, no network; the SAME computation the
/// `almide` driver runs at `main.rs`). Empty when there is no project, no deps, or a fetch
/// failure (graceful: an unresolved external import then walls honestly, the pre-change
/// outcome). This lets a cross-module file importing an EXTERNAL package (`import almai` /
/// `import toml`) resolve here exactly as under `almide run`, instead of walling on the
/// missing-sibling artifact.
fn dep_paths_for(path: &str) -> Vec<(almide::project::PkgId, std::path::PathBuf)> {
    let mut dir = std::path::Path::new(path).parent();
    while let Some(d) = dir {
        let toml = d.join("almide.toml");
        if toml.exists() {
            if let Ok(proj) = almide::project::parse_toml(&toml) {
                if let Ok(deps) = almide::project_fetch::fetch_all_deps(&proj) {
                    return deps.into_iter().map(|fd| (fd.pkg_id, fd.source_dir)).collect();
                }
            }
            return Vec::new();
        }
        dir = d.parent();
    }
    Vec::new()
}

fn discover_self_modules(
    path: &str,
    prog: &almide_lang::ast::Program,
) -> Vec<(String, almide_lang::ast::Program, bool)> {
    // Resolve when the file imports a `self.<submodule>` OR an EXTERNAL (non-stdlib) package
    // — so a cross-module file works under `almide run`-equivalent resolution (incl. fetched
    // deps), not just self-imports. A lone / stdlib-only file stays a strict no-op (no
    // almide.toml walk, no dir scan) for the v0 corpus / spec fixtures.
    let needs_resolve = prog.imports.iter().any(|d| {
        matches!(d, almide_lang::ast::Decl::Import { path, .. }
            if path.first().map(|s| {
                let s = s.as_str();
                s == "self" || !almide_lang::stdlib_info::is_stdlib_module(s)
            }).unwrap_or(false))
    });
    if !needs_resolve {
        return Vec::new();
    }
    let deps = dep_paths_for(path);
    match almide::resolve::resolve_imports_with_deps(path, prog, &deps) {
        Ok(resolved) => resolved
            .modules
            .into_iter()
            .map(|(name, p, _pkg, is_self)| (name, p, is_self))
            .collect(),
        // A self-import file whose sibling chain reaches an unresolvable (external,
        // unfetched) dep: keep the prior single-file behavior rather than abort — the
        // harness must remain TOTAL over the corpus. The cross-module fns then wall as
        // before (honest), exactly the pre-change outcome.
        Err(_) => Vec::new(),
    }
}

/// Lower `.almd` source to a linked `IrProgram` (`parse → check → lower → optimize →
/// mono → ir_link`) — the SAME frontend cut point emit_cert_from_source uses. `modules`
/// are the resolved cross-module siblings (empty ⇒ the original single-file path). When
/// The mangled flat name a user-module function gets when resolved to a user `CallFn`
/// (`bindgen` + `get_str` → `almide_rt_bindgen_get_str`) — the v1 analogue of v0's
/// `ir_link_flatten` module-fn renaming, and the call-site target this resolution emits.
fn user_module_fn_name(module: &str, func: &str) -> String {
    format!("almide_rt_{}_{}", module.replace('.', "_"), func.replace('.', "_"))
}

/// Resolve a USER-package/-module call (`bindgen.get_str(…)` via `import self as bindgen`,
/// `self.classifier.classify(…)`) to a real user `CallFn`. WITHOUT this, the MIR lowering
/// sees `CallTarget::Module { module: "bindgen", … }` and walls it as an "effectful/impure
/// stdlib Module call" — but `bindgen` is a USER module whose function is right here in
/// `ir.modules` (thanks to the sibling-link). This rewrites the CALL TARGET only (no IR-level
/// flatten — that would collide the per-module VarId regions; the sibling DEFINITIONS are
/// lowered separately to MIR with the same mangled name, see `main`):
///   • a `CallTarget::Module { m, f }` where `m` is a user module that defines `f` becomes
///     `CallTarget::Named { name: "almide_rt_<m>_<f>" }` — an ORDINARY user call.
/// SOUNDNESS (caps): the resolved name carries NO dot, so the transitive caps gate treats it
/// as a user call (analyzed via the in-profile map / tainted if unknown), NOT as a pure
/// dotted stdlib call (`is_known_free`). A self-pkg call to an EFFECTFUL user fn therefore
/// surfaces its capability transitively, exactly like any direct user call — never the
/// accept-but-unsafe omission the Module-call purity wall was guarding against. A STDLIB
/// module (`string`, bundled `json`, …) is NOT rewritten — those keep the self-host /
/// pure-combinator dispatch. No-op when there are no linked user modules.
fn resolve_user_module_calls(ir: &mut almide_ir::IrProgram) {
    use almide_ir::{CallTarget, IrExprKind, IrMutVisitor, walk_expr_mut};
    use almide_lang::intern::sym;
    let user_mods: std::collections::HashMap<String, std::collections::HashSet<String>> = ir
        .modules
        .iter()
        .filter(|m| !almide_lang::stdlib_info::is_any_stdlib(m.name.as_str()))
        .map(|m| {
            (
                m.name.as_str().to_string(),
                m.functions.iter().map(|f| f.name.as_str().to_string()).collect(),
            )
        })
        .collect();
    if user_mods.is_empty() {
        return; // single-file / stdlib-only — strict no-op.
    }
    struct Rw<'a> {
        user_mods: &'a std::collections::HashMap<String, std::collections::HashSet<String>>,
        root_fns: std::collections::HashSet<String>,
    }
    impl IrMutVisitor for Rw<'_> {
        fn visit_expr_mut(&mut self, e: &mut almide_ir::IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Call { target, .. } = &mut e.kind {
                if let CallTarget::Module { module, func, .. } = target {
                    let (m, f) = (module.as_str(), func.as_str());
                    if self.user_mods.get(m).is_some_and(|fs| fs.contains(f)) {
                        *target = CallTarget::Named { name: sym(&user_module_fn_name(m, f)) };
                    }
                } else if let CallTarget::Named { name } = target {
                    // A BARE Named call to a fn that lives in exactly ONE linked user
                    // module: the frontend resolves an `import self as g` call
                    // (`g.keyword_groups()`) to the bare name when the target is the
                    // package's own module — rewrite it to the module fn's mangled
                    // definition name (almide-grammar main → mod.almd). Ambiguity
                    // (two modules defining the name, or a root fn shadowing it)
                    // leaves the call untouched — the unlinked gate then walls it
                    // honestly instead of guessing.
                    let f = name.as_str();
                    if !self.root_fns.contains(f) {
                        let mut owners =
                            self.user_mods.iter().filter(|(_, fs)| fs.contains(f));
                        if let (Some((m, _)), None) = (owners.next(), owners.next()) {
                            *target =
                                CallTarget::Named { name: sym(&user_module_fn_name(m, f)) };
                        }
                    }
                }
            }
        }
    }
    let root_fns: std::collections::HashSet<String> =
        ir.functions.iter().map(|f| f.name.as_str().to_string()).collect();
    let mut rw = Rw { user_mods: &user_mods, root_fns };
    for func in &mut ir.functions {
        rw.visit_expr_mut(&mut func.body);
    }
    for tl in &mut ir.top_lets {
        rw.visit_expr_mut(&mut tl.value);
    }
    for m in &mut ir.modules {
        for func in &mut m.functions {
            rw.visit_expr_mut(&mut func.body);
        }
        for tl in &mut m.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
    }
}

/// non-empty, each sibling is inferred + lowered (`lower_module`) and pushed into
/// `ir.modules` — exactly the driver assembly in `src/cli/build.rs` — so a cross-module
/// record/variant type (`self.types.RunResult`) reaches `build_record_layouts` and the
/// lowering finds its field layout instead of walling. `dep_paths` is empty (the siblings
/// are pure-local), so `versioned` is always `None`.
fn source_to_ir_with(
    source: &str,
    modules: &[(String, almide_lang::ast::Program, bool)],
) -> almide_ir::IrProgram {
    let tokens = Lexer::tokenize(source);
    let mut prog = match Parser::new(tokens).parse() {
        Ok(p) => p,
        Err(e) => die(format!("parse error: {e:?}")),
    };
    let canon = canonicalize::canonicalize_program(
        &prog,
        modules.iter().map(|(n, p, s)| (n.as_str(), p, *s)),
    );
    let mut checker = Checker::from_env(canon.env);
    let diags = checker.infer_program(&mut prog);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.level == almide_frontend::diagnostic::Level::Error)
        .map(|d| d.message.clone())
        .collect();
    if !errors.is_empty() {
        die(format!("type errors: {errors:?}"));
    }
    let mut ir = lower_program(&prog, &checker.env, &checker.type_map);

    // Lower each resolved sibling MODULE into `ir.modules` — the SAME sequence the real
    // driver runs after `lower_program` (infer_module → per-module import table →
    // lower_module → push). Bundled stdlib modules carried by `resolve` are skipped (their
    // defs come from the runtime/self-host registry, not user lowering); only real user
    // siblings (`self.types`, `self.classifier`, …) contribute their type_decls + fns.
    for (name, mod_prog, is_self) in modules {
        // Skip NON-bundled stdlib modules (their defs come from the runtime/self-host
        // registry, not user lowering) — but KEEP bundled .almd stdlib + real user
        // siblings, mirroring the driver's `is_stdlib_module && !is_bundled_module` skip.
        if almide_lang::stdlib_info::is_stdlib_module(name)
            && !almide_lang::stdlib_info::is_bundled_module(name)
        {
            continue;
        }
        let mut mod_prog = mod_prog.clone();
        // A self-module's own decls resolve against `self.*`; mirror the driver's
        // self_module_name handling (None ⇒ plain prefix, which is the dep_paths-empty case).
        let _ = is_self;
        checker.infer_module(&mut mod_prog, name);
        let self_name = checker.env.self_module_name.map(|s| s.to_string());
        let import_table_name = self_name.as_deref().unwrap_or(name.as_str());
        let (mod_table, _) = almide_frontend::import_table::build_import_table(
            &mod_prog,
            Some(import_table_name),
            &checker.env.user_modules,
        );
        let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
        let mod_ir = almide_frontend::lower::lower_module(
            name,
            &mod_prog,
            &checker.env,
            &checker.type_map,
            None,
        );
        checker.env.import_table = saved_table;
        ir.modules.push(mod_ir);
    }

    // Resolve self-pkg / imported user-module calls (`bindgen.get_str` → `almide_rt_bindgen_get_str`)
    // to real user CallFns and flatten those user functions into the root, so the MIR lowering
    // treats them as ordinary user calls (caps-tracked transitively) instead of walling them as
    // opaque "impure stdlib Module" calls. No-op when there are no linked USER modules.
    resolve_user_module_calls(&mut ir);

    optimize::optimize_program(&mut ir);
    mono::monomorphize(&mut ir);
    ir_link::ir_link(&mut ir);
    ir
}

/// Single-file convenience (no cross-module siblings) — the bundled-runtime / drop-source
/// re-lowering paths, which never carry `import self.*`.
fn source_to_ir(source: &str) -> almide_ir::IrProgram {
    source_to_ir_with(source, &[])
}

fn main() {
    // STRICT VALUE MODE: this binary is an OUTPUT path — a deferred Const-0
    // must never be executable (flight-evidence-gaps F2, the prim.handle
    // literal address-0 class). The caps-counting classifier stays permissive.
    almide_mir::lower::STRICT_VALUES.store(true, std::sync::atomic::Ordering::Relaxed);

    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| die("usage: render_program <file.almd>".into()));
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| die(format!("cannot read {path}: {e}")));
    // Resolve the input file's `import self.<submodule>` siblings (canonical driver
    // discovery), so a cross-module program (dojo's parse.almd → self.types) lowers its
    // record types instead of walling. Empty for a lone single file (no-op).
    let probe_tokens = Lexer::tokenize(&source);
    let probe_prog = Parser::new(probe_tokens)
        .parse()
        .unwrap_or_else(|e| die(format!("parse error: {e:?}")));
    let self_modules = discover_self_modules(&path, &probe_prog);
    let ir = source_to_ir_with(&source, &self_modules);
    // ADT brick 5b: GENERATE the recursive-drop fns (`__drop_<T>`) for nested-variant types and
    // re-lower with them in scope (so their `type` decls resolve). These are v1-trust-spine-only —
    // v0 (the native oracle) manages its own memory and never sees them. Two-pass: the generation
    // needs the type_decls from pass 1.
    let anon_recs = almide_mir::lower::collect_recursive_anon_records(&ir);
    // A cross-module record/variant type lives in `ir.modules[*].type_decls` (the linker's phase-1
    // `ir_link` leaves modules intact). The drop generators must see the WHOLE program's type decls
    // (root + every module) so a `self.types.RunResult` field gets its `__drop_types_RunResult`
    // emitted. Union them. For a single-file program `ir.modules` is empty ⇒ exactly `ir.type_decls`.
    let mut all_type_decls = ir.type_decls.clone();
    for m in &ir.modules {
        all_type_decls.extend(m.type_decls.iter().cloned());
    }
    let uses_result_opt_str = almide_mir::lower::program_uses_result_option_str(&ir);
    let drops = format!(
        "{}{}",
        almide_mir::lower::generate_variant_drop_sources(&all_type_decls),
        almide_mir::lower::generate_record_drop_sources(&all_type_decls, &anon_recs, uses_result_opt_str),
    );
    // The generated drops free a `Value` field via the value_core INTERNAL `__drop_value` (and a
    // `List[Value]` via `__drop_list_value`) — NOT a public `value.*` the type checker knows. So when
    // the drops reference them, the re-lower's type check would fail with "undefined function
    // '__drop_value'" (porta read_message: `JsonRpcRequest { id: Value }` → `$__drop_opt_…` →
    // `$__drop_…` → `__drop_value`). Bring value_core's source into scope for that type check; at
    // render the auto-link (self_host_runtime) dedups it to one definition.
    let needs_value_core = drops.contains("__drop_value") || drops.contains("__drop_list_value");
    let value_core_src: &str = if needs_value_core {
        include_str!("../../../stdlib/value_core.almd")
    } else {
        ""
    };
    let ir = if drops.trim().is_empty() {
        ir
    } else {
        // Re-lower WITH the same self-modules in scope — the drop fns may reference
        // cross-module record/variant type decls (the resolved siblings).
        source_to_ir_with(&format!("{source}\n{value_core_src}\n{drops}"), &self_modules)
    };

    // Top-level `let` globals (VarId -> Ty), union of program + module top_lets.
    let mut globals: HashMap<almide_ir::VarId, almide_lang::types::Ty> = HashMap::new();
    // The globals' INITIALIZER exprs too — a HEAP global (the base64 alphabet / aes S-box) is
    // materialized from its real value, not walled. Same union (program + modules).
    let mut global_inits: HashMap<almide_ir::VarId, almide_ir::IrExpr> = HashMap::new();
    for tl in &ir.top_lets {
        globals.insert(tl.var, tl.ty.clone());
        global_inits.insert(tl.var, tl.value.clone());
    }
    for m in &ir.modules {
        for tl in &m.top_lets {
            globals.insert(tl.var, tl.ty.clone());
            global_inits.insert(tl.var, tl.value.clone());
        }
    }

    // The record-layout registry (type name → fields) for the VALUE MODEL: a record
    // literal / `r.x` typed `Ty::Named` resolves its field offsets here. Built from the
    // program's type declarations (+ each module's).
    let mut record_layouts = almide_mir::lower::build_record_layouts(&ir.type_decls);
    for m in &ir.modules {
        record_layouts.extend(almide_mir::lower::build_record_layouts(&m.type_decls));
    }
    // Module type decls register under their CANONICAL qualified name
    // (`grammar.KeywordGroup`), but a reference inside the importing file can
    // reach the lowering as the BARE base name — the field-offset lookup then
    // missed and the record read silently shifted (the almide-grammar `info`
    // garble). Alias each UNIQUELY-owned base name to its qualified layout;
    // an ambiguous base (two modules, same-named type) stays qualified-only,
    // so a wrong-guess read is impossible (the lookup misses → walls).
    {
        let mut owners: std::collections::HashMap<String, Vec<String>> = Default::default();
        for k in record_layouts.keys() {
            if let Some((_, base)) = k.rsplit_once('.') {
                owners.entry(base.to_string()).or_default().push(k.clone());
            }
        }
        for (base, ks) in owners {
            if ks.len() == 1 && !record_layouts.contains_key(&base) {
                let v = record_layouts.get(&ks[0]).cloned().unwrap();
                record_layouts.insert(base, v);
            }
        }
    }

    // The variant-layout registry (type name → tag + per-constructor fields) for custom
    // ADTs, the value-model sibling of `record_layouts`. A variant construct / `match`
    // resolves its tag + field slots here.
    let mut variant_layouts = almide_mir::lower::build_variant_layouts(&ir.type_decls);
    for m in &ir.modules {
        let m_vl = almide_mir::lower::build_variant_layouts(&m.type_decls);
        variant_layouts.by_type.extend(m_vl.by_type);
        variant_layouts.ctor_to_type.extend(m_vl.ctor_to_type);
    }
    // The same unique-base aliasing as record_layouts below (a bare `Named`
    // reference to a module ADT must resolve its tag/field layout).
    {
        let mut owners: std::collections::HashMap<String, Vec<String>> = Default::default();
        for k in variant_layouts.by_type.keys() {
            if let Some((_, base)) = k.rsplit_once('.') {
                owners.entry(base.to_string()).or_default().push(k.clone());
            }
        }
        for (base, ks) in owners {
            if ks.len() == 1 && !variant_layouts.by_type.contains_key(&base) {
                let v = variant_layouts.by_type.get(&ks[0]).cloned().unwrap();
                variant_layouts.by_type.insert(base, v);
            }
        }
    }

    // PROGRAM pre-pass: inline mutual-recursive tail siblings so the parser loops become direct
    // self-recursion (exposed to the append-accumulator TCO). Guarded: only where it makes a walled
    // function lower (no regression). Semantics-preserving.
    let inlined_fns =
        almide_mir::lower::inline_mutual_tail_recursion(&ir.functions, &globals, &record_layouts);

    let mut functions = Vec::new();
    let mut walled = Vec::new();
    for func in &inlined_fns {
        // `test "…" { … }` blocks lower to functions whose bodies call the test harness
        // (`assert_eq`, …) — runtime symbols with no wasm definition. They are NEVER
        // reachable from `_start`/`main`, so they are not part of the EXECUTABLE program
        // this renderer emits for ③ execution parity. Skip them: rendering a test fn would
        // only pull in dangling `(call $assert_eq …)` references and wall the whole module.
        if func.is_test {
            continue;
        }
        // lower_function_all_with_types returns the main function plus any lambda-lifted
        // auxiliaries (index 0 is main); all go into the module so the function table
        // covers them. The record registry is threaded so `Ty::Named` records materialize.
        match almide_mir::lower::lower_function_all_with_globals(
            func,
            &globals,
            &global_inits,
            &record_layouts,
            &variant_layouts,
        ) {
            Ok(mirs) => functions.extend(mirs),
            Err(e) => walled.push(format!("{}: {e:?}", func.name.as_str())),
        }
    }
    if !walled.is_empty() {
        eprintln!(
            "[render_program] {} of {} function(s) outside the lowering subset (NOT rendered):",
            walled.len(),
            ir.functions.len()
        );
        for w in &walled {
            eprintln!("  {w}");
        }
    }

    // Lower the linked USER-module functions (`bindgen.get_str`, `self.classifier.classify`) that
    // the target's resolved `CallTarget::Named { almide_rt_<m>_<f> }` now references, renamed to the
    // SAME mangled name — so the call resolves to a real wasm definition. Each is lowered SEPARATELY
    // (its own per-module VarId region + the shared globals), avoiding any IR-level var_table merge.
    // A sibling that itself WALLS is silently skipped (NOT reported — Task-1 dedup: a sibling's own
    // walls are reported only when that sibling is the sweep TARGET); the target then fails the
    // unlinked-call render wall if it truly needed that sibling, which is honest. Stdlib modules stay
    // out (handled by the self-host runtime auto-link below). No-op when there are no user modules.
    let already: std::collections::HashSet<String> =
        functions.iter().map(|f| f.name.clone()).collect();
    for m in &ir.modules {
        if almide_lang::stdlib_info::is_any_stdlib(m.name.as_str()) {
            continue;
        }
        let mname = m.name.as_str().to_string();
        for func in &m.functions {
            if func.is_test {
                continue;
            }
            let mangled = user_module_fn_name(&mname, func.name.as_str());
            if already.contains(&mangled) {
                continue;
            }
            if let Ok(mirs) = almide_mir::lower::lower_function_all_with_globals(
                func,
                &globals,
                &global_inits,
                &record_layouts,
                &variant_layouts,
            ) {
                // mirs[0] is the sibling fn — rename to the mangled call-site name; lambda-lifted
                // auxiliaries (mirs[1..]) keep their fresh names (already unique).
                for (i, mut mir) in mirs.into_iter().enumerate() {
                    if i == 0 {
                        mir.name = mangled.clone();
                    }
                    functions.push(mir);
                }
            }
        }
    }

    // Auto-link the self-hosted stdlib runtime (the registry — int.to_string, string.concat,
    // …) when an entry is called but not defined, renaming its impl fn to the call name. A linked
    // impl may itself call ANOTHER registry entry (e.g. `list.to_string_f` → `float.to_string`), so
    // iterate to a FIXPOINT: keep linking until a full pass adds nothing. (The test harness
    // `lower_source` gets this transitive closure for free via its recursive auto-link; this loop
    // is the example-side equivalent.)
    loop {
        let before = functions.len();
        for (source, entries) in almide_mir::render_wasm::self_host_runtime() {
            let mut any_called = entries.iter().any(|(_, call)| {
                functions.iter().any(|f| {
                    f.ops.iter().any(|op| matches!(op, almide_mir::Op::CallFn { name, .. } if name == call))
                })
            });
            // A Value drop (DropValue & friends) renders `(call $__drop_value …)` — a value_core
            // helper that is NOT a registered call_name, so it is only pulled WITH value_core. A
            // program that builds Values via `json.*` (not `value.*`) reaches no value_core call_name
            // yet still drops Values, so force value_core when ANY Value-drop op is present.
            if entries.iter().any(|(_, c)| *c == "value.null") {
                any_called = any_called
                    || functions.iter().any(|f| {
                        f.ops.iter().any(|op| {
                            matches!(
                                op,
                                almide_mir::Op::DropValue { .. }
                                    | almide_mir::Op::DropListValue { .. }
                                    | almide_mir::Op::DropListStrValue { .. }
                                    | almide_mir::Op::DropListStrStr { .. }
                                    | almide_mir::Op::DropResultValue { .. }
                                    | almide_mir::Op::DropResultListValue { .. }
                            )
                        })
                    });
            }
            let any_defined =
                entries.iter().any(|(_, call)| functions.iter().any(|f| &f.name == call));
            if any_called && !any_defined {
                let rt = source_to_ir(source);
                for f in &rt.functions {
                    let lowered = almide_mir::lower::lower_function(f, &globals);
                    // A self-host fn that fails to lower leaves its call name UNLINKED —
                    // surface the reason instead of skipping silently (the unlinked-call
                    // reject below names the symptom; this names the cause).
                    if let Err(e) = &lowered {
                        if entries.iter().any(|(impl_fn, _)| f.name.as_str() == *impl_fn)
                            || f.name.as_str().starts_with("__")
                        {
                            eprintln!("[self-host] {} failed to lower: {:?}", f.name.as_str(), e);
                        }
                    }
                    if let Ok(mut mir) = lowered {
                        if let Some((_, call)) = entries.iter().find(|(impl_fn, _)| &mir.name == impl_fn) {
                            mir.name = call.to_string();
                        }
                        functions.push(mir);
                    }
                }
            }
        }
        // Dedup by name (a recursively-linked impl carries its own helper copies, e.g. print_str);
        // keep the first definition — identical source ⇒ a no-op merge.
        let mut seen = std::collections::HashSet::new();
        functions.retain(|f| seen.insert(f.name.clone()));
        if functions.len() == before {
            break;
        }
    }

    // A self-hosted runtime fn may call ANOTHER registered impl by its IMPL name (e.g. value_core's
    // `__vstr_arr` recursing through `value_stringify`), but the auto-link RENAMED that def to its
    // call_name (`value.stringify`). Rewrite those call sites to the call_name so the internal
    // recursion resolves to the renamed def instead of a dangling impl-name `(call $…)`.
    let impl_to_call: std::collections::HashMap<&str, &str> =
        almide_mir::render_wasm::self_host_runtime()
            .iter()
            .flat_map(|(_, es)| es.iter().map(|(i, c)| (*i, *c)))
            .collect();
    for f in &mut functions {
        for op in &mut f.ops {
            if let almide_mir::Op::CallFn { name, .. } = op {
                if let Some(&c) = impl_to_call.get(name.as_str()) {
                    *name = c.to_string();
                }
            }
        }
    }

    // Auto-link the self-hosted runtime: `println(s)` lowers to a `PrintStr` call
    // rendered as `(call $print_str ...)`, so a program that prints needs the
    // Almide-written `print_str` (compiled through this same pipeline). Include the
    // bundled runtime unless the program already defines it — the v1 runtime-linking
    // step (the self-host vision: no hand-written wasm for print).
    if !functions.iter().any(|f| f.name == "print_str") {
        let rt = source_to_ir(include_str!("../../../stdlib/print_str.almd"));
        for f in &rt.functions {
            if let Ok(mir) = almide_mir::lower::lower_function(f, &globals) {
                functions.push(mir);
            }
        }
    }

    // EAGER GLOBAL-INIT semantics (C-007, top_let_div_eager): v0 evaluates every
    // ABORTABLE top-let initializer at startup — `let bad = 10 / 0` aborts BEFORE
    // main even when `bad` is never used. The v1 per-use materialization is
    // observably identical for pure inits EXCEPT the trapping ones, so synthesize
    // `__global_init` binding each CALL-FREE SCALAR initializer (the checked
    // div/mod inside aborts with the native-identical stderr + exit 1) and have
    // `_start` call it before `$main`. Call-bearing/heap inits keep their
    // existing per-use/wall handling (out of this eager slice).
    {
        fn has_call(e: &almide_ir::IrExpr) -> bool {
            use almide_ir::visit::{walk_expr, IrVisitor};
            struct C(bool);
            impl IrVisitor for C {
                fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                    if matches!(
                        e.kind,
                        almide_ir::IrExprKind::Call { .. } | almide_ir::IrExprKind::RuntimeCall { .. }
                    ) {
                        self.0 = true;
                    }
                    walk_expr(self, e);
                }
            }
            let mut c = C(false);
            c.visit_expr(e);
            c.0
        }
        let mut max_var = 0u32;
        for (v, _) in &globals {
            max_var = max_var.max(v.0);
        }
        {
            use almide_ir::visit::{walk_expr, IrVisitor};
            struct M(u32);
            impl IrVisitor for M {
                fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                    if let almide_ir::IrExprKind::Var { id } = &e.kind {
                        self.0 = self.0.max(id.0);
                    }
                    walk_expr(self, e);
                }
            }
            let mut m = M(max_var);
            for f in &ir.functions {
                m.visit_expr(&f.body);
            }
            max_var = m.0;
        }
        let mut stmts: Vec<almide_ir::IrStmt> = Vec::new();
        let mut ordered: Vec<_> = ir
            .top_lets
            .iter()
            .chain(ir.modules.iter().flat_map(|m| m.top_lets.iter()))
            .collect();
        ordered.sort_by_key(|tl| tl.var.0);
        // A later initializer may READ an earlier global (`let half = 10 / zero`) —
        // inside the synthesized fn those are unresolvable globals (strict mode
        // refuses the fallback), so INLINE each processed init into its dependents
        // (declaration order; all call-free, so substitution is pure and finite).
        let mut subst: std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr> =
            std::collections::HashMap::new();
        for tl in ordered {
            let scalar = !almide_mir::lower::is_heap_ty(&tl.ty);
            if scalar && !has_call(&tl.value) {
                let mut value = tl.value.clone();
                for (gv, ge) in &subst {
                    value = almide_ir::substitute::substitute_var_in_expr(&value, *gv, ge);
                }
                subst.insert(tl.var, value.clone());
                max_var += 1;
                stmts.push(almide_ir::IrStmt {
                    kind: almide_ir::IrStmtKind::Bind {
                        var: almide_ir::VarId(max_var),
                        mutability: almide_ir::Mutability::Let,
                        ty: tl.ty.clone(),
                        value,
                    },
                    span: None,
                });
            }
        }
        if !stmts.is_empty() {
            let body = almide_ir::IrExpr {
                kind: almide_ir::IrExprKind::Block { stmts, expr: None },
                ty: almide_lang::types::Ty::Unit,
                span: Default::default(),
                def_id: None,
            };
            let init_fn = almide_ir::IrFunction {
                name: almide_lang::intern::sym("__global_init"),
                params: vec![],
                ret_ty: almide_lang::types::Ty::Unit,
                body,
                is_effect: false,
                is_async: false,
                is_test: false,
                generics: None,
                extern_attrs: vec![],
                export_attrs: vec![],
                attrs: vec![],
                visibility: almide_ir::IrVisibility::Public,
                doc: None,
                blank_lines_before: 0,
                def_id: None,
                module_origin: None,
                mutated_params: vec![],
            };
            if let Ok(mir) = almide_mir::lower::lower_function(&init_fn, &globals) {
                functions.push(mir);
            }
        }
    }

    // If `main` itself was WALLED out of the lowering subset (it needs a capability,
    // a RawPtr with no scalar Repr, etc.), there is no `$main` in `functions` — yet
    // render_wasm_program unconditionally emits `(func (export "_start") (call $main))`.
    // That dangling `$main` is invalid wasm. Wall the WHOLE program cleanly instead of
    // emitting a main-less module, so the sweep categorizes it as a wall (not a RUNERR).
    if !functions.iter().any(|f| f.name == "main") {
        die("[render_program] Unsupported: main is outside the MIR-lowering subset".into());
    }

    // Wall any UNLINKED stdlib/runtime call: a `(call $name)` to a function that is
    // neither defined here (user / auto-linked self-host / print_str) nor a preamble
    // runtime fn would be a dangling reference (invalid wasm). Reject cleanly instead of
    // emitting it — conservative and structurally sound (it only removes a bad module).
    match try_render_wasm_program(&MirProgram { functions }) {
        Ok(wat) => print!("{wat}"),
        Err(e) => die(format!("[render_program] {e:?}")),
    }
}
