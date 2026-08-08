
// Imports for the interp leg, which lowers source to linked IR in-process.
// The root package already depends on every one of these crates, which is why
// this binary can host the 3-way gate at all.
use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::ir_link;
use almide_frontend::lower::lower_program;
use almide_interp::{Interpreter, RunStatus};
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;
use almide_optimize::{mono, optimize};
use std::path::PathBuf;

// ── The spec/wasm_cross corpus: built ONCE, asserted by three gates ──
//
// Three gates ask three different questions about the SAME 318 fixtures:
//
//   wasm_cross_target_spec    native == wasm            (the equivalence law)
//   wasm_opt_parity_spec      wasm   == wasm-opt        (the optimizer is observable-neutral)
//   interp_cross_target_spec  interp == native == wasm  (a third judge that shares no codegen pass)
//
// Each used to walk the corpus itself, so the same program was compiled six
// times per fixture: native twice (cross-target + interp), plain wasm three
// times (cross-target + interp + wasm-opt's baseline), wasm-opt once. Over the
// corpus that is 1908 builds where 954 are redundant.
//
// They now share one lazily-built table. The legs are produced once per fixture
// and every gate reads them, so the corpus costs native + wasm + wasm-opt — half
// the builds — while each gate keeps its own `#[test]`, its own name, and its
// own assertions verbatim. Sharing requires ONE test binary: `tests/*.rs` each
// compile to a separate process, so a cache in one cannot serve another. That is
// why the wasm-opt and interp gates were moved into this binary rather than left
// where they were.
//
// The interp leg needs no build at all — it evaluates the linked IR in-process,
// before any target lowering (see crates/almide-interp/CLAUDE.md).

/// Every observable this corpus can produce for one fixture.
struct FixtureLegs {
    name: String,
    /// `// @xt-allow: <reason>` — a KNOWN, tracked native/wasm divergence.
    allow: Option<String>,
    native: (i32, String, String),
    wasm: (i32, String, String),
    /// `None` when the `wasm-opt` binary is absent — the other gates still run.
    wasm_opt: Option<(i32, String, String)>,
    /// `Ran` = the interpreter voted; `Skip` = its own reasoned abstention.
    interp: InterpLeg,
}

/// The corpus, built on first use. Whichever gate runs first pays for it; the
/// rest read the same table. `None` means "no usable toolchain" — every gate
/// then self-skips exactly as it did when it owned the loop.
fn corpus() -> Option<&'static Vec<FixtureLegs>> {
    static CORPUS: std::sync::OnceLock<Option<Vec<FixtureLegs>>> = std::sync::OnceLock::new();
    CORPUS.get_or_init(build_corpus).as_ref()
}

fn build_corpus() -> Option<Vec<FixtureLegs>> {
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() {
        return None;
    }
    // wasmtime runs the wasm leg and captures its stderr + exit code.
    if Command::new("wasmtime").arg("--version").output().is_err() {
        return None;
    }
    // wasm-opt is OPTIONAL: without it the parity gate self-skips, but the
    // equivalence and 3-way gates still have everything they need.
    let have_wasm_opt = Command::new("wasm-opt").arg("--version").output().is_ok();

    let spec_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross");
    if !spec_dir.exists() {
        return None;
    }
    let mut entries: Vec<_> = std::fs::read_dir(&spec_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "almd").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.path());
    if entries.is_empty() {
        return None;
    }

    let mut legs = Vec::with_capacity(entries.len());
    for entry in &entries {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let source = std::fs::read_to_string(&path).unwrap();
        let allow = source
            .lines()
            .find_map(|l| l.trim().strip_prefix("// @xt-allow:").map(|r| r.trim().to_string()));

        let native = run_native_capture(&source);
        // A build/run panic is a BACKEND bug, not a corpus problem: record it as
        // a divergent leg so the owning gate reports it with its own wording.
        let wasm = match std::panic::catch_unwind(|| run_wasm_capture(&source)) {
            Ok(Some(w)) => w,
            // A mid-run wasmtime spawn failure (it WAS probed at entry) is a
            // sentinel leg like a panic — NEVER a whole-corpus None, which
            // silently skipped every gate over every fixture (#991's mid-run
            // green-return, centralized).
            Ok(None) => (i32::MIN, "<wasmtime-spawn-failed>".to_string(), "<wasmtime-spawn-failed>".to_string()),
            Err(_) => (i32::MIN, "<panicked>".to_string(), "<panicked>".to_string()),
        };
        let wasm_opt = if have_wasm_opt {
            match std::panic::catch_unwind(|| run_wasm_opt_capture(&source)) {
                Ok(Some(o)) => Some(o),
                Ok(None) => Some((i32::MIN, "<wasmtime-spawn-failed>".to_string(), "<wasmtime-spawn-failed>".to_string())),
                Err(_) => Some((i32::MIN, "<panicked>".to_string(), "<panicked>".to_string())),
            }
        } else {
            None
        };
        let interp = run_interp_capture(&source);

        legs.push(FixtureLegs { name, allow, native, wasm, wasm_opt, interp });
    }
    Some(legs)
}

/// `--wasm-opt` twin of `run_wasm_capture`: same build, same wasmtime
/// invocation, plus the optimizer pass.
fn run_wasm_opt_capture(source: &str) -> Option<(i32, String, String)> {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("test.almd");
    let wasm_path = dir.path().join("opt.wasm");
    std::fs::write(&src_path, source).unwrap();
    let build = Command::new(almide_bin())
        .args([
            "build",
            src_path.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            wasm_path.to_str().unwrap(),
            "--wasm-opt",
        ])
        .output()
        .expect("failed to build wasm");
    assert!(
        build.status.success(),
        "wasm build failed (--wasm-opt):\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    match Command::new("wasmtime")
        .arg("--dir=/")
        .arg("-S")
        .arg("inherit-env=y")
        .arg(wasm_path.to_str().unwrap())
        .output()
    {
        // A 127 guest exit is a comparable observable, not wasmtime-absence
        // (#991) — only a spawn error means the tool is gone.
        Ok(o) => Some((
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        )),
        Err(_) => None,
    }
}

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

        let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
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

