//! The `almide` command line as clap reads it: the top-level flags, every
//! subcommand and its options, and the `ide` subcommands.

use clap::{Parser, Subcommand};

/// What `almide --version` prints: the version number AND which kind of build
/// produced it (#2384). `version` alone prints `CARGO_PKG_VERSION`, which
/// answers what Cargo.toml says rather than which compiler this is — see
/// `build.rs`'s `emit_version_line` for why those are different questions and
/// what it cost to learn. The second whitespace-separated field is still the
/// bare version, which `Makefile`'s install assertion reads.
const VERSION_LINE: &str = env!("ALMIDE_VERSION_LINE");

#[derive(Parser)]
#[command(name = "almide", version = VERSION_LINE)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
    /// Show internal pipeline notes (e.g. which codegen path built the binary)
    #[arg(short = 'v', long = "verbose", global = true)]
    pub(crate) verbose: bool,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Create a new Almide project
    Init,
    /// Compile and execute
    #[command(trailing_var_arg = true)]
    Run {
        /// Source file (default: src/main.almd)
        file: Option<String>,
        /// Skip type checking
        #[arg(long)]
        no_check: bool,
        /// Build with optimisations (cargo --release). Required for any
        /// performance-sensitive comparison; without it the generated
        /// Rust runs in dev profile.
        #[arg(long)]
        release: bool,
        /// Execution target: `rust` (default, native binary) or `wasm`
        /// (build a wasm32-wasi module and execute it on the `wasmtime`
        /// CLI). Both targets must produce byte-identical observable
        /// behavior — the cross-target equivalence guarantee.
        #[arg(long)]
        target: Option<String>,
        /// The v1 PCC-verified trust-spine renderer is the DEFAULT on BOTH
        /// targets (wasm since 0.29.0; native since 0.30.0). On wasm it is the
        /// ONLY path (the v0 emitter was retired in #782 — a wall is a hard,
        /// diagnosed error); on native, v1 renders first and classic codegen
        /// source is the fallback on a wall. `--verified` is kept as an
        /// accepted no-op for compatibility.
        #[arg(long)]
        verified: bool,
        /// Removed (#782): the legacy v0 escape hatch is a hard error. Only
        /// the sanctioned oracle harnesses may set ALMIDE_NO_VERIFIED_OK=1.
        #[arg(long)]
        no_verified: bool,
        /// Print the ADR-0001 D5 dual-time line after the run: the program's
        /// DETERMINISTIC time (charge units × CM-1, identical on every
        /// machine) next to the measured wall clock on this host.
        #[arg(long)]
        time_report: bool,
        /// Arguments passed to the program. Almide's own flags (`--target`,
        /// `--no-check`, `--release`) are consumed before these; anything
        /// after a `--` separator is forwarded verbatim to the program.
        #[arg(allow_hyphen_values = true)]
        program_args: Vec<String>,
    },
    /// Benchmark a program: verify-then-time, median headline (#1490)
    Bench {
        /// Source file (default: src/main.almd)
        file: Option<String>,
        /// Timed runs after the warmup (default 5)
        #[arg(long, default_value_t = 5)]
        runs: u32,
        /// Leg to time: native (default, release binary) or wasm
        /// (embedded host)
        #[arg(long)]
        target: Option<String>,
        /// Arguments passed to the program on both legs, after a `--`
        /// separator (a workload size, so a row's `main` is long enough to
        /// time: `almide bench nbody.almd -- 200000`).
        #[arg(last = true)]
        program_args: Vec<String>,
    },
    /// Build a binary
    Build {
        /// Source file (default: src/main.almd)
        file: Option<String>,
        /// Output file name
        #[arg(short)]
        o: Option<String>,
        /// Build target: rust (default, this host), wasm, linux-musl
        /// (<host arch>-unknown-linux-musl), or a rustc target triple.
        /// Without it, an inherited CARGO_BUILD_TARGET is honoured.
        #[arg(long)]
        target: Option<String>,
        /// Optimize for performance (opt-level=2)
        #[arg(long)]
        release: bool,
        /// Maximum performance: native CPU, fast-math, opt-level=3, LTO
        #[arg(long)]
        fast: bool,
        /// Use unchecked index access (unsafe, no bounds checking)
        #[arg(long)]
        unchecked_index: bool,
        /// Skip type checking
        #[arg(long)]
        no_check: bool,
        /// Add #[repr(C)] to structs/enums for stable C ABI
        #[arg(long)]
        repr_c: bool,
        /// Build as shared library (.dylib/.so) instead of executable
        #[arg(long)]
        cdylib: bool,
        /// Emit the WASM module even if the Perceus RC gate fails (waiver for a
        /// known compiler-RC bug; the artifact may leak memory). Without this a
        /// verification failure is a hard error.
        #[arg(long)]
        emit_unverified: bool,
        /// The v1 PCC-verified trust-spine renderer is the DEFAULT on BOTH
        /// targets (wasm since 0.29.0; native since 0.30.0). On wasm it is the
        /// ONLY path (the v0 emitter was retired in #782 — a wall is a hard,
        /// diagnosed error); on native, v1 renders first and classic codegen
        /// source is the fallback on a wall. The wasm module ships VERBATIM
        /// (wasm-opt skipped unless --wasm-opt). This flag is a no-op kept
        /// for compatibility.
        #[arg(long)]
        verified: bool,
        /// Removed (#782): the legacy v0 escape hatch is a hard error. Only
        /// the sanctioned oracle harnesses may set ALMIDE_NO_VERIFIED_OK=1.
        #[arg(long)]
        no_verified: bool,
        /// Run `wasm-opt -Oz` on the wasm output after the verified renderer
        /// produces it. This is an explicit opt-in that LEAVES the verified
        /// envelope: wasm-opt is an external, unverified transform, so the
        /// shipped bytes are no longer the exact bytes the trust-spine
        /// rendered (see docs/wasm/WASM-OUTPUT.md). Default off — without this
        /// flag the module ships verbatim. No-op on the native target.
        #[arg(long = "wasm-opt")]
        wasm_opt: bool,
        /// Package the wasm output as a WASI 0.2 COMPONENT (#1628 stage 0):
        /// the core module is wrapped with the pinned
        /// wasi_snapshot_preview1 adapter via wit-component, so the artifact
        /// runs anywhere components run (wasmtime, jco, wasmCloud). Sync
        /// surface only — the fan/async component target is #1628 stage 2.
        #[arg(long = "component")]
        component: bool,
        /// Bake a hard heap ceiling (bytes) into the built artifact (#1530).
        /// Exceeding it is the DEFINED "Error: out of memory" abort (exit 1)
        /// on both targets: the wasm bump frontier checks the cap in $alloc,
        /// and the native binary counts live bytes in a wrapping global
        /// allocator. A leak-harness knob — a leak becomes a deterministic
        /// OOM at the boundary instead of an invisible slow bloat. Absent or
        /// 0 = no ceiling, and the output is byte-identical to a build
        /// without the flag.
        #[arg(long = "heap-cap")]
        heap_cap: Option<u32>,
        /// Write a JS host next to the wasm output (#2265): `--host js`
        /// emits `<mod>.js` (a dependency-free ES module with `init()`,
        /// `run()` and one wrapper per `pub fn`) and `<mod>.d.ts`. The
        /// `@extern(wasm, "js", ...)` imports are wired through
        /// `init({ js: { name } })`; Int/Float/Bool/String/Unit are
        /// marshalled, anything else is refused at build time.
        #[arg(long = "host")]
        host: Option<String>,
        /// `--target wasm` only: append DWARF (`.debug_line`, `.debug_info`,
        /// …) custom sections mapping code offsets to `.almd` file:line
        /// (#1315), read by Chrome DevTools and lldb on wasmtime. Off by
        /// default: without it the module is byte-identical.
        #[arg(long)]
        debug: bool,
    },
    /// Run tests
    Test {
        /// Test file
        file: Option<String>,
        /// Filter test names by pattern
        #[arg(short = 'r', long)]
        run: Option<String>,
        /// Skip type checking
        #[arg(long)]
        no_check: bool,
        /// Output test results as JSON (one per line)
        #[arg(long)]
        json: bool,
        /// Target: wasm (wasmtime)
        #[arg(long)]
        target: Option<String>,
        /// Accept snapshot drift: rewrite each failing `testing.assert_snapshot`
        /// expectation in place, then re-run (also ALMIDE_UPDATE_SNAPSHOTS=1)
        #[arg(long)]
        update_snapshots: bool,
        /// CI mode: snapshots are never written, drift and new snapshots fail (also CI=true)
        #[arg(long)]
        ci: bool,
        /// Exit 0 instead of 5 when the run has no tests to execute
        #[arg(long)]
        allow_no_tests: bool,
        /// Print what every test wrote to stdout/stderr, passing ones included
        /// (a failing test's output is always shown under its failure)
        #[arg(long)]
        show_output: bool,
    },
    /// Type check only
    Check {
        /// Source file. Omitted inside a package: every `.almd` under `src/`
        /// is checked and named (#2165)
        file: Option<String>,
        /// Treat warnings as errors
        #[arg(long)]
        deny_warnings: bool,
        /// Output diagnostics as JSON (one per line)
        #[arg(long)]
        json: bool,
        /// Explain an error code (e.g., --explain E001)
        #[arg(long)]
        explain: Option<String>,
        /// Show effect/capability analysis for each function
        #[arg(long)]
        effects: bool,
        /// Report front-end wall time split by phase (lex / parse / check) plus
        /// a machine-readable `almide-timings {...}` line (#1311)
        #[arg(long)]
        timings: bool,
        /// On a clean check, advance the file's `@dialect(N)` stamp to this
        /// compiler's dialect (writing one if absent). Forward only: a stamp
        /// from a newer compiler is left alone. A successful check IS the
        /// verification the stamp records, which is why this lives here and
        /// not in `fmt`.
        #[arg(long)]
        stamp: bool,
        /// Check under a named profile. `critical` (#567) applies the bounded
        /// profile (ALS §B, E070–E078) to EVERY function — no `@bounded`
        /// attribute needed — with capabilities starting deny-all. A subset,
        /// not a dialect: critical-valid code is always valid in normal mode.
        #[arg(long)]
        profile: Option<String>,
        /// Grant a capability under `--profile critical` (repeatable):
        /// IO, Net, Env, Time, Rand, Process
        #[arg(long)]
        allow: Vec<String>,
        /// Also decide the wasm build route (#1922): after a clean check, run
        /// the same two-leg routing `build --target wasm` uses and report
        /// E081 / E082 at check time instead of at build time. Only `wasm`.
        #[arg(long)]
        target: Option<String>,
    },
    /// Start the Language Server Protocol server (for editor integration)
    Lsp,
    /// Start the Model Context Protocol server on stdio (for agent/LLM clients).
    /// Exposes check / test / API-outline / explain / fmt-check as typed tools
    /// with JSON results — the same answers as the CLI, minus the step where a
    /// model has to parse human-formatted text.
    Mcp,
    /// Explain a diagnostic code (e.g., almide explain E001), or list every code
    Explain {
        /// Diagnostic code such as E001
        #[arg(required_unless_present = "list")]
        code: Option<String>,
        /// List every diagnostic code: code, severity, since, fix-it verdict, title
        #[arg(long, conflicts_with = "code")]
        list: bool,
        /// With --list: one JSON array of {code, mnemonic, severity, since, verdict}
        #[arg(long, requires = "list")]
        json: bool,
    },
    /// Format source files
    Fmt {
        /// Files to format (default: src/**/*.almd)
        files: Vec<String>,
        /// Check formatting without writing (exit non-zero on drift)
        #[arg(long)]
        check: bool,
        /// Machine-readable `--check`: one JSON object naming the files that
        /// need formatting (implies --check; still exits non-zero on drift)
        #[arg(long)]
        json: bool,
        /// Print the formatted text to stdout without writing
        #[arg(long)]
        dry_run: bool,
        /// Keep the import list byte-for-byte (no auto-insert of missing
        /// imports, no removal of unused ones) — for splice-context sources
        /// like the stdlib, where an added import corrupts the splice
        #[arg(long)]
        no_import_edit: bool,
    },
    /// Compile source to .almdi (module interface + IR artifact)
    Compile {
        /// Module name (e.g., "json", "parser") or file path; defaults to project
        module: Option<String>,
        /// Output interface as machine-readable JSON (no artifact)
        #[arg(long)]
        json: bool,
        /// Print human-readable interface (no artifact)
        #[arg(long)]
        dry_run: bool,
        /// Output directory for .almdi files (default: target/compile)
        #[arg(long, short)]
        output: Option<String>,
    },
    /// Advance a locked git dependency to its ref's current remote head
    Update {
        /// Dependency name (default: every non-tag-pinned dependency)
        dep: Option<String>,
    },
    /// Clear the dependency cache and the native build scratch dirs
    Clean,
    /// Add a dependency
    Add {
        /// Package specifier
        pkg: String,
        /// Git repository URL
        #[arg(long)]
        git: Option<String>,
        /// Git tag
        #[arg(long)]
        tag: Option<String>,
        /// The package's directory inside the repository (a repository
        /// holding several packages); the package is named after its last
        /// component
        #[arg(long)]
        subdir: Option<String>,
    },
    /// List dependencies
    Deps,
    /// Print the cached source directory of a dependency
    DepPath {
        /// Dependency name (as declared in almide.toml)
        name: String,
    },
    /// Install an Almide CLI from a git repo into ~/.local/bin (or
    /// $ALMIDE_INSTALL). Like `go install`: clone, build --release,
    /// drop the binary on PATH.
    Install {
        /// Package spec: `github.com/<owner>/<repo>`, a full git URL,
        /// or a local path
        spec: String,
        /// Git tag to install (default: latest commit on the default branch)
        #[arg(long)]
        tag: Option<String>,
        /// Git branch
        #[arg(long)]
        branch: Option<String>,
        /// Override the binary name (default: [package].name from almide.toml)
        #[arg(long)]
        name: Option<String>,
        /// Override the install directory (default: $ALMIDE_INSTALL or ~/.local/bin)
        #[arg(long = "bin-dir")]
        bin_dir: Option<std::path::PathBuf>,
        /// Build target (default: native)
        #[arg(long)]
        target: Option<String>,
    },
    /// Update almide to the latest version
    #[command(name = "self-update")]
    SelfUpdate {
        /// Target version (e.g., v0.13.0); defaults to latest
        version: Option<String>,
    },
    /// Re-check a program's flight-grade certificates with the independent
    /// `almide-verify` binary (found next to almide, else on PATH; there is no
    /// built-in fallback). `almide verify app.almd [--emit out.bundle]`
    /// produces the certificate bundle and hands it over; any other arguments
    /// go to almide-verify verbatim (e.g. `almide verify ownership w.cert`).
    Verify {
        /// `<file.almd> [--emit <bundle>]`, or arguments for almide-verify
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Agent/LLM semantic queries (outline, doc, stdlib-snapshot)
    Ide {
        #[command(subcommand)]
        cmd: IdeCommand,
    },
    /// Apply mechanically-safe fixes to a file (auto-import; reports remaining
    /// try: snippets for manual application).
    Fix {
        /// Source file (default: src/main.almd)
        file: Option<String>,
        /// Show what would change without modifying the file
        #[arg(long)]
        dry_run: bool,
        /// Emit a machine-readable JSON report (for harness integration)
        #[arg(long)]
        json: bool,
    },
    /// Judge a proposed edit BEFORE writing it: apply it in memory, run check,
    /// the tests that reach the file and its contract fixtures on both sides,
    /// and report each diagnostic / test / contract as unchanged, newly_broken,
    /// newly_fixed or removed. Never writes. Exit 0 = survives, 1 = does not,
    /// 2 = the edit could not be judged.
    Survive {
        /// The .almd file the edit is to
        file: String,
        /// The edit: a path (or `-` for stdin) holding a unified diff or the
        /// file's full new text
        #[arg(long)]
        with: String,
        /// How to read `--with`: auto (a diff if it starts `--- ` / `@@ `), patch, text
        #[arg(long = "as", default_value = "auto")]
        as_kind: String,
        /// Emit the survival delta as JSON (schema_version 1)
        #[arg(long)]
        json: bool,
        /// Per-run time limit in seconds for every child check / test / run
        #[arg(long, default_value_t = 600)]
        timeout: u64,
    },
    /// Verify-then-write: run `survive` on the edit and write it (atomically)
    /// only if nothing is newly broken. `--force` writes whatever the verdict.
    Apply {
        /// The .almd file to edit
        file: String,
        /// The edit: a path (or `-` for stdin) holding a unified diff or the
        /// file's full new text
        #[arg(long)]
        with: String,
        /// How to read `--with`: auto, patch, text
        #[arg(long = "as", default_value = "auto")]
        as_kind: String,
        /// Write only if the edit survives. `true` / `false` only — any other
        /// value refuses without writing
        #[arg(long = "if-survives", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
        if_survives: Option<String>,
        /// Write even if the edit does not survive (the verdict is still reported)
        #[arg(long)]
        force: bool,
        /// Emit the survival delta (plus `written`) as JSON
        #[arg(long)]
        json: bool,
        /// Per-run time limit in seconds for every child check / test / run
        #[arg(long, default_value_t = 600)]
        timeout: u64,
    },
    /// Internal to `almide survive`: compile one file's test harness and run it
    /// captured, printing `{compiled, exit_code, output}` as one JSON line.
    #[command(name = "survive-test-leg", hide = true)]
    SurviveTestLeg {
        file: String,
    },
    /// Check canonical docs (llms.txt, etc.) against source-of-truth inputs
    /// (Cargo version, diagnostic code inventory, stdlib auto-import list).
    /// Fails CI when drift is detected.
    #[command(name = "docs-gen")]
    DocsGen {
        /// Verify mode: exit 1 if any drift is found, 0 otherwise.
        #[arg(long)]
        check: bool,
    },
    /// Emit source code or AST
    #[command(hide = true)]
    Emit {
        /// Source file
        file: String,
        /// Target language (rust, wgsl)
        #[arg(long, default_value = "rust")]
        target: String,
        /// Emit AST as JSON
        #[arg(long)]
        emit_ast: bool,
        /// Emit typed IR as JSON
        #[arg(long)]
        emit_ir: bool,
        /// Emit Almide dialect (MLIR-like textual form)
        #[arg(long)]
        emit_dialect: bool,
        /// Skip type checking
        #[arg(long)]
        no_check: bool,
        /// Add #[repr(C)] to structs/enums for stable C ABI
        #[arg(long)]
        repr_c: bool,
        /// #572: emit `// almd: fn <name> @ line <N>` anchors on every
        /// rendered function and write the sidecar traceability map
        /// (`<file>.trace.json`) — the source-to-generated correspondence
        /// a third-party review aligns on. Default off: the plain emission
        /// stays byte-identical to the baselines.
        #[arg(long = "trace-map")]
        trace_map: bool,
    },
    /// List the `ALMIDE_*` environment switches the tree reads — the registry
    /// (`almide_base::env::SWITCHES`, #2205), enumerable at run time.
    #[command(hide = true)]
    Switches {
        /// Render the markdown table `docs/specs/cli.md` embeds (its generated block).
        #[arg(long)]
        md: bool,
    },
}

#[derive(clap::Subcommand, Debug)]
pub(crate) enum IdeCommand {
    /// Print one-line summary of each public decl (fn / type / let).
    /// Use this instead of `grep` to discover a package's API.
    /// Accepts a file path or `@stdlib/<module>` (e.g. `@stdlib/string`).
    Outline {
        /// Source file or `@stdlib/<module>` (default: src/main.almd)
        target: Option<String>,
        /// Filter to a substring (e.g. `upper` or `to_`)
        #[arg(long)]
        filter: Option<String>,
        /// Emit JSON instead of one-line text
        #[arg(long)]
        json: bool,
    },
    /// Show signature + doc for a symbol. Accepts `string.to_upper`, `list.fold`,
    /// or a bare user-defined name.
    Doc {
        /// Symbol name (e.g. `string.to_upper`)
        symbol: String,
        /// File context (default: src/main.almd — used for user-defined lookup)
        #[arg(long)]
        file: Option<String>,
    },
    /// Dump concatenated stdlib outlines in one call.
    /// Intended for LLM harnesses that embed a stdlib API inventory in SYSTEM_PROMPT.
    /// Default modules: string, list, int, option, result, map, set.
    StdlibSnapshot {
        /// Comma-separated module list (e.g. `string,list,int`). Defaults to the core set.
        #[arg(long)]
        modules: Option<String>,
        /// Emit JSON array instead of concatenated text sections
        #[arg(long)]
        json: bool,
    },
}
