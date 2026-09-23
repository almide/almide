//! The `ALMIDE_*` environment switches — ONE registry (#2205).
//!
//! The tree reads behaviour switches from the environment: which wasm leg
//! renders, whether a gate is bypassed, which pass is ablated, which trace
//! channel prints. Each used to be its own `std::env::var("…")` somewhere,
//! so the set of switches existed only as a grep, two sites could disagree on
//! what "set" means (`ALMIDE_FUEL_PROBE=0` forced a leg at one site and armed
//! nothing at another), and a run could produce its verdict under a forced
//! route without the log saying so.
//!
//! [`SWITCHES`] is the roster: every name the compiler, the runtime, the test
//! harness, the scripts and the workflows read, with what it does. The
//! compiler-proper crates read ONLY through [`flag`] / [`var`], which give every
//! switch one boolean semantics and print one stderr line the first time a
//! route-forcing or gate-bypassing switch is found ON. `scripts/check-env-
//! switches.sh` refuses an unregistered name anywhere in the tree, a direct
//! `std::env::var("ALMIDE_…")` outside the crates that cannot depend on this one
//! (`runtime/rs`, `almide-rt-core`), and drift between this table and the one
//! rendered into `docs/specs/cli.md` (`almide switches --md`).
//!
//! The NAMES are a compatibility surface — they are what someone debugging
//! types on the command line — so a rename is a documented change, never a
//! refactor.

/// How a switch's value is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// On/off: set to anything but an OFF spelling (see [`is_on`]).
    Flag,
    /// Carries a value (a substring filter, a number, a path, a list).
    Value,
}

/// What a switch changes — the axis the stderr announcement and the docs table
/// sort by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Forces which compiler leg / emission route runs. ANNOUNCED on stderr
    /// when on: a verdict produced under a forced route must say so.
    Route,
    /// Bypasses or relaxes a check the default run performs. ANNOUNCED.
    Gate,
    /// Turns a pass or an optimisation off for measurement (ablation).
    Ablation,
    /// Arms a runtime trap the shipped artifact leaves off.
    Trap,
    /// Adds stderr output only (a trace channel, a dump, a profile).
    Debug,
    /// A CLI / tooling knob that changes where things go or how much is kept,
    /// never what a program computes.
    Tool,
    /// Read by the COMPILED PROGRAM (the native runtime, the stdlib, the
    /// embedded wasm host) at its own run time.
    Runtime,
    /// A test-harness hook: read by `tests/`, `crates/*/tests`, `tools/`.
    Harness,
    /// Set or read only by CI workflows and `scripts/`.
    Ci,
}

/// One registered switch.
#[derive(Debug)]
pub struct Switch {
    pub name: &'static str,
    pub kind: Kind,
    pub scope: Scope,
    /// What it does, in one sentence (rendered into `docs/specs/cli.md`).
    pub doc: &'static str,
}

const fn sw(name: &'static str, kind: Kind, scope: Scope, doc: &'static str) -> Switch {
    Switch { name, kind, scope, doc }
}

use Kind::{Flag, Value};
// `Debug` as a bare name would shadow the derive macro; the rows say `Dbg`.
use Scope::{Ablation, Ci, Debug as Dbg, Gate, Harness, Route, Runtime, Tool, Trap};

/// Every `ALMIDE_*` switch the tree reads. Sorted by name; the gate proves the
/// order and the completeness.
pub const SWITCHES: &[Switch] = &[
    sw("ALMIDE_ABI_PROBE", Flag, Dbg, "print the lifted-effect-fn ABI decision per function (v1 lowering)"),
    sw("ALMIDE_ALLOC_COUNT", Flag, Harness, "build the native program with a counting allocator that prints `__ALMD_ALLOC allocs=N deallocs=N reallocs=N peak=N` on stderr when `__almide_main` returns (the native borrow oracle's allocation lane, tests/native_borrow_oracle_test.rs)"),
    sw("ALMIDE_BANG_RETURN", Flag, Ablation, "turn OFF the per-position `!` desugars of the v1 lowering so every `!` reaches the bind-position rule or walls loudly (the decline-matrix probe)"),
    sw("ALMIDE_BENCH_DIR", Value, Harness, "the fixture directory of the structural leg's perf probe (default `crates/almide-wasm/tests/perf`)"),
    sw("ALMIDE_BIN", Value, Harness, "path of the `almide` binary the test harnesses, scripts and workflows drive (default: `target/release/almide`, then PATH)"),
    sw("ALMIDE_BORROW_OWN_ALL", Flag, Ablation,"make BorrowInsertion own every borrow-eligible param, as before inference existed — the ablation the ownership certifier's C4 sensitivity test drives, and the borrow-inference perf knob"),
    sw("ALMIDE_BOUNDED_DEBUG", Flag, Dbg, "print why a bounded-loop bind declined (v1 lowering)"),
    sw("ALMIDE_BUILD_PROVENANCE", Value, Ci, "read by `build.rs` at BUILD time: `release` makes `almide --version` say `(release)`, anything else (including unset) says `(dev)`. Set only by `.github/workflows/release.yml`, the one thing that builds from a tag, so a binary claiming to be a release had to come from there (#2384)"),
    sw("ALMIDE_BUILD_SHA", Value, Ci, "read by `build.rs` at BUILD time: the commit `almide --version` names beside the build kind, truncated to 9 characters. Passed in by `make install` and the release workflow rather than read from git in the build script, which would rebuild the root crate after every commit (#2384)"),
    sw("ALMIDE_CAPTURE_MOVE_OFF", Flag, Ablation, "make CaptureClone clone every capture again, as before #2231, instead of moving a value whose sole user is the closure — the ablation the ownership certifier's sensitivity test drives"),
    sw("ALMIDE_CERTIFY_OWNERSHIP", Value, Dbg, "run the native ownership certifier after the pass pipeline (#2231): `report` prints every violation, `fail` aborts the build on one, `off` skips it; unset = `fail` in a debug build, `off` in a release build"),
    sw("ALMIDE_COMPILER_STACK", Value, Tool, "stack size in bytes of the compiler driver thread (default 256 MiB); a deep input that overflows it is the regression test's subject"),
    sw("ALMIDE_COMPONENT_ADAPTER", Flag, Route, "route `--component` through the preview1 adapter instead of the direct component emission"),
    sw("ALMIDE_COMPONENT_P3", Flag, Route, "emit a WASI 0.3 component (stdio over component-model streams, the async canonical ABI) under `--component`; needs a p3-capable wasmtime"),
    sw("ALMIDE_CORPUS_FILTER", Value, Harness, "substring filter over the fixture paths the 3-way oracle test evaluates"),
    sw("ALMIDE_CORPUS_SHARD", Value, Harness, "`k/N` (1-based) walks the k-th modulo slice of the SORTED spec/wasm_cross corpus in the six corpus giants (#2381), read after the sort; `merge/N` reads the N shards' partials from `ALMIDE_CORPUS_SHARD_DIR` and judges the whole-corpus ceilings and the name-keyed bridge ledger; unset = the unsharded gate"),
    sw("ALMIDE_CORPUS_SHARD_DIR", Value, Harness, "directory where a sharded corpus gate writes its walked-fixture list and its partial counts / bridge names, and where `merge/N` reads them; unset = a local slice that writes nothing"),
    sw("ALMIDE_CORPUS_WEIGHTS_DIR", Value, Harness, "directory where a corpus gate records the wall it measured per fixture (`weights/<column>.<gate>[.<k>-of-<N>].txt`, `stem<TAB>ms`) for scripts/gen-corpus-weights.sh to render into proofs/corpus-weights.txt, the table the balanced `k/N` slices read (#2457); CI points it at the shard-partials dir so the committed table is rendered from the runner's own ratios (#2502); unset = nothing recorded"),
    sw("ALMIDE_COVERAGE_CONDITION", Value, Ci, "the coverage ratchet's condition tag (which baseline row a push is judged against)"),
    sw("ALMIDE_CWD", Value, Runtime, "the writer's working directory, set by `almide run` for the wasm host so relative fs paths resolve as on native (C-137); never set by hand"),
    sw("ALMIDE_DBG_ANF", Flag, Dbg, "print why a lambda lift or statement inline declined (v1 lowering)"),
    sw("ALMIDE_DBG_BANG", Flag, Dbg, "print the `!` unwrap decisions of the v1 bind lowering"),
    sw("ALMIDE_DBG_BORROW", Value, Dbg, "dump every native borrow-inference signature key containing the value, per fixed-point iteration, and the keys each call site consults"),
    sw("ALMIDE_DBG_CELLS", Flag, Dbg, "print the captured / mutated / celled variable sets (v1 lowering)"),
    sw("ALMIDE_DBG_CONTLIFT", Flag, Dbg, "print why a poison-oracle continuation lift rolled back (v1 lowering)"),
    sw("ALMIDE_DBG_DESUGAR_FN", Value, Dbg, "print the fully desugared body of the fn named by the value (v1 lowering; was `DBG_DESUGAR_FN` before #2205)"),
    sw("ALMIDE_DBG_DESUGAR_RAW", Flag, Dbg, "with `ALMIDE_DBG_DESUGAR_FN`, print the raw pre-desugar body too (was `DBG_DESUGAR_RAW`)"),
    sw("ALMIDE_DBG_ELEM", Flag, Dbg, "print why a list-literal Block element declined (v1 lowering)"),
    sw("ALMIDE_DBG_FAN", Flag, Dbg, "print the fan lowering's prefetch and pattern decisions (structural leg)"),
    sw("ALMIDE_DBG_GINIT", Flag, Dbg, "print the eager top-let init runner's admission set and why an extended runner declined (v1 lowering, C-007)"),
    sw("ALMIDE_DBG_LINK", Flag, Dbg, "dump the wasm link demand set and what each key resolves to"),
    sw("ALMIDE_DBG_LOWER_FN", Value, Dbg, "print the fully desugared body the v1 lowering actually lowers, for the fn named by the value (was `DBG_LOWER_FN`)"),
    sw("ALMIDE_DBG_NEMATCH", Flag, Dbg, "print the never-err match analysis per function (v1 lowering)"),
    sw("ALMIDE_DBG_NESTED_MATCH", Flag, Dbg, "print why a nested-match chain was refused (v1 lowering)"),
    sw("ALMIDE_DBG_QQ", Flag, Dbg, "print which path lowered each `??` (match-first vs route fallback, v1 lowering)"),
    sw("ALMIDE_DBG_ROUTER", Flag, Dbg, "print a stdlib call name refused for its registered signature, with the mismatch and the argument types (v1 lowering)"),
    sw("ALMIDE_DBG_SWITCH", Flag, Dbg, "print the `br_table` switch rendering decisions (v1 wasm render)"),
    sw("ALMIDE_DBG_TCO", Flag, Dbg, "print the tail-call-to-loop admission decisions (v1 lowering)"),
    sw("ALMIDE_DBG_TRAP", Flag, Dbg, "print the wasm trap's backtrace and host state when the embedded host catches one"),
    sw("ALMIDE_DBG_UNLINKED", Flag, Dbg, "print wasm references with no resolvable definition"),
    sw("ALMIDE_DBG_WAT", Value, Dbg, "print the rendered WAT of every function whose name contains the value (v1 wasm render)"),
    sw("ALMIDE_DBG_WHILE", Flag, Dbg, "print the while-loop lowering decisions (v1 lowering)"),
    sw("ALMIDE_DEBUG_CALL_OPS", Flag, Harness, "print the call ops of every lowered fn (the classify_corpus example)"),
    sw("ALMIDE_DEBUG_EFFECTS", Flag, Dbg, "print the effect inference pass's per-function results (native codegen)"),
    sw("ALMIDE_DEBUG_MIR_OPS", Flag, Harness, "print every MIR op with its index (the classify_corpus example — the certificate-bisection instrument)"),
    sw("ALMIDE_DEFAULTS_DEBUG", Flag, Dbg, "print the record-default resolution when no default keys were found (native codegen)"),
    sw("ALMIDE_DISABLE_OPT", Flag, Ablation, "run the optimiser pipeline with every optional pass off (ablation; the always-on enabler passes still run)"),
    sw("ALMIDE_DUMP_DROPS", Flag, Dbg, "print the computed drop set (v1 lowering)"),
    sw("ALMIDE_DUMP_IR", Value, Dbg, "dump the IR after the named passes (comma-separated, or `all`) on the native pipeline, and the post-chain body of every fn whose name contains the value on the v1 pipeline"),
    sw("ALMIDE_DUMP_MIR", Flag, Dbg, "print every lowered fn's op stream before the native render runs"),
    sw("ALMIDE_DUMP_VERIFY", Flag, Dbg, "print the native render's verification transcript"),
    sw("ALMIDE_DUMP_WMIR", Value, Dbg, "print the lowered wasm-leg op stream of every fn whose name contains the value"),
    sw("ALMIDE_EXPECT_TOOLS", Flag, Harness, "make a harness test FAIL instead of skipping when an external tool (wasmtime, wasm-tools) is missing; CI sets it"),
    sw("ALMIDE_FALLBACK_NAMES", Flag, Tool, "make `almide test` print one `FALLBACK <file>` line per file the wasm leg did not pass — the wasm coverage ratchet's data feed"),
    sw("ALMIDE_FAN_SEQUENTIAL", Flag, Runtime, "run `fan.*` sequentially in the native runtime (a determinism lever for measurement; the observable result is the same by contract)"),
    sw("ALMIDE_FLOAT_SWEEP_N", Value, Harness, "how many xorshift64 bit patterns the float printer sweep prints and compares with Rust `format!` on each leg (default 100000; tests/float_to_string_cross_target_test.rs)"),
    sw("ALMIDE_FN_ESCAPE_OFF", Flag, Ablation, "make BorrowInsertion borrow EVERY fn-typed param as `&dyn Fn`, escaping or not (#2288) — the ablation the ownership certifier's C5 sensitivity test drives"),
    sw("ALMIDE_FUEL_PROBE", Flag, Route,"insert fuel charges and force the INCUMBENT wasm leg (the charge probe); `almide run --time-report` sets it internally"),
    sw("ALMIDE_FUZZ_BASE", Value, Harness, "the first seed of the differential fuzz's fixed seed range (default 0)"),
    sw("ALMIDE_FUZZ_HOST_ORACLE", Flag, Harness, "run the differential fuzz in host-oracle mode (arm selection is deterministic per seed and mode)"),
    sw("ALMIDE_FUZZ_ITERS", Value, Harness, "how many seeds the differential fuzz's fixed range covers (default 200)"),
    sw("ALMIDE_HEAP_TRACE", Flag, Dbg, "print the interpreter's heap-block allocations and frees"),
    sw("ALMIDE_HTTP_TIMEOUT_SECS", Value, Runtime, "the http client's request timeout in seconds, read by the compiled program (default 30)"),
    sw("ALMIDE_INSTALL", Value, Tool, "the directory `almide install` installs binaries into (overrides the default `~/.local/bin`)"),
    sw("ALMIDE_INTERP_SWEEP_THREADS", Value, Harness, "interp sweep thread count; 1 = serial (#2381)"),
    sw("ALMIDE_IR_FAULT", Value, Harness, "inject an IR violation after the named optimiser pass, so the per-pass verifier can be watched turning red in the release binary"),
    sw("ALMIDE_KEEP_SCRATCH", Flag, Tool, "keep the `almide test` scratch build directory instead of deleting it"),
    sw("ALMIDE_LOCAL_REUSE_THRESHOLD", Value, Route, "the distinct-local count above which the v1 wasm render reuses locals (default 8000); a test knob that forces the transform on across the corpus"),
    sw("ALMIDE_LSP_TRACE", Flag, Dbg, "print every LSP request and response the language server handles"),
    sw("ALMIDE_MANIFEST_TREE_CHECK", Value, Ci, "the parity-manifest generators' stale-tree refusal (#2405, scripts/lib/oracle-header.sh): `strict` (default) refuses an ORACLE that is not `<Cargo.toml version> (dev…)`, is stamped with a commit other than HEAD, or is unstamped and older than the sources; a worktree behind its upstream; and an untracked spec/ fixture. `gate` keeps only the untracked-fixture check (scripts/check-parity-goldens.sh vouches for CI's artifact). `off` is the deliberate override"),
    sw("ALMIDE_MG_DEBUG", Flag, Dbg, "print the mutable-global slot assignment and cross-module name-bridge decisions (v1 lowering)"),
    sw("ALMIDE_MONO_DEBUG", Flag, Dbg, "print the monomorphisation discovery and instantiation decisions"),
    sw("ALMIDE_MP_PROBE", Flag, Dbg, "print the mut-param analysis decisions (IR)"),
    sw("ALMIDE_MUTATION_BASE", Value, Ci, "the base ref the mutation gate diffs against"),
    sw("ALMIDE_MUTATION_SCOPE", Value, Ci, "which mutation set the mutation gate runs"),
    sw("ALMIDE_MUTATION_SHARD", Value, Ci, "this job's shard index of the mutation gate"),
    sw("ALMIDE_MUTATION_SHARDS", Value, Ci, "how many shards the mutation gate is split into"),
    sw("ALMIDE_NAMES_DEBUG", Flag, Dbg, "print the native name-verification map and its scoped shadowing decisions"),
    sw("ALMIDE_NO_AVAIL_CHECK", Flag, Gate, "bypass the E081 stdlib availability check (the measurement escape the availability probe builds through)"),
    sw("ALMIDE_NO_BR_TABLE", Flag, Route, "render every switch as an if-chain instead of `br_table` (v1 wasm render)"),
    sw("ALMIDE_NO_RTLIB", Flag, Route, "build the native runtime inline instead of linking the prebuilt runtime crate (the self-contained cargo path; `almide test` sets it for the harness build)"),
    sw("ALMIDE_NO_VERIFIED_OK", Flag, Gate, "re-enable the retired `--no-verified` legs (the v0 fallback) instead of refusing the flag"),
    sw("ALMIDE_OMEGA", Value, Route, "the baked ω ordinal for deterministic wall-deadline replay: the artifact cuts at the n-th wall check without reading the clock (`-1` / unset = live)"),
    sw("ALMIDE_OMEGA_RECORD", Flag, Route, "make the native artifact print `__ALMD_OMEGA <ord>` at each region exit whose deadline fired (record on native, replay anywhere)"),
    sw("ALMIDE_ONLY_PASS", Value, Ablation, "run the optimiser with ONLY the named optional pass (plus the always-on enablers); an unknown name is a hard error"),
    sw("ALMIDE_ORACLE_KEEP", Flag, Harness, "keep the programs the native borrow-mode oracle generates (tests/native_borrow_oracle_test.rs) instead of deleting them after the run"),
    sw("ALMIDE_ORG_DIR", Value, Ci, "the checkout directory of the org repos the cross-repo verification scripts walk"),
    sw("ALMIDE_P3_HTTP_STOP", Value, Ablation, "make the p3 http shim answer its static error right after stage N of the request build (1..=5), to localise a hang"),
    sw("ALMIDE_PASS_EDGES", Value, Harness, "extra `A<B` pass-order edges (comma-separated) the shuffle honours — the bisection instrument that names the pair a shuffle divergence needs declared"),
    sw("ALMIDE_PROBE_DUMP", Value, Harness, "the path the heap probe writes its emitted wasm to"),
    sw("ALMIDE_PROBE_IR", Value, Harness, "the path the heap probe writes its lowered IR to"),
    sw("ALMIDE_PROBE_SRC", Value, Harness, "the source file the heap probe compiles (unset = the probe is skipped)"),
    sw("ALMIDE_PROFILE", Flag, Dbg, "print per-pass and per-phase timings of the native pipeline"),
    sw("ALMIDE_RC_TRAP_DOUBLE_FREE", Flag, Trap, "arm the structural leg's double-free trap: releasing a block already at rc 0 traps instead of wrapping"),
    sw("ALMIDE_REGION_DEBUG", Flag, Dbg, "print the region-window pass's decisions (native and structural leg)"),
    sw("ALMIDE_REGION_OFF", Flag, Ablation, "turn the region-window allocation pass off (native and structural leg)"),
    sw("ALMIDE_REGION_TRAP_STALE", Flag, Trap, "arm the native region prelude's stale-reference trap (#2200)"),
    sw("ALMIDE_RENDER", Value, Ci, "the render_program example binary the prelude audit re-renders fixtures with"),
    sw("ALMIDE_REPO", Value, Ci, "the repository slug a release script targets"),
    sw("ALMIDE_RUN_PROJECT_DIR", Value, Tool, "the scratch dir `almide run` / `almide build` compile native binaries in, instead of `<temp>/almide-run` (the content-keyed binary cache, its cargo target, its rustc incremental sessions); `almide clean` empties it"),
    sw("ALMIDE_SEMLAW_CASES", Value, Harness, "how many cases the semantic-laws property test draws"),
    sw("ALMIDE_SHUFFLE_PASSES", Value, Gate, "run the native passes in the seeded random order the declared dependency edges permit — a pass-dependency probe: the emitted Rust must not change (#2186)"),
    sw("ALMIDE_SIZE_ALONE", Value, Harness, "the one fixture a child process of the size ratchet measures alone, for its isolation check (#2309); the ratchet sets it on the processes it spawns"),
    sw("ALMIDE_SKIP_PASS", Value, Ablation, "skip the named optional passes (comma-separated) — a pass-dependency probe: output must not change"),
    sw("ALMIDE_SKIP_VERSION_CHECK", Flag, Gate, "skip the project's `almide` version requirement check"),
    sw("ALMIDE_STREAM_FUSION_OFF", Flag, Ablation, "turn the stream-fusion pass off"),
    sw("ALMIDE_TCO_DEBUG", Flag, Dbg, "print the native tail-call loop rewrite decisions"),
    sw("ALMIDE_TEST_LAX_WASM", Flag, Gate, "let the default `almide test` lane (wasm first, native fallback) PASS a file whose wasm leg diverged — trapped where the native re-run passed; without it a diverged leg fails the run"),
    sw("ALMIDE_TEST_VERBOSE", Flag, Tool, "show the full cargo / rustc output of the `almide test` harness build"),
    sw("ALMIDE_TIME_PHASES", Flag, Dbg, "print the wall-clock time of each `almide run` phase"),
    sw("ALMIDE_TMPDBG", Flag, Dbg, "print the temporary-drop decisions of the v1 lowering"),
    sw("ALMIDE_TOPLET_DEBUG", Flag, Dbg, "print the cross-module top-let type writes and reads of the checker"),
    sw("ALMIDE_TRACE_PASSES", Flag, Dbg, "name each optimiser pass BEFORE it runs, so a pass that never returns is identifiable"),
    sw("ALMIDE_UPDATE_ALLOC", Flag, Harness, "regenerate the structural leg's allocation baseline"),
    sw("ALMIDE_UPDATE_DUMPS", Flag, Harness, "regenerate the structural leg's section-dump goldens"),
    sw("ALMIDE_UPDATE_GAUNTLET", Flag, Harness, "regenerate the gauntlet manifest"),
    sw("ALMIDE_UPDATE_INTERP_LEDGER", Flag, Harness, "regenerate the interpreter abstain and bridge-fallback ledgers"),
    sw("ALMIDE_UPDATE_NATIVE_OWN", Flag, Harness, "regenerate the native result-ownership ledger"),
    sw("ALMIDE_UPDATE_RC_SNAPSHOTS", Flag, Harness, "regenerate the rc-placement snapshots"),
    sw("ALMIDE_UPDATE_SIZES", Flag, Harness, "regenerate the structural leg's size baselines"),
    sw("ALMIDE_UPDATE_SIZE_LADDER", Flag, Harness, "regenerate the stdlib-linking size ladder ledger (#2141)"),
    sw("ALMIDE_UPDATE_SNAPSHOTS", Flag, Tool, "same as `almide test --update-snapshots`"),
    sw("ALMIDE_UPDATE_SURFACE", Flag, Harness, "regenerate the exercised-surface golden"),
    sw("ALMIDE_UPDATE_WITNESS_FLOOR", Flag, Harness, "regenerate the certificate witness floor"),
    sw("ALMIDE_VERBOSE", Flag, Dbg, "same as `almide -v`: surface the native wall-and-fallback notes that a quiet run hides"),
    sw("ALMIDE_VERIFIED_DEBUG", Flag, Dbg, "name the wasm leg that rendered, and why the other declined (the route oracle)"),
    sw("ALMIDE_VERSION_LINE", Value, Ci, "NOT read from the environment at run time: `build.rs` EMITS it as `cargo:rustc-env`, and `src/main.rs` reads it with `env!` at compile time. It is the string `almide --version` prints — `<version> (<kind>[, <sha>])` — assembled from ALMIDE_BUILD_PROVENANCE and ALMIDE_BUILD_SHA (#2384)"),
    sw("ALMIDE_WALL_REASON", Flag, Dbg, "make `almide test` say WHICH stage of the wasm leg declined a fallback file, not just `v1 wall`"),
    sw("ALMIDE_WASM_ALLOC_COUNT", Flag, Harness, "emit the structural wasm leg's allocation counters (#2407: four i64 globals `$alloc`/`$free` bump, exported as `__alloc_count` / `__alloc_reused` / `__alloc_bytes` / `__free_count`) and make `almide run --target wasm` print `__ALMD_WASM_ALLOC allocs=N reused=N bytes=N frees=N heap_end=N` on stderr after the run; off, the module is byte-identical to a build without the switch (the wasm twin of `ALMIDE_ALLOC_COUNT`; the count ledger is crates/almide-wasm/tests/golden/alloc-count-baseline.txt)"),
    sw("ALMIDE_WASM_FREES", Flag, Ci, "the frees-churn gate's switch; its compiler reader retired with the v0 emitter (#782), the gate that still sets it is #2207's"),
    sw("ALMIDE_WASM_INCUMBENT", Flag, Route, "force the INCUMBENT wasm leg (the v1 MIR renderer) instead of the structural-first route"),
    sw("ALMIDE_WASM_STRUCTURAL", Flag, Route, "force the STRUCTURAL wasm leg for a shape the router would send to the incumbent (the route-flip probe)"),
    sw("ALMIDE_WAT_PRELUDE_REACH", Flag, Ci, "make the prelude audit re-render every named fixture to measure reachability (CI sets it)"),
    sw("ALMIDE_WITNESS_DUMP", Flag, Harness, "print every fixture's certificate witness in the witness-floor test"),
    sw("ALMIDE_WRITE_FUZZ_CORPUS", Flag, Harness, "write the generated fuzz programs to disk"),
];

/// The registered switch named `name`.
pub fn switch(name: &str) -> Option<&'static Switch> {
    SWITCHES.iter().find(|s| s.name == name)
}

/// The ONE boolean semantics: a set switch is OFF when its value is empty or
/// spells no — `0`, `false`, `off`, `no` (any case) — and ON otherwise.
pub fn is_on(value: &std::ffi::OsStr) -> bool {
    let v = value.to_string_lossy();
    let v = v.trim();
    !(v.is_empty() || v.eq_ignore_ascii_case("0") || v.eq_ignore_ascii_case("false")
        || v.eq_ignore_ascii_case("off") || v.eq_ignore_ascii_case("no"))
}

/// Is the flag `name` on? Registered names only — an unregistered name is a
/// programming error and panics, the roster being what makes the set
/// enumerable (`scripts/check-env-switches.sh` catches it before a build). A
/// route-forcing or gate-bypassing switch found ON is announced once on
/// stderr, so a verdict produced under it says so.
pub fn flag(name: &str) -> bool {
    let sw = registered(name);
    let on = std::env::var_os(name).is_some_and(|v| is_on(&v));
    if on {
        announce(sw);
    }
    on
}

/// The value of `name`, when set and non-empty. Registered names only; a
/// route/gate switch is announced like [`flag`].
pub fn var(name: &str) -> Option<String> {
    let sw = registered(name);
    let v = std::env::var(name).ok().filter(|v| !v.trim().is_empty())?;
    announce(sw);
    Some(v)
}

fn registered(name: &str) -> &'static Switch {
    match switch(name) {
        Some(sw) => sw,
        None => panic!("`{name}` is not in almide_base::env::SWITCHES — register it before reading it"),
    }
}

/// The `ALMIDE_*` names the process INHERITED from its environment, recorded by
/// [`inherit`] at process start. A switch the tool sets on itself later
/// (`almide test` arms `ALMIDE_NO_RTLIB` for its harness build, `--time-report`
/// arms `ALMIDE_FUEL_PROBE`) is the tool's own decision, not a forced route the
/// log has to disclose — only an inherited one is announced.
static INHERITED: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// Record which switches the process was started with. Call once, first thing
/// in `main`; a process that never calls it (a library consumer, a test)
/// announces every on switch instead.
pub fn inherit() {
    let names: Vec<String> = std::env::vars_os()
        .filter_map(|(k, _)| k.into_string().ok())
        .filter(|k| k.starts_with("ALMIDE_"))
        .collect();
    let _ = INHERITED.set(names);
}

/// The once-per-process stderr line for a Route / Gate switch that is on.
fn announce(sw: &'static Switch) {
    use std::sync::Mutex;
    static SAID: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    if !matches!(sw.scope, Scope::Route | Scope::Gate) {
        return;
    }
    if INHERITED.get().is_some_and(|names| !names.iter().any(|n| n == sw.name)) {
        return;
    }
    let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
    if said.contains(&sw.name) {
        return;
    }
    said.push(sw.name);
    eprintln!("[almide] {} is set: {}", sw.name, sw.doc);
}

impl Scope {
    /// The scope's name in the tables.
    pub fn label(self) -> &'static str {
        match self {
            Scope::Route => "route",
            Scope::Gate => "gate",
            Scope::Ablation => "ablation",
            Scope::Trap => "trap",
            Scope::Debug => "debug",
            Scope::Tool => "tool",
            Scope::Runtime => "runtime",
            Scope::Harness => "harness",
            Scope::Ci => "ci",
        }
    }
}

impl Switch {
    /// `NAME` for a flag, `NAME=value` for a value switch.
    pub fn spelling(&self) -> String {
        match self.kind {
            Kind::Flag => self.name.to_string(),
            Kind::Value => format!("{}=value", self.name),
        }
    }
}

/// The docs table (`docs/specs/cli.md`'s generated block), one row per switch.
pub fn markdown_table() -> String {
    let mut out = String::from("| 変数 | 種別 | 説明 |\n|---|---|---|\n");
    for s in SWITCHES {
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            s.spelling(),
            s.scope.label(),
            s.doc.replace('|', "\\|")
        ));
    }
    out
}

/// The `almide switches` listing: one line per switch, aligned.
pub fn plain_table() -> String {
    let width = SWITCHES.iter().map(|s| s.spelling().len()).max().unwrap_or(0);
    let mut out = String::new();
    for s in SWITCHES {
        out.push_str(&format!("{:<width$}  {:<8} {}\n", s.spelling(), s.scope.label(), s.doc));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_roster_is_sorted_and_unique() {
        for w in SWITCHES.windows(2) {
            assert!(w[0].name < w[1].name, "{} must come before {}", w[1].name, w[0].name);
        }
    }

    #[test]
    fn every_name_carries_the_prefix_and_a_doc() {
        for s in SWITCHES {
            assert!(s.name.starts_with("ALMIDE_"), "{}", s.name);
            assert!(!s.doc.is_empty(), "{} has no doc", s.name);
        }
    }

    #[test]
    fn one_boolean_semantics() {
        use std::ffi::OsStr;
        for off in ["", "0", "false", "FALSE", "off", "No", "  "] {
            assert!(!is_on(OsStr::new(off)), "{off:?} must be off");
        }
        for on in ["1", "true", "yes", "anything", " 1 "] {
            assert!(is_on(OsStr::new(on)), "{on:?} must be on");
        }
    }
}
