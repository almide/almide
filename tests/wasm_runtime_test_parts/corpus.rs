// ── The spec/wasm_cross corpus: one table per gate binary, three gates ──
//
// Three gates ask three different questions about the SAME fixtures:
//
//   wasm_cross_target_spec    native == wasm            (the equivalence law)                        tests/wasm_runtime_cross_target.rs
//   wasm_opt_parity_spec      wasm   == wasm-opt        (the optimizer is observable-neutral)        tests/wasm_runtime_opt_parity.rs
//   interp_cross_target_spec  interp == native == wasm  (a third judge that shares no codegen pass)  tests/wasm_runtime_interp_oracle.rs
//
// Each used to walk the corpus itself, so the same program was compiled six
// times per fixture: native twice (cross-target + interp), plain wasm three
// times (cross-target + interp + wasm-opt's baseline), wasm-opt once. They
// were then folded into ONE binary around this lazily-built table, so a
// `cargo test` process paid native + wasm + wasm-opt once for all three.
//
// That in-process sharing no longer exists on CI, by design: the shards run
// under cargo-nextest, which executes every test in its OWN process, so each
// gate was already building its own table there (measured on run
// 33695667642: 360 s + 360 s + 517 s, serialized end to end by
// .config/nextest.toml's shared-fixture group). The three gates therefore
// live in three binaries that share this SOURCE — each `include!`s it and
// builds its own table — so the shard packer (scripts/ci-test-shard.sh) can
// put each corpus build on a different runner instead of stacking all three
// on one shard. What the split costs is a local `cargo test` that runs more
// than one of the three: each process builds the table once. Every gate keeps
// its own `#[test]`, its own name, and its own assertions verbatim.
//
// The interp leg needs no build at all — it evaluates the linked IR in-process,
// before any target lowering (interp_leg.rs; see crates/almide-interp/CLAUDE.md).

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

/// The corpus, built on first use — once per process, i.e. once per gate
/// binary (see the header). `None` means "no usable toolchain" — the gate
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
