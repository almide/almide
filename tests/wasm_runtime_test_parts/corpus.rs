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
// A table is built ON DEMAND, leg by leg (#2381). Every gate reads the plain
// wasm leg, so that one is always built; the other three are built only when
// the binary's `NEEDED_LEGS` asks for them. Before this, every binary built
// all four legs for all ~720 fixtures and then read two or three of them:
// the parity gate paid a rustc compile per fixture (the native leg) and a
// full interpreter evaluation it never compared, the equivalence gate paid
// the interpreter and wasm-opt legs it never compared, the oracle paid
// wasm-opt. Each binary declares, before the `include!`:
//
//   const NEEDED_LEGS: Legs = Legs { native: …, wasm_opt: …, interp: … };
//   const GATE_SOURCE: &str = include_str!("<its own file>");
//
// and `corpus_legs_declared_match_reads` (below, compiled into each binary)
// holds the declaration to the gate body both ways: a leg the body reads
// must be declared, a declared leg must be read. A body that reaches for an
// undeclared leg through the accessors panics on the first fixture with the
// leg's name — never a sentinel value that a byte-compare would judge.
//
// The interp leg needs no build at all — it evaluates the linked IR in-process,
// before any target lowering (interp_leg.rs; see crates/almide-interp/CLAUDE.md).

/// The legs a gate binary reads. Plain wasm is not listed: every gate reads
/// it, so `build_corpus` always builds it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Legs {
    native: bool,
    wasm_opt: bool,
    interp: bool,
}

/// Every observable this corpus can produce for one fixture. A leg the
/// binary did not declare in `NEEDED_LEGS` is `None` and is reached only
/// through the accessors below, which name the missing leg instead of
/// handing a gate something to compare.
struct FixtureLegs {
    name: String,
    /// `// @xt-allow: <reason>` — a KNOWN, tracked native/wasm divergence.
    allow: Option<String>,
    /// `None` when `NEEDED_LEGS.native` is false.
    native: Option<(i32, String, String)>,
    wasm: (i32, String, String),
    /// `None` when `NEEDED_LEGS.wasm_opt` is false OR the `wasm-opt` binary
    /// is absent — the accessor tells the two apart.
    wasm_opt: Option<(i32, String, String)>,
    /// `Ran` = the interpreter voted; `Skip` = its own reasoned abstention.
    /// `None` when `NEEDED_LEGS.interp` is false.
    interp: Option<InterpLeg>,
}

impl FixtureLegs {
    fn native(&self) -> &(i32, String, String) {
        self.native
            .as_ref()
            .unwrap_or_else(|| self.undeclared("native"))
    }

    /// `None` = the `wasm-opt` binary is absent (the gate self-skips).
    fn wasm_opt(&self) -> Option<&(i32, String, String)> {
        if !NEEDED_LEGS.wasm_opt {
            self.undeclared("wasm-opt");
        }
        self.wasm_opt.as_ref()
    }

    fn interp(&self) -> &InterpLeg {
        self.interp
            .as_ref()
            .unwrap_or_else(|| self.undeclared("interp"))
    }

    /// A gate body reached for a leg this binary never built: loud, named,
    /// on the first fixture — never a value a byte-compare could judge.
    fn undeclared(&self, leg: &str) -> ! {
        panic!(
            "{}: the {leg} leg was not built — this binary's NEEDED_LEGS = {:?} does not declare it",
            self.name, NEEDED_LEGS
        )
    }
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
    // equivalence and 3-way gates still have everything they need. A binary
    // that does not read the leg does not probe for the tool either.
    let have_wasm_opt =
        NEEDED_LEGS.wasm_opt && Command::new("wasm-opt").arg("--version").output().is_ok();

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
    // ALMIDE_CORPUS_SHARD=k/N (#2381): the k-th modulo slice of the SORTED
    // list, taken here and nowhere else. The three gates over this table
    // assert per fixture, so a slice is judged whole in its shard; the
    // walked list goes to ALMIDE_CORPUS_SHARD_DIR for the coverage step
    // (∪ shards == ls spec/wasm_cross) — the only thing that makes a
    // partition safe. `merge/N` is refused: nothing here needs aggregating.
    if let Some(shard) = almide_corpus::corpus_shard() {
        let gate = env!("CARGO_CRATE_NAME");
        shard.require_slice(gate);
        entries = shard.apply(entries, gate, |e| e.path().file_stem().unwrap().to_str().unwrap().to_string());
        let walked: Vec<String> = entries
            .iter()
            .map(|e| e.path().file_stem().unwrap().to_str().unwrap().to_string())
            .collect();
        almide_corpus::write_partial(shard, gate, "fixtures", &walked);
    }
    // ALMIDE_CORPUS_FILTER=<substring>: a developer's single-fixture loop for
    // the 3-way harnesses (seconds instead of the ~6 min full corpus). Never
    // set in CI — the ledger gate over a filtered corpus would read every
    // filtered-out row as stale.
    if let Ok(filter) = std::env::var("ALMIDE_CORPUS_FILTER") {
        entries.retain(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.contains(filter.as_str()))
        });
    }
    if entries.is_empty() {
        return None;
    }

    let sources: Vec<String> = entries
        .iter()
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .collect();

    // The interp leg needs no build, so — when this binary declares it — it
    // runs over the whole corpus on a scoped pool
    // (interp_leg.rs::interp_sweep_parallel, #2381) while THIS thread walks
    // the native/wasm builds fixture by fixture as before; the rows come back
    // in corpus order and are zipped onto the built legs below, so nothing
    // about the table depends on which finished first. An undeclared interp
    // leg spawns no sweep at all: every row gets `None`.
    let stems: Vec<String> = entries.iter().map(|e| e.path().file_stem().unwrap().to_str().unwrap().to_string()).collect();
    let (built, interps) = std::thread::scope(|scope| {
        let sweep = NEEDED_LEGS
            .interp
            .then(|| scope.spawn(|| interp_sweep_parallel(&sources, &stems)));
        let built = build_backend_legs(&entries, &sources, have_wasm_opt);
        let interps: Vec<Option<InterpLeg>> = match sweep {
            Some(handle) => handle
                .join()
                .expect("interp sweep panicked")
                .into_iter()
                .map(|(leg, _fallbacks)| Some(leg))
                .collect(),
            None => entries.iter().map(|_| None).collect(),
        };
        (built, interps)
    });
    let n_interp = interps.iter().filter(|i| i.is_some()).count();
    let (n_native, n_wasm, n_wasm_opt) = (
        built.iter().filter(|b| b.2.is_some()).count(),
        built.len(),
        built.iter().filter(|b| b.4.is_some()).count(),
    );
    let legs: Vec<FixtureLegs> = built
        .into_iter()
        .zip(interps)
        .map(|((name, allow, native, wasm, wasm_opt), interp)| FixtureLegs {
            name,
            allow,
            native,
            wasm,
            wasm_opt,
            interp,
        })
        .collect();
    // One line per table so a CI log shows what each binary paid for (#2381).
    eprintln!(
        "corpus: {} fixture(s) — built {n_native} native / {n_wasm} wasm / {n_wasm_opt} wasm-opt / {n_interp} interp leg(s) \
         (NEEDED_LEGS: native={} wasm_opt={} interp={})",
        legs.len(),
        NEEDED_LEGS.native,
        NEEDED_LEGS.wasm_opt,
        NEEDED_LEGS.interp
    );
    Some(legs)
}

/// The binary's `NEEDED_LEGS` must match what its gate body reads, both
/// ways. Read-but-undeclared would panic on the first fixture anyway (the
/// accessors); declared-but-unread is the silent one — a leg paid for and
/// then discarded, which is exactly what this table stopped doing. The read
/// set is the gate's own source: `.native` / `.wasm_opt` / `.interp` appear
/// there only as accessor calls (this file is `include!`d, not part of
/// `GATE_SOURCE`).
#[test]
fn corpus_legs_declared_match_reads() {
    for (leg, declared, marker) in [
        ("native", NEEDED_LEGS.native, ".native"),
        ("wasm-opt", NEEDED_LEGS.wasm_opt, ".wasm_opt"),
        ("interp", NEEDED_LEGS.interp, ".interp"),
    ] {
        let read = GATE_SOURCE.contains(marker);
        assert_eq!(
            declared, read,
            "{leg} leg: NEEDED_LEGS declares it = {declared}, the gate body reads it = {read} \
             (marker {marker:?} in GATE_SOURCE)"
        );
    }
}

/// The built legs of every fixture — native (when declared), wasm and (when
/// declared and the optimizer is present) wasm-opt — in corpus order, exactly
/// as `build_corpus` walked them before the interp leg moved onto its pool;
/// each is a subprocess build so the walk stays serial on the caller's thread.
type BackendLegs = (String, Option<String>, Option<(i32, String, String)>, (i32, String, String), Option<(i32, String, String)>);

fn build_backend_legs(entries: &[std::fs::DirEntry], sources: &[String], have_wasm_opt: bool) -> Vec<BackendLegs> {
    let mut legs = Vec::with_capacity(entries.len());
    // Per-fixture build wall (native + wasm + wasm-opt subprocesses),
    // recorded under ALMIDE_CORPUS_WEIGHTS_DIR (#2457) — the `build` column
    // of proofs/corpus-weights.txt.
    let mut walls: Vec<(String, std::time::Duration)> = Vec::with_capacity(entries.len());
    for (entry, source) in entries.iter().zip(sources) {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let t0 = std::time::Instant::now();
        let allow = source
            .lines()
            .find_map(|l| l.trim().strip_prefix("// @xt-allow:").map(|r| r.trim().to_string()));

        let native = NEEDED_LEGS.native.then(|| run_native_capture(source));
        // A build/run panic is a BACKEND bug, not a corpus problem: record it as
        // a divergent leg so the owning gate reports it with its own wording.
        let wasm = match std::panic::catch_unwind(|| run_wasm_capture(source)) {
            Ok(Some(w)) => w,
            // A mid-run wasmtime spawn failure (it WAS probed at entry) is a
            // sentinel leg like a panic — NEVER a whole-corpus None, which
            // silently skipped every gate over every fixture (#991's mid-run
            // green-return, centralized).
            Ok(None) => (i32::MIN, "<wasmtime-spawn-failed>".to_string(), "<wasmtime-spawn-failed>".to_string()),
            Err(_) => (i32::MIN, "<panicked>".to_string(), "<panicked>".to_string()),
        };
        let wasm_opt = if have_wasm_opt {
            match std::panic::catch_unwind(|| run_wasm_opt_capture(source)) {
                Ok(Some(o)) => Some(o),
                Ok(None) => Some((i32::MIN, "<wasmtime-spawn-failed>".to_string(), "<wasmtime-spawn-failed>".to_string())),
                Err(_) => Some((i32::MIN, "<panicked>".to_string(), "<panicked>".to_string())),
            }
        } else {
            None
        };
        walls.push((name.clone(), t0.elapsed()));
        legs.push((name, allow, native, wasm, wasm_opt));
    }
    almide_corpus::record_weights("build", env!("CARGO_CRATE_NAME"), &walls);
    legs
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
