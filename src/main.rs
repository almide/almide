mod cli;
mod compile_driver;

// Bring library modules into binary crate scope so cli/ submodules can use `crate::*`.
pub use almide::{
    ast, canonicalize, check, codegen, diagnostic, diagnostic_render, fmt,
    import_table, intern, ir, lexer, lower, mono, optimize,
    parser, project, project_fetch, resolve, stdlib, types,
    out, out_no_nl, err, err_no_nl, WASM_WALL_MARKER,
};

// `cli/*.rs` call these via `crate::<name>` — re-exported here (private,
// visible to descendants of the crate root) so those call sites don't need
// to change after the `compile_driver` split.
use compile_driver::{parse_file, try_compile, register_versioned_module_names, lower_one_user_module, try_compile_with_ir};

use std::process::Command;

/// When true, suppress warning output during compilation (used by REPL).
pub(crate) static SUPPRESS_WARNINGS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn warnings_suppressed() -> bool {
    SUPPRESS_WARNINGS.load(std::sync::atomic::Ordering::Relaxed)
}
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "almide", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    /// Show internal pipeline notes (e.g. which codegen path built the binary)
    #[arg(short = 'v', long = "verbose", global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
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
    /// Build a binary
    Build {
        /// Source file (default: src/main.almd)
        file: Option<String>,
        /// Output file name
        #[arg(short)]
        o: Option<String>,
        /// Build target (wasm)
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
    },
    /// Type check only
    Check {
        /// Source file (default: src/main.almd)
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
    },
    /// Start the Language Server Protocol server (for editor integration)
    Lsp,
    /// Start the Model Context Protocol server on stdio (for agent/LLM clients).
    /// Exposes check / test / API-outline / explain / fmt-check as typed tools
    /// with JSON results — the same answers as the CLI, minus the step where a
    /// model has to parse human-formatted text.
    Mcp,
    /// Explain a diagnostic code (e.g., almide explain E001)
    Explain {
        /// Diagnostic code such as E001
        code: String,
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
    /// Clear dependency cache
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
}

#[derive(clap::Subcommand, Debug)]
enum IdeCommand {
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

fn find_rustc() -> String {
    if Command::new("rustc").arg("--version").output().is_ok() {
        return "rustc".to_string();
    }
    if let Some(home) = std::env::var_os("HOME") {
        let cargo_rustc = std::path::PathBuf::from(home).join(".cargo/bin/rustc");
        if cargo_rustc.exists() {
            return cargo_rustc.to_string_lossy().to_string();
        }
    }
    "rustc".to_string()
}

fn collect_almd_files(dir: &std::path::Path, out: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                collect_almd_files(&path, out);
            } else if path.extension().map_or(false, |ext| ext == "almd") {
                out.push(path.to_string_lossy().to_string());
            }
        }
    }
}


/// #782 retirement step 2: `--no-verified` (the legacy v0 escape hatch) is now a HARD
/// ERROR — v0.30.0 shipped one release of the deprecation notice, and the v0 emitters
/// are build-only CI parity oracles from here. `ALMIDE_NO_VERIFIED_OK=1` keeps the flag
/// working for the sanctioned oracle harnesses (org-byte-verify / frees-churn / the
/// differential tests), whose v0 invocations ARE the parity gate, not user escapes.
fn warn_no_verified_deprecated(no_verified: bool) {
    if no_verified && std::env::var_os("ALMIDE_NO_VERIFIED_OK").is_none() {
        err(&format!(
            "error: --no-verified (the legacy v0 codegen path) has been removed; the \
             verified renderer is byte-identical where it lowers and falls back to v0 \
             automatically, so the flag should never be needed. If a program genuinely \
             needs it, file an issue: https://github.com/almide/almide/issues"
        ));
        std::process::exit(1);
    }
}

fn resolve_file(file: Option<String>) -> String {
    file.unwrap_or_else(|| {
        if std::path::Path::new("almide.toml").exists() {
            // Early-validate package name before looking for entry point
            match crate::project::parse_toml(std::path::Path::new("almide.toml")) {
                Err(e) if e.contains("hyphens") => {
                    err(&format!("error: {}", e));
                    std::process::exit(1);
                }
                _ => {}
            }
            // src/mod.almd = package entry point, src/main.almd = executable entry point
            for entry in &["src/mod.almd", "src/main.almd"] {
                if std::path::Path::new(entry).exists() {
                    return entry.to_string();
                }
            }
            err(&format!("almide.toml found but no entry point (src/mod.almd or src/main.almd)."));
            err(&format!("Create src/mod.almd (library) or src/main.almd (executable)."));
            std::process::exit(1);
        } else {
            err(&format!("No file specified and no almide.toml found."));
            err(&format!("Run 'almide init' to create a project, or specify a file."));
            std::process::exit(1);
        }
    })
}

// The `docs/diagnostics/<CODE>.md` reference texts, embedded at build time
// (generated by build.rs — see `embed_diagnostic_docs`). This used to be a disk
// probe (a 6-level parent walk from the executable, plus an
// `ALMIDE_DIAGNOSTICS_DIR` override) backed by a hardcoded E001–E010 fallback
// table: from a checkout every code explained, from an installed binary 21 of
// the documented codes answered "Unknown error code" — while
// docs/diagnostics/README.md told users to run exactly this command (#923). The
// embedded table is the binary-alone delivery, so both the walk and the
// fallback (a hand-maintained SECOND copy of ten docs, drifted from the real
// ones by construction) are gone.
include!(concat!(env!("OUT_DIR"), "/diagnostic_docs.rs"));

fn print_error_explanation(code: &str) {
    // `e005` means E005 — the old disk probe answered that only on
    // case-insensitive filesystems (macOS yes, Linux no), which is exactly the
    // kind of host-dependent behavior the embedded table exists to end.
    let code = code.to_ascii_uppercase();
    match DIAGNOSTIC_DOCS.iter().find(|(k, _)| *k == code) {
        Some((_, doc)) => out(&format!("{}", doc)),
        None => {
            err(&format!("Unknown error code: {}", code));
            std::process::exit(1);
        }
    }
}

/// The parse → check → lower → emit pipeline is deeply recursive: AST and IR
/// walks, and type-directed codegen (e.g. `emit_eq_typed` / `emit_ord_cmp3`,
/// container-literal emission) recurse with the shape of the input program. A
/// sufficiently deep or wide machine-generated expression can therefore exhaust
/// the OS's *default* main-thread stack — which is platform-dependent: ~8 MiB on
/// Linux/macOS but only ~1 MiB on Windows. That made compilation depth a silent
/// cross-platform divergence (a program that compiled on Unix overflowed the
/// stack mid-codegen on Windows). We follow the same strategy production
/// compilers use (rustc spawns its driver on an enlarged thread): run the whole
/// driver on a worker thread with a large, fixed stack, so the achievable
/// recursion depth is bounded by heap and identical on every host. The size is a
/// virtual reservation — pages are committed lazily as the stack grows — so it
/// costs no physical memory for shallow programs.
///
/// Overridable via `ALMIDE_COMPILER_STACK` (bytes), the analogue of rustc's
/// `RUST_MIN_STACK`: a non-numeric or empty value falls back to the default. The
/// override exists mainly so a regression test can pin a deliberately small
/// stack and prove that compilation stays within bounded native stack on wide /
/// deep input (see `tests/compiler_stack_test.rs`).
const COMPILER_STACK_SIZE: usize = 256 * 1024 * 1024; // 256 MiB

fn compiler_stack_size() -> usize {
    match std::env::var("ALMIDE_COMPILER_STACK") {
        Ok(v) => v.trim().parse::<usize>().ok().filter(|n| *n > 0).unwrap_or(COMPILER_STACK_SIZE),
        Err(_) => COMPILER_STACK_SIZE,
    }
}

fn main() {
    let child = std::thread::Builder::new()
        .name("almide-main".to_string())
        .stack_size(compiler_stack_size())
        .spawn(run_main)
        .expect("failed to spawn the compiler driver thread");
    // A panic on the worker thread has already printed via the default hook;
    // mirror the main-thread panic exit code (101) so behavior is unchanged.
    if child.join().is_err() {
        std::process::exit(101);
    }
}

fn run_main() {
    crate::diagnostic_render::init_color();
    // Legacy mode: `almide file.almd [--target X]` → rewrite as `almide emit file.almd [--target X]`
    let raw_args: Vec<String> = std::env::args().collect();
    if raw_args.len() >= 2
        && !raw_args[1].starts_with('-')
        && (raw_args[1].ends_with(".almd") || raw_args[1].ends_with(".json"))
    {
        let mut new_args = vec![raw_args[0].clone(), "emit".to_string()];
        new_args.extend_from_slice(&raw_args[1..]);
        let cli = Cli::parse_from(new_args);
        dispatch(cli);
        return;
    }

    let cli = Cli::parse();
    dispatch(cli);
}

/// `dispatch`'s `Commands::Run` arm. Extracted verbatim.
fn dispatch_run(file: Option<String>, no_check: bool, release: bool, target: Option<String>, no_verified: bool, time_report: bool, program_args: Vec<String>) {
    let file = resolve_file(file);
    if time_report {
        // The deterministic meter is the probe machinery: setting the env here
        // (before any compile thread exists) makes the pipeline insert charge
        // ops, and the run leg formats the probe line into the D5 dual-time
        // report instead of printing it raw.
        // SAFETY: single-threaded at this point (process entry, pre-dispatch).
        unsafe { std::env::set_var("ALMIDE_FUEL_PROBE", "1") };
    }
    // 0.29.0: v1-first verified wasm is the DEFAULT; `--no-verified` opts out.
    // 0.30.0 (#764 rung-5 complete): the v1 NATIVE trust-spine renderer is
    // likewise the DEFAULT (byte-identical to v0 where it lowers — the
    // differential rows + the 18/18 wasm_cross native byte sweep — and an
    // honest wall falls back to v0). `--no-verified` opts out of BOTH legs;
    // `--verified` is kept as a no-op for compatibility.
    warn_no_verified_deprecated(no_verified);
    cli::cmd_run(cli::RunArgs {
        file: &file,
        program_args: &program_args,
        no_check,
        release,
        target: target.as_deref(),
        verified: !no_verified,
        native_verified: !no_verified,
        time_report,
    });
}

/// `dispatch`'s `Commands::Test` arm. Extracted verbatim.
fn dispatch_test(file: Option<String>, run: Option<String>, no_check: bool, json: bool, target: Option<String>) {
    let file_str = file.as_deref().unwrap_or("");
    if target.as_deref() == Some("wasm") {
        cli::cmd_test_wasm(file_str, run.as_deref());
    } else if json {
        cli::cmd_test_json(file_str, run.as_deref());
    } else if matches!(target.as_deref(), Some("rust" | "native")) {
        // Explicit pure-native run (e.g. CI's "Test Rust" job).
        cli::cmd_test(file_str, no_check, run.as_deref());
    } else {
        // Default: fast rustc-free WASM path, native fallback for gaps.
        cli::cmd_test_fast(file_str, no_check, run.as_deref());
    }
}

/// `dispatch`'s `Commands::Check` arm. Extracted verbatim — `explain` still
/// returns early into the caller via its own `bool` return (`true` = already
/// handled, caller should return).
fn dispatch_check(file: Option<String>, deny_warnings: bool, json: bool, explain: Option<String>, effects: bool, timings: bool, stamp: bool) {
    if let Some(code) = explain {
        print_error_explanation(&code);
        return;
    }
    let file = resolve_file(file);
    if effects {
        cli::cmd_check_effects(&file);
    } else if json {
        cli::cmd_check_json(&file);
    } else {
        cli::cmd_check(&file, deny_warnings, timings, stamp);
    }
}

/// `dispatch`'s `Commands::Ide` arm (nested `IdeCommand` match). Extracted verbatim.
fn dispatch_ide(cmd: IdeCommand) {
    match cmd {
        IdeCommand::Outline { target, filter, json } => {
            let target = match target {
                Some(t) if t.starts_with("@stdlib/") => t,
                other => resolve_file(other),
            };
            cli::cmd_ide_outline(&target, filter.as_deref(), json);
        }
        IdeCommand::Doc { symbol, file } => {
            let file = resolve_file(file);
            cli::cmd_ide_doc(&symbol, &file);
        }
        IdeCommand::StdlibSnapshot { modules, json } => {
            cli::cmd_ide_stdlib_snapshot(modules.as_deref(), json);
        }
    }
}

/// `dispatch`'s `Commands::Fmt` arm.
///
/// A DIRECTORY argument recurses (`almide fmt --check spec/`) — it used to reach
/// `parse_file` as-is and report "Is a directory", which the always-zero exit then
/// swallowed, so a CI job pointed at a tree silently checked nothing (#919). With no
/// argument at all the `src/` sweep is unchanged.
fn dispatch_fmt(files: Vec<String>, check: bool, json: bool, dry_run: bool, no_import_edit: bool) {
    let mode = match (json, check, dry_run) {
        // `--json` is the machine-readable spelling of the SAME gate: it never
        // writes, and it exits non-zero on drift exactly like `--check`.
        (true, _, _) => cli::FmtMode::CheckJson,
        (false, true, _) => cli::FmtMode::Check,
        (false, false, true) => cli::FmtMode::DryRun,
        (false, false, false) => cli::FmtMode::Write,
    };
    let fmt_files = if files.is_empty() {
        let mut found = Vec::new();
        if std::path::Path::new("src").is_dir() {
            collect_almd_files(std::path::Path::new("src"), &mut found);
        }
        if found.is_empty() {
            err(&format!("No .almd files found in src/"));
            std::process::exit(1);
        }
        found
    } else {
        let mut expanded = Vec::new();
        for f in files {
            let p = std::path::Path::new(&f);
            if p.is_dir() {
                collect_almd_files(p, &mut expanded);
            } else {
                expanded.push(f);
            }
        }
        expanded
    };
    cli::cmd_fmt(&fmt_files, mode, no_import_edit);
}

/// `dispatch`'s `Commands::Add` arm. Extracted verbatim.
fn dispatch_add(pkg: String, git: Option<String>, tag: Option<String>) {
    let (name, git_url, tag) = project_fetch::resolve_add_target(pkg, git, tag);
    project_fetch::add_dep_to_toml(&name, &git_url, tag.as_deref())
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    let dep = project::Dependency {
        name: name.clone(),
        git: git_url,
        tag,
        branch: None,
        version: None,
        path: None,
    };
    project_fetch::fetch_dep(&dep)
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
}

/// `dispatch`'s `Commands::Update` arm (#1131): the sanctioned path FORWARD
/// for a locked git dependency — `add` re-pins the old commit and `clean`
/// only clears the cache, so before this the sole escape was hand-editing
/// the lock the file itself says not to edit.
fn dispatch_update(dep: Option<String>) {
    if !std::path::Path::new("almide.toml").exists() {
        err(&format!("No almide.toml found"));
        std::process::exit(1);
    }
    let proj = project::parse_toml(std::path::Path::new("almide.toml"))
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    let changed = project_fetch::update_locked_deps(&proj, dep.as_deref())
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    if changed.is_empty() {
        out(&format!("No dependencies updated"));
        return;
    }
    for (name, before, after) in &changed {
        let short = |h: &String| h.chars().take(12).collect::<String>();
        match before.is_empty() {
            true => out(&format!("{} -> {}", name, short(after))),
            false => out(&format!("{} {} -> {}", name, short(before), short(after))),
        }
    }
}

/// `dispatch`'s `Commands::Deps` arm. Extracted verbatim.
fn dispatch_deps() {
    if std::path::Path::new("almide.toml").exists() {
        let proj = project::parse_toml(std::path::Path::new("almide.toml"))
            .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
        if proj.dependencies.is_empty() {
            out(&format!("No dependencies"));
        } else {
            for dep in &proj.dependencies {
                let ref_name = dep.tag.as_deref().or(dep.branch.as_deref()).unwrap_or("main");
                out(&format!("{} = {} ({})", dep.name, dep.git, ref_name));
            }
        }
    } else {
        err(&format!("No almide.toml found"));
    }
}

/// `dispatch`'s `Commands::DepPath` arm. Extracted verbatim.
fn dispatch_dep_path(name: String) {
    if !std::path::Path::new("almide.toml").exists() {
        err(&format!("No almide.toml found"));
        std::process::exit(1);
    }
    let proj = project::parse_toml(std::path::Path::new("almide.toml"))
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    let fetched = project_fetch::fetch_all_deps(&proj)
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    match fetched.iter().find(|fd| fd.pkg_id.name == name) {
        Some(fd) => out(&format!("{}", fd.source_dir.display())),
        None => {
            err(&format!("Dependency '{}' not found in almide.toml", name));
            std::process::exit(1);
        }
    }
}

/// `dispatch`'s second half: the "tooling" commands (LSP, diagnostics
/// explain, IDE queries, fmt, compile, clean, package management, self
/// update, emit). Split out of `dispatch`'s single flat match — cyclomatic
/// complexity counts one branch per match arm regardless of how thin the
/// arm body is, and `Commands` has ~19 variants, so the single match alone
/// tripped the threshold. Extracted verbatim; the split point is arbitrary
/// (arm count, not domain semantics) — `other` is exhaustive over exactly
/// the variants `dispatch`'s own match doesn't handle.
fn dispatch_rest(command: Commands) {
    match command {
        Commands::Lsp => {
            cli::lsp::run_lsp();
        }
        Commands::Mcp => {
            cli::mcp::run_mcp();
        }
        Commands::Explain { code } => {
            print_error_explanation(&code);
        }
        Commands::Ide { cmd } => dispatch_ide(cmd),
        Commands::Fmt { files, check, json, dry_run, no_import_edit } => dispatch_fmt(files, check, json, dry_run, no_import_edit),
        Commands::Compile { module, json, dry_run, output } => {
            cli::cmd_compile(module.as_deref(), json, dry_run, output.as_deref());
        }
        Commands::Clean => cli::cmd_clean(),
        Commands::Add { pkg, git, tag } => dispatch_add(pkg, git, tag),
        Commands::Update { dep } => dispatch_update(dep),
        Commands::Deps => dispatch_deps(),
        Commands::DepPath { name } => dispatch_dep_path(name),
        Commands::Install { spec, tag, branch, name, bin_dir, target } => {
            cli::cmd_install(
                &spec,
                tag.as_deref(),
                branch.as_deref(),
                name.as_deref(),
                bin_dir.as_deref(),
                target.as_deref(),
            );
        }
        Commands::SelfUpdate { version } => {
            cli::cmd_self_update(version.as_deref());
        }
        Commands::Emit { file, target, emit_ast, emit_ir, emit_dialect, no_check, repr_c, trace_map } => {
            cli::cmd_emit(cli::EmitArgs { file: &file, target: &target, emit_ast, emit_ir, emit_dialect, no_check, repr_c, trace_map });
        }
        // `command`'s static type is the full `Commands` enum — Rust can't
        // narrow it to "one of the 12 variants `dispatch` doesn't handle"
        // across the function boundary, so this match must stay exhaustive.
        // `dispatch`'s own match already handles the other 7 variants
        // before ever calling this function, so this arm is genuinely
        // unreachable at runtime.
        _ => unreachable!("dispatch's match should have handled this Commands variant"),
    }
}

fn dispatch(cli: Cli) {
    if cli.verbose {
        // Deep pipeline code reads the env var so verbosity needs no
        // plumbing through every call chain (same pattern as
        // ALMIDE_FUEL_PROBE above).
        // SAFETY: single-threaded at this point (process entry, pre-dispatch).
        unsafe { std::env::set_var("ALMIDE_VERBOSE", "1") };
    }
    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            cli::repl::run_repl();
            return;
        }
    };
    match command {
        Commands::Init => cli::cmd_init(),
        Commands::Run { file, no_check, release, target, verified: _, no_verified, time_report, program_args } =>
            dispatch_run(file, no_check, release, target, no_verified, time_report, program_args),
        Commands::Build { file, o, target, release, fast, unchecked_index, no_check, repr_c, cdylib, emit_unverified, verified: _, no_verified, wasm_opt, component, heap_cap } => {
            let file = resolve_file(file);
            warn_no_verified_deprecated(no_verified);
            cli::cmd_build(cli::BuildArgs {
                file: &file,
                output: o.as_deref(),
                target: target.as_deref(),
                release: release || fast,
                fast,
                unchecked_index,
                no_check,
                repr_c,
                cdylib,
                emit_unverified,
                verified: !no_verified,
                native_verified: !no_verified,
                wasm_opt,
                component,
                heap_cap,
            });
        }
        Commands::Test { file, run, no_check, json, target } => dispatch_test(file, run, no_check, json, target),
        Commands::Check { file, deny_warnings, json, explain, effects, timings, stamp } => dispatch_check(file, deny_warnings, json, explain, effects, timings, stamp),
        Commands::Fix { file, dry_run, json } => {
            let file = resolve_file(file);
            cli::cmd_fix(&file, dry_run, json);
        }
        Commands::DocsGen { check } => {
            cli::cmd_docs_gen(check);
        }
        other => dispatch_rest(other),
    }
}
