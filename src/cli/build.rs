use std::process::Command;
use crate::{project, err};
use super::wasm_debug::debug_build_guard;

/// Flags for [`cmd_build`] — bundled into one struct (was 12 positional
/// params, a max-params violation on its own) so the function signature
/// stays under the params threshold. Field names match `Commands::Build`'s
/// clap fields 1:1, so the call site in `main.rs` builds it directly from
/// the destructured match arm.
pub struct BuildArgs<'a> {
    pub file: &'a str,
    pub output: Option<&'a str>,
    pub target: Option<&'a str>,
    pub release: bool,
    pub fast: bool,
    /// `--target-cpu` (#3604): the CPU a native `--release` build is tuned for.
    pub target_cpu: Option<&'a str>,
    /// `--print-release-rustflags`: print the release codegen flags, do not build.
    pub print_release_rustflags: bool,
    pub unchecked_index: bool,
    pub no_check: bool,
    pub repr_c: bool,
    pub cdylib: bool,
    pub emit_unverified: bool,
    pub verified: bool,
    pub native_verified: bool,
    pub wasm_opt: bool,
    pub component: bool,
    pub heap_cap: Option<u32>,
    /// `--host js` (#2265): write the JS host next to the wasm output.
    pub host: Option<&'a str>,
    /// `--debug` (#1315): DWARF line tables in the wasm output.
    pub debug: bool,
}

/// The npm/JavaScript target was removed with the TS backend; reject it with
/// a clear pointer instead of emitting a non-functional stub package. Exits
/// the process (never returns) when `target` names a removed target.
fn reject_removed_target(target: Option<&str>) {
    if matches!(target, Some("npm" | "js" | "ts" | "javascript" | "typescript")) {
        let t = target.unwrap_or("npm");
        err(&format!(
            "error: the npm/JavaScript build target has been removed\n  \
             in `almide build --target {t}`\n  \
             supported targets: rust (default, native binary), wasm, linux-musl, or a rustc target triple\n  \
             hint: use `--target wasm` for a portable build"
        ));
        std::process::exit(2);
    }
}

/// Compute the build's output path: `file`/`almide.toml`-derived default
/// unless `-o` was given, plus the Windows `.exe` auto-suffix for native
/// builds. Extracted verbatim from `cmd_build`.
fn compute_output_path(file: &str, output: Option<&str>, is_wasm: bool) -> String {
    let default_output = if is_wasm {
        format!("{}.wasm", file.strip_suffix(".almd").unwrap_or("a.out"))
    } else if std::path::Path::new("almide.toml").exists() {
        // `[package].name` as TOML reads it — not the first line that starts
        // with `name`, which could be a key in any table (#3253).
        project::manifest_package_name(std::path::Path::new("almide.toml"))
            .unwrap_or_else(|| file.strip_suffix(".almd").unwrap_or("a.out").to_string())
    } else {
        file.strip_suffix(".almd").unwrap_or("a.out").to_string()
    };
    let output_raw = output.unwrap_or(&default_output);

    // On Windows, auto-append .exe for native builds
    if cfg!(target_os = "windows") && !is_wasm
        && !output_raw.ends_with(".exe") && !output_raw.ends_with(".wasm")
    {
        format!("{}.exe", output_raw)
    } else {
        output_raw.to_string()
    }
}

/// The generated crate's Cargo name for a native build of `file` (#3349):
/// `[package].name` of the `almide.toml` in the working directory — the same
/// source the default output name reads — else the entry file's stem. Never
/// the `-o` value: that is an output PATH, and a path is not a crate name.
fn native_crate_name(file: &str) -> String {
    let manifest = std::path::Path::new("almide.toml");
    let base = manifest.exists().then(|| project::manifest_package_name(manifest)).flatten()
        .unwrap_or_else(|| std::path::Path::new(file).file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default());
    sanitize_crate_name(&base)
}

/// Make `name` a valid Cargo library name: every character outside
/// `[A-Za-z0-9_]` becomes `_`, and a name that is empty or starts with a digit
/// gets a leading `almide_`.
fn sanitize_crate_name(name: &str) -> String {
    let clean: String = name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    match clean.chars().next() {
        Some(c) if !c.is_ascii_digit() => clean,
        _ => format!("almide_{clean}"),
    }
}

/// Where a cdylib build writes its library (#3349): the `-o` path exactly as
/// given — the same meaning `-o` has for a binary — else today's default,
/// `lib<crate>.<ext>` for the build's target in the working directory.
fn cdylib_output_path(output: Option<&str>, crate_name: &str, triple: Option<&str>) -> std::path::PathBuf {
    match output {
        Some(o) => std::path::PathBuf::from(o),
        None => std::path::PathBuf::from(super::native_target::cdylib_file_name(crate_name, triple)),
    }
}

/// Install a built artifact at `dest`: create the parent directory, then
/// stage-and-RENAME, never copy onto an existing file. A bare `fs::copy`
/// rewrites the destination IN PLACE (same inode), and on macOS the kernel's
/// code-signature cache is keyed by vnode: a binary overwritten at the same
/// inode after its previous content was executed gets SIGKILLed on the next
/// exec — no exit code, no stderr, nothing to debug. `almide build app.almd -o
/// app` twice in a row then `./app` reproduced it sporadically, and the
/// fuzzer's per-worker reused output path hit it reliably deep into a campaign
/// (seed 1785165458340124000 index 572: a phantom "native run failed while
/// wasm succeeded"). A loaded dylib is mapped the same way. The rename gives
/// the destination a fresh inode atomically; the staging temp lives in the
/// SAME directory so the rename cannot cross a filesystem. `-o build/app` must
/// not fail just because `build/` doesn't exist yet.
fn install_artifact(built: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let file_name = dest.file_name().map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dest.to_string_lossy().into_owned());
    let staged = dest.with_file_name(format!(".{}.staged-{}", file_name, std::process::id()));
    let copy_then_rename = std::fs::copy(built, &staged).and_then(|_| std::fs::rename(&staged, dest));
    if copy_then_rename.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    copy_then_rename
}

/// `cmd_build`'s cdylib target: build a shared library (.dylib/.so/.dll) and
/// write it to `dest`. Exits the process on a compile error, otherwise prints
/// the written path and returns.
fn cmd_build_cdylib(rs_code: &str, crate_name: &str, dest: &std::path::Path, use_release: bool, native_deps: &[project::NativeDep], source_root: Option<&std::path::Path>, note: &str) {
    let project_dir = std::env::temp_dir().join("almide-build-cdylib");
    // Strip fn main() from the code — cdylib has no entry point
    let lib_code = rs_code.replace("fn main()", "fn __almide_unused_main()");
    // Serialize across processes: the shared scratch dir's src + target would
    // otherwise be corrupted by a concurrent `almide build`. The lock is held
    // through the install so the library copied out is this build's.
    let _ = std::fs::create_dir_all(&project_dir);
    let _flock = super::build_dir::BuildDirLock::acquire(&project_dir)
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    // Same stale-incremental-session recovery as the bin path (#2500), under
    // the lock just taken.
    let built = super::cargo_ice::build_recovering_from_ice(&project_dir, || {
        super::cargo_build_cdylib(&lib_code, &project_dir, crate_name, use_release, native_deps, source_root)
    });
    match built {
        Ok(lib_path) => {
            if let Err(e) = install_artifact(&lib_path, dest) {
                err(&format!("Failed to copy library to {}: {}", dest.display(), e));
                std::process::exit(1);
            }
            err(&format!("Built {}{}", dest.display(), note));
        }
        Err(e) => {
            err(&format!("Compile error:\n{}", e));
            std::process::exit(1);
        }
    }
}

/// `cmd_build`'s native binary target: the content-cached build shared with
/// `almide run` — the cache key is the generated code, so identical output
/// from any caller (or any source path) reuses one binary and skips cargo
/// entirely. Locking and atomic binary staging live inside
/// `build_native_cached`; the copy-out below reads a content-named,
/// atomically-renamed file, so it needs no lock.
fn cmd_build_native(rs_code: &str, output: &str, use_release: bool, native_deps: &[project::NativeDep], source_root: Option<&std::path::Path>, note: &str) {
    match super::run::build_native_cached(rs_code, false, use_release, None, native_deps, source_root) {
        Ok(bin_path) => {
            if let Err(e) = install_artifact(&bin_path, std::path::Path::new(output)) {
                err(&format!("Failed to copy binary to {}: {}", output, e));
                std::process::exit(1);
            }
            err(&format!("Built {}{}", output, note));
        }
        Err(e) => {
            err(&format!("Compile error:\n{}", e));
            std::process::exit(1);
        }
    }
}

pub fn cmd_build(args: BuildArgs) {
    // Destructured immediately so the function body below is untouched
    // (verbatim) — this is purely a call-site params bundling.
    let BuildArgs {
        file, output, target, release, fast, target_cpu, print_release_rustflags, unchecked_index: _unchecked_index,
        no_check, repr_c, cdylib, emit_unverified, verified, native_verified, wasm_opt, component, heap_cap, host, debug,
    } = args;
    reject_removed_target(target);
    let is_wasm = matches!(target, Some("wasm" | "wasm32" | "wasi"));
    let is_wasm_direct = matches!(target, Some("wasm"));
    let heap_cap = heap_cap.filter(|n| *n > 0);
    if is_wasm && target_cpu.is_some() {
        err("error: --target-cpu tunes a native --release build; a wasm module is CPU-independent");
        std::process::exit(2);
    }
    if is_wasm && print_release_rustflags {
        err("error: --print-release-rustflags describes a native --release build; a wasm module is not built by rustc");
        std::process::exit(2);
    }

    // Direct WASM emit: .almd → IR → WASM binary (no rustc)
    if is_wasm_direct {
        // The knob rides a render-scoped guard, not a params thread-through:
        // the whole CLI runs on the one `almide-main` worker thread, so the
        // thread-local is exactly as scoped as this call.
        // #1729: the cap becomes the emitted memory's declared maximum.
        let _cap = heap_cap.map(almide_wasm::heap_cap::HeapCapGuard::set);
        let _lines = debug.then(|| debug_build_guard(component, wasm_opt));
        super::build_wasm::cmd_build_wasm_direct(&super::build_wasm::WasmBuild {
            file, output, allow_unverified: emit_unverified, verified, wasm_opt, component, host,
        });
        return;
    }
    if debug {
        err("error: --debug is a wasm option (DWARF line tables): `almide build app.almd --target wasm --debug`");
        std::process::exit(2);
    }
    if host.is_some() {
        err("error: --host is a wasm option: `almide build app.almd --target wasm --host js`");
        std::process::exit(2);
    }

    // #2772: resolve the native target BEFORE compiling — an unknown `--target`
    // used to fall through to a host build, and an inherited
    // `CARGO_BUILD_TARGET` handed back the previous host binary. The triple is
    // scoped to this build; see `native_target`.
    let _cross = (!is_wasm).then(|| {
        let env_target = std::env::var("CARGO_BUILD_TARGET").ok();
        match super::native_target::resolve_native_target(target, env_target.as_deref(), std::env::consts::ARCH) {
            Ok(triple) => {
                if let Err(e) = super::native_target::validate_target_cpu(target_cpu, triple.as_deref()) {
                    err(&e);
                    std::process::exit(2);
                }
                super::native_target::CrossTargetGuard::set(triple)
            }
            Err(e) => {
                err(&e);
                std::process::exit(2);
            }
        }
    });
    // #3604: scoped like the triple; read by every native release build path.
    let _cpu = super::native_target::TargetCpuGuard::set(target_cpu.map(str::to_string));
    if print_release_rustflags {
        println!("{}", super::native_target::release_codegen_rustc_args().join(" "));
        return;
    }

    let requested_output = output;
    let output = compute_output_path(file, output, is_wasm);

    let opts = crate::codegen::CodegenOptions { repr_c, allow_unverified: false, trace: false, meter_in_codegen: false };
    let (rs_code, metered, outside_guarantee) = crate::try_compile_reporting(file, no_check, &opts)
        .unwrap_or_else(|_| std::process::exit(1));

    // WASI target: use bare rustc (no external crate deps needed for WASM)
    if is_wasm {
        let rs_code = match heap_cap {
            Some(n) => inject_heap_cap(&rs_code, n),
            None => rs_code,
        };
        cmd_build_wasi_rustc(&rs_code, &output, &guarantee_note(&outside_guarantee));
        return;
    }

    // NATIVE trust spine (#764, rung 1) — OPT-IN `--verified` (explicit flag, not
    // the wasm default): try the v1 MIR renderer (same Perceus MIR as the wasm
    // leg; Drop erased to Rust scope-end, ownership verified pre-render). A WALL
    // falls back to the v0 source above — honest-wall discipline: a v1-rendered
    // program is never wrong.
    let rs_code = if native_verified && !repr_c && !cdylib {
        super::render_v1_native_or_fallback(file, rs_code, metered, &opts)
    } else {
        rs_code
    };

    // --heap-cap (#1530): wrap the binary's allocator AFTER the v1/v0 source
    // decision so both native paths carry the same ceiling.
    let rs_code = match heap_cap {
        Some(n) => {
            // The rlib fast path SLIMS the generated source down to its main
            // body and splices a prebuilt runtime in — the injected allocator
            // block would be stripped with the prelude it sits in, and the
            // "capped" binary would silently carry no cap (exactly the
            // skip-as-pass shape this knob exists to kill). A cap build is a
            // harness build: take the self-contained cargo path.
            // SAFETY: the whole CLI runs sequentially on the one `almide-main`
            // worker thread and no other thread is alive to read the
            // environment concurrently.
            unsafe { std::env::set_var("ALMIDE_NO_RTLIB", "1") };
            inject_heap_cap(&rs_code, n)
        }
        None => rs_code,
    };
    let rs_code = arm_alloc_count(rs_code);
    let rs_code = inject_fs_scope(rs_code);

    // Load native deps from almide.toml (search in input file's directory, then
    // CWD). BOTH the cdylib and bin paths need them: a cdylib with a `native/*.rs`
    // shim or a `[native-deps]` crate must wire them in exactly like a bin, or it
    // fails with E0433 / a missing dep (#719). source_root is the directory
    // containing almide.toml (where native/ lives).
    let use_release = release || fast;
    let (native_deps, source_root) = super::load_native_build_config(file);

    // cdylib target: build shared library (.dylib/.so)
    if cdylib {
        // #3349: `-o` is the library FILE, as it is for a binary; the crate
        // name comes from the package / entry, not from the output path.
        let crate_name = native_crate_name(file);
        let triple = super::native_target::cross_target();
        let dest = cdylib_output_path(requested_output, &crate_name, triple.as_deref());
        cmd_build_cdylib(&rs_code, &crate_name, &dest, use_release, &native_deps, source_root.as_deref(), &guarantee_note(&outside_guarantee));
        return;
    }

    cmd_build_native(&rs_code, &output, use_release, &native_deps, source_root.as_deref(), &guarantee_note(&outside_guarantee));
}

/// `--heap-cap` (#1530): wrap the generated program's allocator in a counting
/// `GlobalAlloc` with a hard live-bytes ceiling. Exceeding it is the DEFINED
/// abort — "Error: out of memory" on stderr, exit 1 — the exact shape the wasm
/// leg's `$oom` prints when its bump frontier passes the same knob, so a leak
/// harness can drive both targets to one deterministic boundary. The block is
/// inserted AFTER the leading inner attributes (`#![...]` must stay first in a
/// crate root); item order is otherwise free in Rust.
fn inject_heap_cap(rs_code: &str, cap: u32) -> String {
    let runtime = format!(
        r#"// --heap-cap runtime (#1530): hard ceiling on live heap bytes.
const __ALMIDE_HEAP_CAP: usize = {cap};
static __ALMIDE_HEAP_LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static __ALMIDE_HEAP_TRIPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
struct __AlmideCapAlloc;
impl __AlmideCapAlloc {{
    fn charge(&self, n: usize) {{
        use std::sync::atomic::Ordering::Relaxed;
        let live = __ALMIDE_HEAP_LIVE.fetch_add(n, Relaxed) + n;
        // TRIPPED gates the abort to fire once; allocations made by the exit
        // path itself pass through instead of re-entering the abort.
        if live > __ALMIDE_HEAP_CAP && !__ALMIDE_HEAP_TRIPPED.swap(true, Relaxed) {{
            use std::io::Write;
            let _ = std::io::stderr().write_all(b"Error: out of memory\n");
            std::process::exit(1);
        }}
    }}
}}
unsafe impl std::alloc::GlobalAlloc for __AlmideCapAlloc {{
    unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {{
        self.charge(l.size());
        std::alloc::System.alloc(l)
    }}
    unsafe fn alloc_zeroed(&self, l: std::alloc::Layout) -> *mut u8 {{
        self.charge(l.size());
        std::alloc::System.alloc_zeroed(l)
    }}
    unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) {{
        __ALMIDE_HEAP_LIVE.fetch_sub(l.size(), std::sync::atomic::Ordering::Relaxed);
        std::alloc::System.dealloc(p, l)
    }}
    unsafe fn realloc(&self, p: *mut u8, l: std::alloc::Layout, new: usize) -> *mut u8 {{
        if new > l.size() {{
            self.charge(new - l.size());
        }} else {{
            __ALMIDE_HEAP_LIVE.fetch_sub(l.size() - new, std::sync::atomic::Ordering::Relaxed);
        }}
        std::alloc::System.realloc(p, l, new)
    }}
}}
#[global_allocator]
static __ALMIDE_CAP_ALLOC: __AlmideCapAlloc = __AlmideCapAlloc;
"#
    );
    splice_after_inner_attrs(rs_code, &runtime)
}

/// Place `block` after the generated crate root's leading inner attributes.
/// Inner attributes must precede all items: split after the leading run of
/// `#![...]` / blank / `//` comment lines, then place the block between the
/// two halves. The v1 trust-spine render opens with a `// Generated by …`
/// line BEFORE its `#![allow(..)]`; a leading run that stopped at the comment
/// put the runtime ahead of the inner attribute — "an inner attribute is not
/// permitted in this context" — which stayed invisible while every cap-test
/// program still walled v1 (#1869 widened the floor).
fn splice_after_inner_attrs(rs_code: &str, block: &str) -> String {
    let mut split = 0;
    for line in rs_code.split_inclusive('\n') {
        let l = line.trim_start();
        if l.trim().is_empty() || l.starts_with("#![") || l.starts_with("//") {
            split += line.len();
        } else {
            break;
        }
    }
    format!("{}{}{}", &rs_code[..split], block, &rs_code[split..])
}

/// `ALMIDE_ALLOC_COUNT` (#2228): is the allocation-count lane armed? A harness
/// switch, read once per build here and in `compile_to_binary_with`.
pub(crate) fn alloc_count_armed() -> bool {
    almide_base::env::flag("ALMIDE_ALLOC_COUNT")
}

/// `ALMIDE_ALLOC_COUNT` (#2228): inject the counting allocator when the lane is
/// armed, taking the self-contained cargo path the same way `--heap-cap` does
/// (see the comment there for why the rlib fast path cannot carry it).
/// `[permissions] fs` (#4022) on native: the granted roots, baked into the
/// program as a constant and resolved on the first fs call (a `OnceLock`), so
/// every fs call asks the scope before it touches a path. Nothing hangs on
/// `main`: a test harness build (`almide test`, which has none) is bounded
/// exactly like `almide run` (#4084). Only a program that reaches the fs
/// runtime carries the check. The roots are absolute: the manifest anchored
/// the relative ones to its own directory.
///
/// The check wraps the program's CALLS, not the runtime's fns: each
/// `almide_rt_fs_<f>(..)` call of the user code becomes a guard macro around
/// the same call, so the runtime source — and every inspection fingerprint
/// over it — is the same with and without a scope. Which guard a fn takes is
/// read off its runtime signature (a `bool` return answers false; `src, dst`
/// checks both; `pattern` checks the glob's literal base; `prefix` checks the
/// temp dir; `at` checks a rooted call's root), never from a list of fn names. A program without the key is
/// emitted unchanged.
pub(crate) fn inject_fs_scope(rs_code: String) -> String {
    let Some(roots) = almide_wasm_run::fs_scope() else { return rs_code };
    if !rs_code.contains("almide_rt_fs_") {
        return rs_code;
    }
    let guards = fs_runtime_guards();
    // Only the user code is rewritten: with the runtime inlined, its own
    // definitions and internal calls sit before the boundary marker.
    let marker = almide::codegen::RT_BOUNDARY_MARKER;
    let (head, user) = match rs_code.find(marker) {
        Some(i) => rs_code.split_at(i + marker.len()),
        None => ("", rs_code.as_str()),
    };
    let user = wrap_fs_calls(user, &guards);
    let list: Vec<String> = roots.iter().map(|r| format!("{r:?}")).collect();
    let prelude = format!(
        "\n{}\nconst __ALMIDE_FS_SCOPE_ROOTS: &[&str] = &[{}];\n{FS_SCOPE_GUARDS}",
        include_str!("../../crates/almide-base/src/fs_scope.rs"),
        list.join(", ")
    );
    if head.is_empty() {
        splice_after_inner_attrs(&user, &prelude)
    } else {
        format!("{head}{prelude}{user}")
    }
}

/// The guard macros and the scope cell `inject_fs_scope` splices in.
const FS_SCOPE_GUARDS: &str = r#"static __ALMIDE_FS_SCOPE: std::sync::OnceLock<Vec<std::path::PathBuf>> = std::sync::OnceLock::new();
fn __almide_fs_scope_check(path: &str) -> Result<(), String> {
    let roots = __ALMIDE_FS_SCOPE.get_or_init(|| __ALMIDE_FS_SCOPE_ROOTS.iter().map(|r| fs_scope_root(r)).collect());
    if fs_scope_admits(path, roots) { Ok(()) } else { Err(fs_scope_refusal(path)) }
}
macro_rules! __almide_fs_path { ($f:expr, $p:expr $(, $a:expr)* $(,)?) => {{ let __almide_p = $p; match __almide_fs_scope_check(&*__almide_p) { Ok(()) => ($f)(__almide_p $(, $a)*), Err(e) => Err(e.into()) } }}; }
macro_rules! __almide_fs_bool { ($f:expr, $p:expr $(,)?) => {{ let __almide_p = $p; if __almide_fs_scope_check(&*__almide_p).is_ok() { ($f)(__almide_p) } else { false } }}; }
macro_rules! __almide_fs_two { ($f:expr, $p:expr, $q:expr $(,)?) => {{ let (__almide_p, __almide_q) = ($p, $q); match __almide_fs_scope_check(&*__almide_p).and_then(|()| __almide_fs_scope_check(&*__almide_q)) { Ok(()) => ($f)(__almide_p, __almide_q), Err(e) => Err(e.into()) } }}; }
macro_rules! __almide_fs_glob { ($f:expr, $p:expr $(,)?) => {{ let __almide_p = $p; match __almide_fs_scope_check(&fs_scope_glob_base(&*__almide_p)) { Ok(()) => ($f)(__almide_p), Err(e) => Err(e.into()) } }}; }
macro_rules! __almide_fs_root { ($f:expr, $p:expr $(, $a:expr)* $(,)?) => {{ let __almide_p = $p; match __almide_fs_scope_check((&*__almide_p).split('\0').next().unwrap_or("")) { Ok(()) => ($f)(__almide_p $(, $a)*), Err(e) => Err(e.into()) } }}; }
macro_rules! __almide_fs_temp { ($f:expr, $p:expr $(,)?) => {{ let __almide_p = $p; match __almide_fs_scope_check(&std::env::temp_dir().to_string_lossy()) { Ok(()) => ($f)(__almide_p), Err(e) => Err(e.into()) } }}; }
"#;

/// Each public `almide_rt_fs_*` fn of the runtime with the guard macro its
/// signature calls for (`None`: it takes no path).
fn fs_runtime_guards() -> std::collections::HashMap<String, &'static str> {
    let src = almide::codegen::generated::rust_runtime::RUST_RUNTIME_MODULES
        .iter()
        .find(|(m, _)| *m == "fs")
        .map_or("", |(_, s)| *s);
    let mut out = std::collections::HashMap::new();
    for line in src.lines() {
        let Some(rest) = line.strip_prefix("pub fn almide_rt_fs_") else { continue };
        let name_len = rest.find(['<', '(']).unwrap_or(rest.len());
        let name = format!("almide_rt_fs_{}", &rest[..name_len]);
        let Some((params, ret)) = fs_signature(&rest[name_len..]) else { continue };
        let guard = match params.as_slice() {
            [] => continue,
            [p, ..] if ret.starts_with("bool") && (p == "path" || p == "dir") => "__almide_fs_bool",
            [s, d] if s == "src" && d == "dst" => "__almide_fs_two",
            [p] if p == "pattern" => "__almide_fs_glob",
            [p] if p == "prefix" => "__almide_fs_temp",
            // The rooted calls' `<root>\0<segments>` operand (#4084): the
            // root is judged; the descriptor walk keeps the rest under it.
            [p, ..] if p == "at" => "__almide_fs_root",
            [p, ..] if p == "path" || p == "dir" => "__almide_fs_path",
            _ => continue,
        };
        out.insert(name, guard);
    }
    out
}

/// The parameter names and the return text of a signature tail
/// `<generics>(params) -> Ret {`.
fn fs_signature(tail: &str) -> Option<(Vec<String>, String)> {
    let mut depth = 0i32;
    let mut start = None;
    for (i, c) in tail.char_indices() {
        match c {
            '<' | '(' | '[' => {
                if c == '(' && depth == 0 && start.is_none() {
                    start = Some(i + 1);
                }
                depth += 1;
            }
            '>' if i > 0 && tail.as_bytes()[i - 1] == b'-' => {}
            '>' | ')' | ']' => {
                depth -= 1;
                if c == ')' && depth == 0 && start.is_some() {
                    let params = &tail[start?..i];
                    let ret = tail[i + 1..].trim_start().strip_prefix("->").unwrap_or("").trim_start().to_string();
                    let mut names = Vec::new();
                    let (mut d, mut seg) = (0i32, String::new());
                    for ch in params.chars() {
                        match ch {
                            '<' | '(' | '[' => d += 1,
                            '>' | ')' | ']' => d -= 1,
                            _ => {}
                        }
                        if ch == ',' && d == 0 {
                            names.push(std::mem::take(&mut seg));
                        } else {
                            seg.push(ch);
                        }
                    }
                    names.push(seg);
                    let names = names
                        .iter()
                        .filter_map(|p| p.split(':').next().map(str::trim))
                        .filter(|n| !n.is_empty())
                        .map(str::to_string)
                        .collect();
                    return Some((names, ret));
                }
            }
            _ => {}
        }
    }
    None
}

/// `code` with every call `almide_rt_fs_<f>[::<..>](` whose fn has a guard
/// rewritten to `<guard>!(almide_rt_fs_<f>[::<..>], `.
fn wrap_fs_calls(code: &str, guards: &std::collections::HashMap<String, &'static str>) -> String {
    const PREFIX: &str = "almide_rt_fs_";
    let mut out = String::with_capacity(code.len() + 256);
    let mut rest = code;
    while let Some(i) = rest.find(PREFIX) {
        let before = &rest[..i];
        let after = &rest[i..];
        let ident_len = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(after.len());
        let ident = &after[..ident_len];
        let preceded_by_ident = before.chars().last().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        // An optional turbofish, then the call's `(`.
        let mut j = ident_len;
        if after[j..].starts_with("::<") {
            let mut depth = 0i32;
            for (k, c) in after[j + 2..].char_indices() {
                match c {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            j = j + 2 + k + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        out.push_str(before);
        match guards.get(ident) {
            Some(g) if !preceded_by_ident && !before.ends_with("fn ") && after[j..].starts_with('(') => {
                out.push_str(&format!("{g}!({}, ", &after[..j]));
                rest = &after[j + 1..];
            }
            _ => {
                out.push_str(ident);
                rest = &after[ident_len..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn arm_alloc_count(rs_code: String) -> String {
    if !alloc_count_armed() {
        return rs_code;
    }
    // SAFETY: the whole CLI runs sequentially on the one `almide-main` worker
    // thread and no other thread is alive to read the environment concurrently.
    unsafe { std::env::set_var("ALMIDE_NO_RTLIB", "1") };
    inject_alloc_count(&rs_code)
}

/// `ALMIDE_ALLOC_COUNT` (#2228): wrap the generated program's allocator in a
/// counting `GlobalAlloc` and print one line on stderr when `__almide_main`
/// returns — `__ALMD_ALLOC allocs=N deallocs=N reallocs=N peak=N` — so a
/// harness can pin how much a program allocates. stdout equality and rustc
/// acceptance are both blind to a value cloned where a borrow would do or
/// kept alive past its last use; the count is not. The counters are reset
/// right before `__almide_main` runs, so process start-up (arguments, the
/// worker thread) is outside the window and the window is the program's own
/// work. Like `--heap-cap`, an armed build takes the self-contained cargo
/// path: the rlib fast path slims the source to its main body and would strip
/// the block with the prelude it sits in.
pub(crate) fn inject_alloc_count(rs_code: &str) -> String {
    const RUNTIME: &str = r#"// ALMIDE_ALLOC_COUNT runtime (#2228): count every allocation of the program's own work.
struct __AlmideCountAlloc;
static __ALMIDE_ALLOCS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static __ALMIDE_DEALLOCS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static __ALMIDE_REALLOCS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static __ALMIDE_LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static __ALMIDE_PEAK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
impl __AlmideCountAlloc {
    fn grow(&self, n: usize) {
        use std::sync::atomic::Ordering::Relaxed;
        let live = __ALMIDE_LIVE.fetch_add(n, Relaxed) + n;
        __ALMIDE_PEAK.fetch_max(live, Relaxed);
    }
    fn reset() {
        use std::sync::atomic::Ordering::Relaxed;
        for c in [&__ALMIDE_ALLOCS, &__ALMIDE_DEALLOCS, &__ALMIDE_REALLOCS, &__ALMIDE_LIVE, &__ALMIDE_PEAK] {
            c.store(0, Relaxed);
        }
    }
    fn report() {
        use std::io::Write;
        use std::sync::atomic::Ordering::Relaxed;
        let line = format!(
            "__ALMD_ALLOC allocs={} deallocs={} reallocs={} peak={}
",
            __ALMIDE_ALLOCS.load(Relaxed), __ALMIDE_DEALLOCS.load(Relaxed),
            __ALMIDE_REALLOCS.load(Relaxed), __ALMIDE_PEAK.load(Relaxed),
        );
        let _ = std::io::stderr().write_all(line.as_bytes());
    }
}
unsafe impl std::alloc::GlobalAlloc for __AlmideCountAlloc {
    unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {
        __ALMIDE_ALLOCS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.grow(l.size());
        std::alloc::System.alloc(l)
    }
    unsafe fn alloc_zeroed(&self, l: std::alloc::Layout) -> *mut u8 {
        __ALMIDE_ALLOCS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.grow(l.size());
        std::alloc::System.alloc_zeroed(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) {
        __ALMIDE_DEALLOCS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        __ALMIDE_LIVE.fetch_sub(l.size(), std::sync::atomic::Ordering::Relaxed);
        std::alloc::System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: std::alloc::Layout, new: usize) -> *mut u8 {
        __ALMIDE_REALLOCS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if new > l.size() {
            self.grow(new - l.size());
        } else {
            __ALMIDE_LIVE.fetch_sub(l.size() - new, std::sync::atomic::Ordering::Relaxed);
        }
        std::alloc::System.realloc(p, l, new)
    }
}
#[global_allocator]
static __ALMIDE_COUNT_ALLOC: __AlmideCountAlloc = __AlmideCountAlloc;
/// Armed as the FIRST local of `main`, so it drops LAST: every value main's
/// body allocated has been freed when the report is written.
struct __AlmideAllocGuard;
impl __AlmideAllocGuard {
    fn arm() -> Self { __AlmideCountAlloc::reset(); __AlmideAllocGuard }
}
impl Drop for __AlmideAllocGuard {
    fn drop(&mut self) { __AlmideCountAlloc::report(); }
}
"#;
    // The window is `main`'s body: the guard is its first local (reset) and
    // drops last (report), in the v0 shape (`fn main` calling `__almide_main`)
    // and the v1 shape (the program's body inline in `fn main`) alike. A
    // program without a `fn main` (a test harness build) keeps the counters
    // but never reports — the oracle treats a missing line as a failure,
    // never as a pass. A `process.exit` inside the body skips the drop, and
    // so the report, for the same reason.
    let spliced = splice_after_inner_attrs(rs_code, RUNTIME);
    match spliced.find("\nfn main() {\n") {
        Some(m) => {
            let at = m + "\nfn main() {\n".len();
            format!("{}    let __almide_alloc_guard = __AlmideAllocGuard::arm();\n{}", &spliced[..at], &spliced[at..])
        }
        None => spliced,
    }
}

/// Build for WASI target using bare rustc (no external crate deps).
fn cmd_build_wasi_rustc(rs_code: &str, output: &str, note: &str) {
    let stem = output.strip_suffix(".wasm").unwrap_or(output);
    let tmp_rs = format!("{}.rs", stem);
    if let Err(e) = std::fs::write(&tmp_rs, rs_code) {
        err(&format!("Failed to write {}: {}", tmp_rs, e));
        std::process::exit(1);
    }

    let rustc = Command::new(&crate::find_rustc())
        .arg(&tmp_rs)
        .arg("-o").arg(output)
        .arg("-C").arg("overflow-checks=no")
        .arg("--edition").arg("2021")
        .arg("--target").arg("wasm32-wasip1")
        .arg("-C").arg("opt-level=3")
        .arg("-C").arg("lto=yes")
        // Enable WASM SIMD128 — all modern runtimes support it (wasmtime,
        // browsers since ~2022). Unlocks LLVM auto-vectorization for matmul.
        .arg("-C").arg("target-feature=+simd128")
        .output()
        .unwrap_or_else(|e| { err(&format!("Failed to run rustc: {}", e)); std::process::exit(1); });

    let _ = std::fs::remove_file(&tmp_rs);

    if !rustc.status.success() {
        let stderr = String::from_utf8_lossy(&rustc.stderr);
        err(&format!("Compile error:\n{}", stderr));
        std::process::exit(1);
    }

    err(&format!("Built {}{}", output, note));
}


/// #4008: the `Built` line's tail for a build that leaves the memory-safety
/// guarantee — empty for one that does not. Names up to three places, the
/// first an author reading the line would look at, and counts the rest.
pub(super) fn guarantee_note(sites: &[String]) -> String {
    match sites {
        [] => String::new(),
        _ => {
            let shown = sites.iter().take(3).cloned().collect::<Vec<_>>().join("; ");
            let more = if sites.len() > 3 { format!("; and {} more", sites.len() - 3) } else { String::new() };
            format!(" — outside the memory-safety guarantee: {shown}{more}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crate_name_is_an_identifier_whatever_the_package_is_called() {
        assert_eq!(sanitize_crate_name("ceangal"), "ceangal");
        assert_eq!(sanitize_crate_name("my-lib"), "my_lib");
        assert_eq!(sanitize_crate_name("out/libx.dylib"), "out_libx_dylib");
        assert_eq!(sanitize_crate_name("3d"), "almide_3d");
        assert_eq!(sanitize_crate_name(""), "almide_");
    }

    #[test]
    fn a_cdylib_output_is_the_path_given_else_the_target_named_default() {
        // #3349: `-o` is the file, verbatim, in every spelling.
        for o in ["x", "out/libx.dylib", "/abs/dir/libx.so", "x.dll"] {
            assert_eq!(cdylib_output_path(Some(o), "pkg", None), std::path::PathBuf::from(o));
        }
        assert_eq!(
            cdylib_output_path(None, "pkg", Some("x86_64-unknown-linux-gnu")),
            std::path::PathBuf::from("libpkg.so")
        );
        assert_eq!(
            cdylib_output_path(None, "pkg", Some("x86_64-pc-windows-gnu")),
            std::path::PathBuf::from("pkg.dll")
        );
    }
}
