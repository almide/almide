// The interpreter leg of the spec/wasm_cross corpus: lowers a fixture to
// linked IR in-process and runs the reference interpreter over it. Shared by
// tests/wasm_runtime_interp_oracle.rs (through corpus.rs) and by
// tests/wasm_runtime_interp_ledger.rs, which includes THIS file alone — the
// ledger is backend-free by design and must never touch the corpus table.
//
// Imports for the interp leg, which lowers source to linked IR in-process.
// The root package already depends on every one of these crates, which is why
// the wasm_runtime_* binaries can host the 3-way gate at all.
use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::lower::lower_program;
use almide_interp::{Interpreter, RunStatus};
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;

// ── Leg 4: the reference interpreter (no build — evaluates linked IR in-process) ──

/// The interp leg's outcome. Either an observable `(exit, stdout, stderr)`
/// 3-tuple to vote with, or a `Skip(reason)` meaning the interpreter cannot run
/// this fixture — NOT a divergence.
enum InterpLeg {
    Ran(i32, String, String),
    Skip(String),
}

/// Lower `source` to a linked `IrProgram` at the interpreter's cut point
/// (`lower → optimize → mono → ir_link`) with NO stdlib bodies loaded — the same
/// lightweight `canonicalize(.., iter::empty())` recipe the interp's eval_test
/// uses. Returns `Err(reason)` (rather than panicking) when the program does not
/// parse / typecheck at this cut point, so the harness can record it as a clean,
/// reasoned skip (e.g. `import json` is unresolved without the json module
/// source). All of parse / check / lower run under `catch_unwind` so an internal
/// `assert`/`unwrap` in the frontend on an out-of-scope construct degrades to a
/// skip instead of crashing the whole gate.
fn lower_for_interp(source: &str) -> Result<almide_ir::IrProgram, String> {
    let src = source.to_string();
    let result = std::panic::catch_unwind(move || {
        let tokens = Lexer::tokenize(&src);
        let mut parser = Parser::new(tokens);
        let mut prog = match parser.parse() {
            Ok(p) => p,
            Err(e) => return Err(format!("parse error: {:?}", e)),
        };
        if !parser.errors.is_empty() {
            return Err(format!("parse errors: {:?}", parser.errors));
        }

        // The BUNDLED pure-Almide modules the fixture imports, loaded the way
        // the DRIVER's resolve loads them (parse the embedded source, register
        // as a user module, lower into `ir.modules`) — so `args.*` bodies land
        // in the interp's tier-(i) module dispatch instead of falling through
        // to `Unsupported` (#1217). Allowlisted per module and expanded by
        // MEASUREMENT (the abstain ledger names what each addition closes):
        // a module whose fns reach unfloored effect prims would lower fine
        // here and then abstain per call anyway, so listing it buys nothing.
        const INTERP_BUNDLED_MODULES: &[&str] = &["args"];
        let mut bundled: Vec<(String, almide_lang::ast::Program)> = Vec::new();
        for imp in &prog.imports {
            let almide_lang::ast::Decl::Import { path, .. } = imp else { continue };
            let Some(root) = path.first() else { continue };
            let name = root.as_str();
            if !INTERP_BUNDLED_MODULES.contains(&name) {
                continue;
            }
            if let Some(src) = almide_lang::stdlib_info::bundled_source(name)
                && let Some(p) = almide_lang::parse_cached(src)
            {
                bundled.push((name.to_string(), p.clone()));
            }
        }

        let canon = canonicalize::canonicalize_program(
            &prog,
            bundled.iter().map(|(n, p)| (n.as_str(), p, false)),
        );
        let mut checker = Checker::from_env(canon.env);
        let diags = checker.infer_program(&mut prog);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.level == almide_frontend::diagnostic::Level::Error)
            .map(|d| d.message.clone())
            .collect();
        if !errors.is_empty() {
            // The common case: an `import json|regex|…` fixture whose module
            // source is not loaded by the empty-stdlib recipe. A reasoned skip,
            // not a divergence.
            return Err(format!("type errors at interp cut point: {:?}", errors));
        }

        let mut ir = lower_program(&prog, &checker.env, &checker.type_map);
        // Mirror `lower_one_user_module`'s essentials (compile_driver.rs): infer
        // the module body, swap in ITS import table for lowering, push the
        // lowered IrModule. A module type error is a reasoned skip, not a crash.
        for (name, mod_prog) in &mut bundled {
            // The DRIVER swallows a bundled module's own check diagnostics (its
            // reporting path is keyed by user source files, which a bundled
            // module does not have) and lowers anyway — mirror that exactly;
            // the 3-way voting gate is the arbiter of whether the result is
            // faithful, not this recipe.
            checker.infer_module(mod_prog, name);
            let (mod_table, _) = almide_frontend::import_table::build_import_table(
                mod_prog,
                Some(name),
                &checker.env.user_modules,
            );
            let saved = std::mem::replace(&mut checker.env.import_table, mod_table);
            let m = almide_frontend::lower::lower_module(
                name,
                mod_prog,
                &checker.env,
                &checker.type_map,
                None,
            );
            checker.env.import_table = saved;
            ir.modules.push(m);
        }
        almide_driver::link_ir(&mut ir);
        Ok(ir)
    });
    match result {
        Ok(r) => r,
        Err(_) => Err("interp lowering panicked (out-of-scope construct)".to_string()),
    }
}

/// Run the fixture through the interpreter. Maps the interpreter's own
/// self-reported scope limits to a `Skip`; everything else is a real third vote.
/// stdout/stderr are `.trim()`-ed to match the native/wasm legs' comparison.
fn run_interp_capture(source: &str) -> InterpLeg {
    let ir = match lower_for_interp(source) {
        Ok(ir) => ir,
        Err(reason) => return InterpLeg::Skip(reason),
    };
    // The interpreter is single-shot per program; catch a defensive panic so an
    // evaluator bug surfaces as a loud skip rather than poisoning the gate.
    let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Interpreter::new(&ir).run_main()
    })) {
        Ok(o) => o,
        Err(_) => return InterpLeg::Skip("interp evaluation panicked".to_string()),
    };
    match &outcome.status {
        // `Exited(n)` RAN — it is an explicit `process.exit(n)`, a real
        // observable outcome the backends reproduce exactly, so it casts a
        // vote like Ok and Aborted. Folding it into a skip would have hidden
        // #1124's fixture from the third judge, which is the one that catches
        // a bug both backends share.
        RunStatus::Ok | RunStatus::Aborted | RunStatus::Exited(_) => InterpLeg::Ran(
            outcome.exit_code(),
            outcome.stdout.trim().to_string(),
            outcome.stderr.trim().to_string(),
        ),
        RunStatus::Unsupported(what) => {
            InterpLeg::Skip(format!("out-of-interp-scope capability: {what}"))
        }
        RunStatus::FuelExhausted => {
            InterpLeg::Skip("interp fuel/recursion budget exhausted".to_string())
        }
    }
}
