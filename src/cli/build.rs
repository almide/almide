use std::process::Command;
use crate::{parse_file, canonicalize, check, diagnostic, resolve, project, project_fetch, err};

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
fn cmd_build_cdylib(rs_code: &str, crate_name: &str, dest: &std::path::Path, use_release: bool, native_deps: &[project::NativeDep], source_root: Option<&std::path::Path>) {
    let project_dir = std::env::temp_dir().join("almide-build-cdylib");
    // Strip fn main() from the code — cdylib has no entry point
    let lib_code = rs_code.replace("fn main()", "fn __almide_unused_main()");
    // Serialize across processes: the shared scratch dir's src + target would
    // otherwise be corrupted by a concurrent `almide build`. The lock is held
    // through the install so the library copied out is this build's.
    let _ = std::fs::create_dir_all(&project_dir);
    let _flock = super::run::BuildDirLock::acquire(&project_dir)
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    // Same stale-incremental-session recovery as the bin path (#2500), under
    // the lock just taken.
    let built = super::cargo_build::build_recovering_from_ice(&project_dir, || {
        super::cargo_build_cdylib(&lib_code, &project_dir, crate_name, use_release, native_deps, source_root)
    });
    match built {
        Ok(lib_path) => {
            if let Err(e) = install_artifact(&lib_path, dest) {
                err(&format!("Failed to copy library to {}: {}", dest.display(), e));
                std::process::exit(1);
            }
            err(&format!("Built {}", dest.display()));
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
fn cmd_build_native(rs_code: &str, output: &str, use_release: bool, native_deps: &[project::NativeDep], source_root: Option<&std::path::Path>) {
    match super::run::build_native_cached(rs_code, false, use_release, None, native_deps, source_root) {
        Ok(bin_path) => {
            if let Err(e) = install_artifact(&bin_path, std::path::Path::new(output)) {
                err(&format!("Failed to copy binary to {}: {}", output, e));
                std::process::exit(1);
            }
            err(&format!("Built {}", output));
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
        file, output, target, release, fast, unchecked_index: _unchecked_index,
        no_check, repr_c, cdylib, emit_unverified, verified, native_verified, wasm_opt, component, heap_cap, host,
    } = args;
    reject_removed_target(target);
    let is_wasm = matches!(target, Some("wasm" | "wasm32" | "wasi"));
    let is_wasm_direct = matches!(target, Some("wasm"));
    let heap_cap = heap_cap.filter(|n| *n > 0);

    // Direct WASM emit: .almd → IR → WASM binary (no rustc)
    if is_wasm_direct {
        // The knob rides a render-scoped guard, not a params thread-through:
        // the whole CLI runs on the one `almide-main` worker thread, so the
        // thread-local is exactly as scoped as this call.
        // #1729: the cap becomes the emitted memory's declared maximum.
        let _cap = heap_cap.map(almide_wasm::heap_cap::HeapCapGuard::set);
        cmd_build_wasm_direct(file, output, no_check, emit_unverified, verified, wasm_opt, component, host);
        return;
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
            Ok(triple) => super::native_target::CrossTargetGuard::set(triple),
            Err(e) => {
                err(&e);
                std::process::exit(2);
            }
        }
    });

    let requested_output = output;
    let output = compute_output_path(file, output, is_wasm);

    let opts = crate::codegen::CodegenOptions { repr_c, allow_unverified: false, trace: false };
    let (rs_code, _ir) = crate::try_compile_with_ir(file, no_check, &opts)
        .unwrap_or_else(|_| std::process::exit(1));

    // WASI target: use bare rustc (no external crate deps needed for WASM)
    if is_wasm {
        let rs_code = match heap_cap {
            Some(n) => inject_heap_cap(&rs_code, n),
            None => rs_code,
        };
        cmd_build_wasi_rustc(&rs_code, &output);
        return;
    }

    // NATIVE trust spine (#764, rung 1) — OPT-IN `--verified` (explicit flag, not
    // the wasm default): try the v1 MIR renderer (same Perceus MIR as the wasm
    // leg; Drop erased to Rust scope-end, ownership verified pre-render). A WALL
    // falls back to the v0 source above — honest-wall discipline: a v1-rendered
    // program is never wrong.
    let rs_code = if native_verified && !repr_c && !cdylib {
        super::render_v1_native_or_fallback(file, rs_code)
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
        cmd_build_cdylib(&rs_code, &crate_name, &dest, use_release, &native_deps, source_root.as_deref());
        return;
    }

    cmd_build_native(&rs_code, &output, use_release, &native_deps, source_root.as_deref());
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
fn cmd_build_wasi_rustc(rs_code: &str, output: &str) {
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

    err(&format!("Built {}", output));
}

/// Direct WASM emit: parse → check → lower → optimize → monomorphize → emit WASM binary.
/// `--host js` (#2265): write `<mod>.js` + `<mod>.d.ts` next to the module
/// (a refusal removes the module too, so a failed build leaves nothing) and
/// return the note the `Built` line appends.
fn write_js_host(output: &str, file: &str, bytes: &[u8], surface: &crate::cli::js_host::HostSurface) -> String {
    let base = output.strip_suffix(".wasm").unwrap_or(output);
    let (js_path, dts_path) = (format!("{base}.js"), format!("{base}.d.ts"));
    let wasm_name = std::path::Path::new(output).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| output.to_string());
    let owned = almide_wasm::host_exports::export_param_owned();
    let (js, dts) = match crate::cli::js_host::generate(&wasm_name, file, bytes, surface, &owned) {
        Ok(g) => g,
        Err(message) => {
            let _ = std::fs::remove_file(output);
            err(&message);
            std::process::exit(1);
        }
    };
    for (path, text) in [(&js_path, &js), (&dts_path, &dts)] {
        if let Err(e) = std::fs::write(path, text) {
            err(&format!("Failed to write {}: {}", path, e));
            std::process::exit(1);
        }
    }
    format!(" + {js_path} + {dts_path}")
}

/// The `--host` switch, validated: `js` or nothing; never with `--component`.
fn js_host_requested(host: Option<&str>, component: bool) -> bool {
    let js_host = match host {
        None => false,
        Some("js") => true,
        Some(other) => {
            err(&format!("error: `--host` accepts only `js` (got `{other}`)"));
            std::process::exit(2);
        }
    };
    if js_host && component {
        err("error: --host js writes a core-module host; it does not apply to --component");
        std::process::exit(2);
    }
    js_host
}

#[allow(clippy::too_many_arguments)]
fn cmd_build_wasm_direct(file: &str, output: Option<&str>, _no_check: bool, allow_unverified: bool, verified: bool, wasm_opt: bool, component: bool, host: Option<&str>) {
    let default_output = format!("{}.wasm", file.strip_suffix(".almd").unwrap_or("a.out"));
    let output = output.unwrap_or(&default_output);
    // `--host js` (#2265): the compiler writes the JS host next to the
    // module. The module exports its allocator + release under the guard,
    // and the emitter notes which exported params the callee owns.
    let js_host = js_host_requested(host, component);
    let _js_host = js_host.then(almide_wasm::host_exports::JsHostGuard::set);

    // The whole parse→check→lower→emit pipeline lives in `compile_to_wasm_bytes`
    // so `almide run --target wasm` produces the byte-identical module this
    // command writes — the cross-target equivalence guarantee depends on both
    // entry points sharing one code path. Any compile diagnostic was already
    // printed there; we just propagate the exit.
    let (bytes, host_ops, surface) = match compile_to_wasm_bytes_surfaced(file, allow_unverified, verified, true, false) {
        Ok(b) => b,
        Err(()) => std::process::exit(1),
    };
    // The module imports `almide.*` (the embedded host's surface). A BUILD artifact must run on stock runtimes, so it ships in
    // the WASI form — same index space, shimmed imports, proc_exit on trap
    // (the #1588 transform; the 578-fixture stock-wasmtime gate is its
    // reproduction witness).
    // `--component` (#1628 stage 1): the DIRECT p2 path — canonical-ABI
    // imports straight off the almide.* module, no preview1 adapter (~25 KB
    // lighter, and the only shape the stage-2 fan/async lowering can build
    // on). `ALMIDE_COMPONENT_ADAPTER=1` is the switch back to the stage-0
    // adapter wrap over the `to_wasi` module (#2752 keeps it: the adapter
    // route is also what an fs program takes below).
    // #2589 (ADR-0025): the subprocess family rides the private
    // `almide:process/spawn` import, which the p1 core module carries and no
    // component world declares — refuse the component build by name rather
    // than let a transform fail on an import it cannot place.
    let proc_op = host_ops.iter().copied().find(|op| (80..=89).contains(op));
    if component && let Some(op) = proc_op {
        err(&format!(
            "error[E081]: process.* (host op {op}) needs the private almide:process/spawn capability, which no component world declares (ADR-0025)\n  \
             hint: build the core module (drop --component) for a host that implements almide:process/spawn, or run it with `almide run {file} --target wasm`"
        ));
        std::process::exit(1);
    }
    let direct_p2 = component && !almide_base::env::flag("ALMIDE_COMPONENT_ADAPTER");
    // `ALMIDE_COMPONENT_P3=1` (#1628 stage 2, experimental): the WASI 0.3
    // component — stdio over component-model streams on the async
    // canonical ABI. Needs a p3-capable runtime (wasmtime 46+); stays an
    // env opt-in until the fan lowering lands on the same plumbing and
    // the corpus gates cover it.
    let direct_p3 = direct_p2 && almide_base::env::flag("ALMIDE_COMPONENT_P3");
    // #2742: a WASI 0.2 component that reaches the p1 fs service keeps the
    // stage-0 adapter route whenever the p1 shim serves its whole op set.
    // Those programs used to reach that route through the incumbent (the
    // p1 op audit rerouted every fs op there); the fs service plus the
    // preview1 adapter now serve them from the structural module, so the
    // direct shim's E081 must not claim a program that built before. An op
    // set without an fs op keeps the direct shim's verdict (#2113).
    let fs_via_adapter = direct_p2
        && !direct_p3
        && almide_wasm_run::component_availability::check(&host_ops, false).is_err()
        && host_ops.iter().any(|op| almide_wasm_run::wasi::FS_SERVICE_OPS.iter().any(|(o, _, _)| o == op))
        && host_ops.iter().all(|op| almide_wasm_run::wasi::P1_SERVED_OPS.contains(op));
    let direct_p2 = direct_p2 && !fs_via_adapter;
    if direct_p2
        && let Err(message) = almide_wasm_run::component_availability::check(&host_ops, direct_p3)
    {
        err(&message);
        std::process::exit(1);
    }
    let bytes = if direct_p3 {
        match almide_wasm_run::wasi_p3::to_p3(&bytes, &host_ops) {
            Ok(c) => c,
            Err(e) => {
                err(&format!("error: p3 component transform failed — this is an Almide bug: {e}"));
                std::process::exit(1);
            }
        }
    } else if direct_p2 {
        match almide_wasm_run::wasi_p2::to_p2(&bytes) {
            Ok(c) => c,
            Err(e) => {
                err(&format!("error: p2 component transform failed — this is an Almide bug: {e}"));
                std::process::exit(1);
            }
        }
    } else {
        let bytes = match almide_wasm_run::wasi::to_wasi(&bytes, &host_ops) {
            Ok(w) => w,
            Err(e) => {
                err(&format!("error: WASI transform failed — this is an Almide bug: {e}"));
                std::process::exit(1);
            }
        };
        // Stage-0 adapter wrap: the WASI core module + the Cargo-pinned
        // preview1 adapter. Packaging, not a rewrite.
        if component {
            match wrap_component(&bytes) {
                Ok(c) => c,
                Err(e) => {
                    err(&format!("error: component encoding failed — this is an Almide bug: {e}"));
                    std::process::exit(1);
                }
            }
        } else {
            bytes
        }
    };

    let pre_size = bytes.len();
    if let Err(e) = std::fs::write(output, &bytes) {
        err(&format!("Failed to write {}: {}", output, e));
        std::process::exit(1);
    }
    // The JS host is derived from the bytes that SHIP (#2276): after the
    // optional `wasm-opt` rewrite below, never from the pre-opt module.
    let host_from_shipped = |shipped: &[u8]| if js_host { write_js_host(output, file, shipped, &surface) } else { String::new() };

    // The trust-spine ships the bytes ITS OWN rendering process produced —
    // reachability DCE and the name-section trim already ran inside that
    // pipeline (docs/wasm/WASM-OUTPUT.md). `wasm-opt` is a different kind of
    // thing: an EXTERNAL, unverified transform applied to the renderer's
    // finished output, so running it replaces bytes the trust-spine produced
    // with bytes a separate, un-certified tool rewrote. That is why it stays
    // an explicit, default-off opt-in (`--wasm-opt`) rather than automatic —
    // see the wasm-opt parity leg (`tests/wasm_runtime_opt_parity.rs::wasm_opt_parity_spec`) for the
    // differential-testing evidence backing this tier's own guarantee.
    // Name the LEG in the one line every build prints: "which renderer
    // produced these bytes" was invisible by default (the line said
    // v1-verified even for structural output), and that opacity cost real
    // diagnosis time — a "wasm doesn't work" report cannot be split
    // between legs without it.
    let leg = match component {
        false => "structural leg",
        true if direct_p3 => "structural leg, WASI 0.3 component (direct, async ABI)",
        true if direct_p2 => "structural leg, WASI 0.2 component (direct)",
        true => "structural leg, WASI 0.2 component (adapter)",
    };
    // The trust word belongs to what earned it: the structural leg is
    // trusted end to end with its certificate PENDING (#1696,
    // docs/contracts/proven-vs-trusted.md). `verified` on output no
    // certificate covers is how #2154's run-time trap shipped (#2184).
    let trust = "trusted, certificate pending";
    // The artifact needs a host that grants the subprocess capability: say
    // so at build time, since a stock runtime will refuse it at load.
    if proc_op.is_some() {
        err(&format!(
            "note: {output} imports almide:process/spawn (process.*, ADR-0025): a stock WASI runtime refuses it at load; \
             it runs on a host that implements that import — `almide run {file} --target wasm` is one"
        ));
    }
    if !wasm_opt {
        let host_note = host_from_shipped(&bytes);
        err(&format!(
            "Built {}{} ({} bytes, {}, {} — wasm-opt skipped; pass --wasm-opt for a smaller build rewritten outside the renderer)",
            output, host_note, pre_size, leg, trust
        ));
        return;
    }

    match run_wasm_opt(output) {
        Ok(post_size) => {
            let shipped = std::fs::read(output).unwrap_or_else(|e| { err(&format!("Failed to read back {}: {}", output, e)); std::process::exit(1); });
            let host_note = host_from_shipped(&shipped);
            let pct = if pre_size > 0 { 100.0 * (pre_size - post_size) as f64 / pre_size as f64 } else { 0.0 };
            err(&format!(
                "Built {}{} ({} bytes → {} bytes, -{:.1}%, {}) — wasm-opt applied: these are NOT the renderer's own bytes",
                output, host_note, pre_size, post_size, pct, leg
            ));
        }
        Err(why) => {
            let host_note = host_from_shipped(&bytes);
            err(&format!(
                "Built {}{} ({} bytes, {}, {}) — --wasm-opt requested but not applied: {}; shipped the renderer's own module unoptimized",
                output, host_note, pre_size, leg, trust, why
            ));
        }
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
/// Type-check and lower one user module for the WASM path, appending its IR
/// directly onto `ir_program`. Extracted verbatim from
/// `compile_to_wasm_bytes`'s per-module loop body — same checker/env
/// mutation order, `continue` becomes an early `return`. (Sibling of
/// `crate::lower_one_user_module` in main.rs, which additionally tracks a
/// per-module `module_irs` map the WASM path doesn't need.)
pub(super) fn lower_one_wasm_module(
    checker: &mut check::Checker,
    name: &mut String,
    mod_prog: &mut almide::ast::Program,
    pkg_id: &mut Option<project::PkgId>,
    ir_program: &mut almide::ir::IrProgram,
    sources: &std::collections::HashMap<String, (String, String)>,
    module_diags: &mut Vec<(String, String, Vec<crate::diagnostic::Diagnostic>)>,
) {
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
    let mut module_diags = Vec::new();
    let sources = std::mem::take(&mut resolved.sources);
    for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
        lower_one_wasm_module(
            checker, name, mod_prog, pkg_id, &mut ir_program, &sources, &mut module_diags,
        );
    }
    resolved.sources = sources;
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
) -> Result<(), ()> {
    // The measurement escape: the availability PROBE builds through this
    // binary to measure the ground truth the table declares — with the
    // check armed it would measure its own declaration (circular).
    if almide_base::env::flag("ALMIDE_NO_AVAIL_CHECK") {
        return Ok(());
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
    }
    impl<'a> IrVisitor for Scan<'a> {
        fn visit_expr(&mut self, e: &almide::ir::IrExpr) {
            if let almide::ir::IrExprKind::Call {
                target: almide::ir::CallTarget::Module { module, func, .. }, ..
            } = &e.kind
            {
                let key = format!("{}.{}", module.as_str(), func.as_str());
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
    let mut scan = Scan { table, own: &own, leg_lit, hits: BTreeMap::new() };
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
    if hits.is_empty() {
        return Ok(());
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
    inputs: almide::wasm_route::RouteInputs,
    dep_paths: &[(project::PkgId, std::path::PathBuf)],
) -> Result<(Vec<u8>, Vec<i32>), ()> {
    use almide::wasm_route::{route_wasm, ModuleSource, RouteOptions};
    let opts = RouteOptions::from_env(library_ok);
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

pub(crate) fn compile_to_wasm_bytes(file: &str, allow_unverified: bool, verified: bool, library_ok: bool, embedded_leg: bool) -> Result<(Vec<u8>, Vec<i32>), ()> {
    compile_to_wasm_bytes_surfaced(file, allow_unverified, verified, library_ok, embedded_leg).map(|(b, o, _)| (b, o))
}

/// [`compile_to_wasm_bytes`] plus the program's host-visible surface (the
/// `pub fn` exports, the `@extern(wasm, ..)` imports, whether `main` exists),
/// read from the IR before routing — what `--host js` marshals (#2265).
pub(crate) fn compile_to_wasm_bytes_surfaced(file: &str, allow_unverified: bool, verified: bool, library_ok: bool, embedded_leg: bool) -> Result<(Vec<u8>, Vec<i32>, crate::cli::js_host::HostSurface), ()> {
    let (mut program, source_text, mut resolved, dep_paths) = parse_and_resolve_wasm(file)?;
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
    check_wasm_availability(&ir_program, &package, embedded_leg)?;
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
    render_wasm_module_routed(file, &source_text, library_ok, inputs, &dep_paths)
        .map(|(b, o)| (b, o, surface))
}

/// Run `wasm-opt -Oz` on the output file, in-place.
/// Returns the new file size on success.
/// Wrap a WASI-preview1 core module into a WASI 0.2 component: the
/// wit-component encoder + the adapter the provider crate pins. The
/// spike evidence (issue #1628): hello-world and the stdin family run
/// byte-identically core vs component on wasmtime 47.
fn wrap_component(core: &[u8]) -> Result<Vec<u8>, String> {
    wit_component::ComponentEncoder::default()
        .module(core)
        .map_err(|e| format!("module: {e}"))?
        .adapter(
            "wasi_snapshot_preview1",
            wasi_preview1_component_adapter_provider::WASI_SNAPSHOT_PREVIEW1_COMMAND_ADAPTER,
        )
        .map_err(|e| format!("adapter: {e}"))?
        .encode()
        .map_err(|e| format!("encode: {e}"))
}

fn run_wasm_opt(path: &str) -> Result<usize, String> {
    // `-Oz`, matching the flag's documented contract: `--wasm-opt` exists to
    // shrink the module (the published size tables are -Oz numbers), and the
    // implementation silently ran `-O3 --enable-simd` instead — a speed
    // profile with a feature the v1 renderer does not emit (no v128 in the
    // default output, #864/#916), so the documented numbers were not
    // reproducible through the flag.
    // --enable-nontrapping-float-to-int: float→int renders as
    // `i64.trunc_sat_f64_s` (the saturating truncate, lib_b.rs).
    // --enable-tail-call: mutual/self tail recursion renders `return_call`.
    // --enable-bulk-memory: the STRUCTURAL leg renders `memory.copy`
    // (#1616 — without the flag wasm-opt refuses the module, and the old
    // message blamed a missing install).
    // --enable-mutable-globals: the structural leg EXPORTS mutable
    // globals; newer binaryen accepts that by default but older releases
    // (CI's) validate the MVP restriction unless told otherwise — the
    // first CI run of the honest-refusal path caught exactly this.
    let out = std::process::Command::new("wasm-opt")
        .args([
            "-Oz",
            "--enable-nontrapping-float-to-int",
            "--enable-tail-call",
            "--enable-bulk-memory",
            "--enable-mutable-globals",
            path,
            "-o",
            path,
        ])
        .output()
        .map_err(|e| format!("wasm-opt is not installed ({})", e))?;
    if !out.status.success() {
        // The tool RAN and refused — a different failure class than a
        // missing install, and its stderr names the real cause (#1616:
        // "not installed" sent users to reinstall a tool they had).
        return Err(format!(
            "wasm-opt ran and refused (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let meta = std::fs::metadata(path).map_err(|e| format!("stat {}: {}", path, e))?;
    Ok(meta.len() as usize)
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
