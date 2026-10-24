//! #4102: a shipped wasm module carries its capability set in its import
//! table. For every `spec/wasm_cross` fixture, the artifact users build
//! (`almide build --target wasm`, the stock `to_wasi` form) imports only the
//! WASI calls its INFERRED effects allow. A program without `IO` has no path
//! to the file system, stdin or the preopens even if a miscompile tried to
//! reach one: the host gives it nothing to call. A reviewer or a host can read
//! the module's capabilities from its imports without trusting the compiler.
//!
//! The check runs on two independent readings of the same program: the
//! checker's effect report (`almide check --effects`) and the artifact's
//! import section. Neither is derived from the other: the emitter keeps an
//! import only when a reachable function calls it.
//!
//! The same holds one layer down, for the in-process module `almide run
//! --target wasm` hands the embedded host. That module imports the whole
//! `almide.fs_call` gateway whatever its effects, so the bound there is the
//! op numbers: every `fs_call` site's op must be a visible constant
//! (`almide_wasm::cert_project::fs_call_ops`), the host serves no other op
//! (crates/almide-wasm-run/src/host.rs `op_allowed`), and the capabilities
//! those ops reach (`cert_project::op_caps`) stay inside the effects.
//!
//! Controls, so the gate cannot pass blind:
//! - every fixture's effect report must be read. A missing report fails the
//!   fixture; it is never read as "no effects";
//! - the IO fixtures must import the file system and the pure ones must
//!   exist in number, so the import reader and the effect reader both see
//!   something to compare.
//!
//! Release-only (a full corpus build); CI runs it in the commissioned wasm
//! gates job's release-only root tests.

use std::collections::BTreeSet;

use almide::wasm_route::{route_wasm, ModuleSource, RouteOptions};
use almide_wasm::cert_project::{cap, fs_call_ops, op_caps};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// What any program may import: console output and exit, and the monotonic
/// clock the runtime's own `fan` deadline and fuel machinery reads (it carries
/// no data in or out but the time).
const BASE: &[&str] = &["fd_write", "proc_exit", "clock_time_get"];

/// The WASI preview-1 calls each effect adds to [`BASE`]. One row per effect
/// category; an effect missing here allows nothing more, so a new category
/// fails this gate until its row is written down.
const BY_EFFECT: &[(&str, &[&str])] = &[
    // #4155 reports ADR-0026 D2 leaves. The file and stdin leaves each keep
    // the whole call set the old `IO` row allowed: a program that reads one
    // file links the fs service, so a per-leaf split here would only move
    // the bound, not tighten it. Narrowing per leaf is a follow-up.
    ("FS.read", FILE_AND_STDIN),
    // `fs.create_temp_file` / `create_temp_dir` name the entry from the
    // entropy source (#4113), so writing may draw random bytes.
    ("FS.write", FILE_AND_STDIN_WITH_TEMP_NAMES),
    ("IO.stdin", FILE_AND_STDIN),
    // Console output and abort are [`BASE`]: they add nothing.
    ("IO.stdout", &[]),
    ("IO.stderr", &[]),
    ("Abort", &[]),
    ("Env.read", &["environ_get", "environ_sizes_get", "args_get", "args_sizes_get"]),
    ("Env.write", &["environ_get", "environ_sizes_get", "args_get", "args_sizes_get"]),
    ("Rand", &["random_get"]),
    ("Time", &[]),
    ("Fan", &[]),
];

/// [`FILE_AND_STDIN`] plus the entropy a temp entry's name is drawn from.
const FILE_AND_STDIN_WITH_TEMP_NAMES: &[&str] = &[
    "fd_read",
    "fd_close",
    "fd_filestat_get",
    "fd_prestat_get",
    "fd_prestat_dir_name",
    "fd_readdir",
    "path_open",
    "path_filestat_get",
    "path_create_directory",
    "path_remove_directory",
    "path_unlink_file",
    "path_rename",
    "environ_get",
    "environ_sizes_get",
    "random_get",
];

/// The calls the old single `IO` category allowed: stdin, files,
/// directories, and the preopen walk that resolves a relative path (which
/// reads the environment for the cwd).
const FILE_AND_STDIN: &[&str] = &[
    "fd_read",
    "fd_close",
    "fd_filestat_get",
    "fd_prestat_get",
    "fd_prestat_dir_name",
    "fd_readdir",
    "path_open",
    "path_filestat_get",
    "path_create_directory",
    "path_remove_directory",
    "path_unlink_file",
    "path_rename",
    "environ_get",
    "environ_sizes_get",
];

/// The `cert_project::cap` ids any program's `fs_call` ops may reach:
/// console output and the clock, as [`BASE`].
const BASE_CAPS: &[u32] = &[cap::STDOUT, cap::CLOCK];

/// The capability ids each effect adds to [`BASE_CAPS`], as [`BY_EFFECT`].
const CAPS_BY_EFFECT: &[(&str, &[u32])] = &[
    ("FS.read", &[cap::FS_READ, cap::FS_WRITE, cap::STDIN]),
    ("FS.write", &[cap::FS_READ, cap::FS_WRITE, cap::STDIN]),
    ("IO.stdin", &[cap::FS_READ, cap::FS_WRITE, cap::STDIN]),
    ("IO.stdout", &[]),
    ("IO.stderr", &[]),
    ("Abort", &[]),
    ("Env.read", &[cap::ENV]),
    ("Env.write", &[cap::ENV]),
    ("Rand", &[cap::ENTROPY]),
    ("Time", &[cap::CLOCK]),
    ("Net.fetch", &[cap::NET]),
    ("Net.listen", &[cap::NET]),
    ("Proc", &[cap::PROC]),
    ("Fan", &[]),
];

/// Effects a program may have and still count as touching nothing: console
/// output and abort (ADR-0022 lets a plain `fn` print), which [`BASE`] covers.
const CONSOLE_ONLY: &[&str] = &["IO.stdout", "IO.stderr", "Abort"];

/// The leaves that reach files or stdin: what the old `IO` category meant.
const FILE_LEAVES: &[&str] = &["FS.read", "FS.write", "IO.stdin"];

/// The file-system calls: what a program without `IO` must never import.
const FILE_SYSTEM: &[&str] = &["path_open", "fd_prestat_get", "fd_readdir", "fd_filestat_get", "path_filestat_get"];

/// The effect names the checker reports for any function of `src`, or why
/// the report could not be read.
fn effects(bin: &str, src: &Path) -> Result<BTreeSet<String>, String> {
    let o = Command::new(bin)
        .args(["check", "--effects", src.to_str().expect("utf8")])
        .stdin(Stdio::null())
        .output()
        .expect("almide runs");
    // The report is written to stderr.
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    if !o.status.success() || !text.contains("functions:") {
        return Err(format!("no effect report (exit {:?}):\n{text}", o.status.code()));
    }
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let Some((_, rest)) = line.split_once("→ {") else { continue };
        let Some((set, _)) = rest.split_once('}') else { continue };
        out.extend(set.split(',').map(str::trim).filter(|e| !e.is_empty()).map(String::from));
    }
    Ok(out)
}

/// The function imports of a module, by field name.
fn imports(bytes: &[u8]) -> Result<BTreeSet<String>, String> {
    let mut out = BTreeSet::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::ImportSection(reader) = payload.map_err(|e| e.to_string())? {
            for imp in reader.into_imports() {
                let imp = imp.map_err(|e| e.to_string())?;
                out.insert(imp.name.to_string());
            }
        }
    }
    Ok(out)
}

fn allowed(effects: &BTreeSet<String>) -> BTreeSet<&'static str> {
    let mut out: BTreeSet<&'static str> = BASE.iter().copied().collect();
    for (eff, calls) in BY_EFFECT {
        if effects.contains(*eff) {
            out.extend(calls.iter().copied());
        }
    }
    out
}

fn allowed_caps(effects: &BTreeSet<String>) -> BTreeSet<u32> {
    let mut out: BTreeSet<u32> = BASE_CAPS.iter().copied().collect();
    for (eff, caps) in CAPS_BY_EFFECT {
        if effects.contains(*eff) {
            out.extend(caps.iter().copied());
        }
    }
    out
}

struct Row {
    name: String,
    effects: BTreeSet<String>,
    imports: BTreeSet<String>,
    /// The in-process module's `fs_call` ops; `None` when a site's op is not
    /// a visible constant.
    raw_ops: Option<BTreeSet<i32>>,
}

fn measure(bin: &str, src: &Path, dir: &Path) -> Result<Row, String> {
    let name = src.file_stem().expect("stem").to_string_lossy().into_owned();
    let effects = effects(bin, src).map_err(|e| format!("{name}: {e}"))?;
    let wasm = dir.join(format!("{name}.wasm"));
    let o = Command::new(bin)
        .args(["build", src.to_str().expect("utf8"), "--target", "wasm", "-o", wasm.to_str().expect("utf8")])
        .stdin(Stdio::null())
        .output()
        .expect("almide runs");
    if !o.status.success() {
        return Err(format!("{name}: the wasm build failed:\n{}", String::from_utf8_lossy(&o.stderr)));
    }
    let bytes = std::fs::read(&wasm).map_err(|e| format!("{name}: {e}"))?;
    let imports = imports(&bytes).map_err(|e| format!("{name}: {e}"))?;
    // The in-process module, routed the way `almide build` routes it.
    let path = src.to_str().expect("utf8");
    let text = std::fs::read_to_string(src).map_err(|e| format!("{name}: {e}"))?;
    let opts = RouteOptions { library: true, ..RouteOptions::default() };
    let raw = route_wasm(path, &text, ModuleSource::Disk { dep_paths: &[] }, None, opts, &mut |_| {})
        .map_err(|e| format!("{name}: the in-process route failed: {e:?}"))?;
    let raw_ops = fs_call_ops(&raw.bytes).map_err(|e| format!("{name}: {e}"))?;
    Ok(Row { name, effects, imports, raw_ops })
}

#[cfg_attr(debug_assertions, ignore = "full corpus build — release-only (CI: commissioned wasm gates)")]
#[test]
fn every_shipped_module_imports_only_what_its_effects_allow() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross");
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("spec/wasm_cross exists")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "almd"))
        .collect();
    fixtures.sort();
    let tmp = tempfile::tempdir().expect("tempdir");
    let bin = almide_bin();
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = fixtures.len().div_ceil(workers).max(1);
    let rows: Vec<Result<Row, String>> = std::thread::scope(|s| {
        let handles: Vec<_> = fixtures
            .chunks(chunk)
            .map(|part| {
                let (bin, tmp) = (&bin, tmp.path());
                // The compiler runs on a deep stack (src/main.rs).
                std::thread::Builder::new()
                    .stack_size(256 * 1024 * 1024)
                    .spawn_scoped(s, move || part.iter().map(|src| measure(bin, src, tmp)).collect::<Vec<_>>())
                    .expect("worker thread")
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("worker")).collect()
    });

    let mut wrong: Vec<String> = Vec::new();
    let (mut pure, mut io_with_fs, mut io) = (0usize, 0usize, 0usize);
    for row in &rows {
        let row = match row {
            Ok(r) => r,
            Err(e) => {
                wrong.push(e.clone());
                continue;
            }
        };
        let ok = allowed(&row.effects);
        let extra: Vec<&String> = row.imports.iter().filter(|i| !ok.contains(i.as_str())).collect();
        if !extra.is_empty() {
            wrong.push(format!("{}: effects {:?} do not allow the import(s) {:?}", row.name, row.effects, extra));
        }
        match &row.raw_ops {
            None => wrong.push(format!("{}: an `fs_call` site's op is not a visible constant, so the embedded host cannot bound it", row.name)),
            Some(ops) => {
                let ok = allowed_caps(&row.effects);
                let over: Vec<(i32, u32)> = ops
                    .iter()
                    .flat_map(|&op| op_caps(op).iter().map(move |&c| (op, c)))
                    .filter(|(_, c)| !ok.contains(c))
                    .collect();
                if !over.is_empty() {
                    wrong.push(format!("{}: effects {:?} do not allow the in-process (op, capability) pair(s) {:?}", row.name, row.effects, over));
                }
            }
        }
        if row.effects.iter().all(|e| CONSOLE_ONLY.contains(&e.as_str())) {
            pure += 1;
        }
        if FILE_LEAVES.iter().any(|l| row.effects.contains(*l)) {
            io += 1;
            if FILE_SYSTEM.iter().any(|c| row.imports.contains(*c)) {
                io_with_fs += 1;
            }
        }
    }
    eprintln!(
        "wasm imports within effects: {} fixtures, {pure} with no effect, {io} with IO ({io_with_fs} importing the file system), {} wrong",
        rows.len(),
        wrong.len()
    );
    assert!(pure > 600, "only {pure} effect-free fixtures were judged — the effect reader went blind");
    assert!(io_with_fs >= 15, "only {io_with_fs} IO fixtures import the file system — the import reader went blind");
    assert!(wrong.is_empty(), "{} fixture(s):\n{}", wrong.len(), wrong.join("\n"));
}
