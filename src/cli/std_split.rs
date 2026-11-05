//! The `--release` link with std's backtrace printer precompiled (#4459).
//!
//! A `--release` native build runs rustc's fat LTO over the program, the
//! runtime and std in one module. Measured on fib (c4-standard-8, rustc
//! 1.99.1), three quarters of what that module still holds after LTO is the
//! code that prints a panic's backtrace under `RUST_BACKTRACE`: the DWARF
//! reader (gimli, addr2line, object, miniz_oxide), the symbol demangler and
//! the frame printer — 50K of the 66K instructions LTO optimizes and
//! code-generates on every build. No program calls into it: it is reached
//! only from std's default panic hook, through one function,
//! `<BacktraceLock::print::DisplayBacktrace as Display>::fmt`, and the
//! symbolizer's cache, `Cache::with_global::<resolve::{closure}>`.
//!
//! The split takes those two functions out of the LTO module and links them
//! precompiled, once per toolchain:
//!
//! * a sysroot whose libstd carries the two functions as declarations in its
//!   bitcode (every other rlib is a symlink, and libstd's machine code and
//!   metadata are untouched), so the LTO module no longer reaches the
//!   printer, and
//! * one object, `backtrace.o`, holding the same two functions compiled from
//!   the same std bitcode with everything they reach as private copies —
//!   except the per-thread panic count, which it references in the program's
//!   std ([`SHARED_TLS`]), so the thread-local block is today's byte for byte.
//!   Its definitions are weak: in a link without LTO (a build script of a
//!   cargo dependency) std's own definition wins.
//!
//! What the program's own functions see is unchanged: the printer is not in
//! any of their call graphs, and their machine code compares equal function by
//! function, thread-local offsets included, with
//! the same instruction count to within 0.003% (cachegrind, fib, strbuild,
//! regexcycle, decode). What moves is the layout: `.text` starts elsewhere and
//! the std functions after the program's are packed differently. On the
//! layout-sensitive perf rows that alone reads as -16%..+9% for ONE layout,
//! and the plain profile itself spans 10-13% on those rows when only its
//! function order is shuffled (lld `--shuffle-sections`). Over 16 shuffled
//! layouts of each profile the rows are equal (c4-standard-8, #4459).
//!
//! A panic prints the same text under every `RUST_BACKTRACE` setting, and that
//! is checked, not assumed: the cache is only marked ready after a probe
//! program built both ways printed the same panic output under unset, `0`,
//! `1` and `full` (thread ids and addresses normalized).
//!
//! The transform needs the `llvm-tools` rustup component (`opt`, `llc`,
//! `llvm-dis`, `llvm-as`, `llvm-objcopy`, `llvm-ar`). Without it, on any
//! host but Linux, for a cross build, when either function is not found in
//! std's bitcode, or when any step or the probe fails, the build uses the
//! plain fat-LTO profile — slower, never different. `ALMIDE_RELEASE_STD_UNSPLIT=1`
//! forces that profile (the negative control of the build-time ratchet).

use std::path::{Path, PathBuf};

/// A ready split for this toolchain: pass `--sysroot <sysroot>` and link `object`.
pub(super) struct StdSplit {
    pub sysroot: PathBuf,
    pub object: PathBuf,
}

impl StdSplit {
    /// The rustc arguments a fat-LTO `--release` link adds.
    pub(super) fn rustc_args(&self) -> Vec<String> {
        vec![
            "--sysroot".to_string(),
            self.sysroot.display().to_string(),
            "-C".to_string(),
            format!("link-arg={}", self.object.display()),
        ]
    }
}

/// Bumped whenever what the cache holds changes shape.
const SPLIT_FORMAT: u32 = 2;

/// The LLVM tools the transform runs, from the toolchain's own `llvm-tools`.
const TOOLS: [&str; 7] = ["llvm-ar", "llvm-objcopy", "llvm-dis", "llvm-as", "llvm-nm", "opt", "llc"];

/// The one thread-local the printer shares with the rest of std: the
/// per-thread panic count (`std::panicking::panic_count::LOCAL_PANIC_COUNT`).
/// `backtrace.o` references the LTO module's copy instead of carrying its own,
/// so the program's thread-local block keeps today's layout byte for byte and
/// a panic inside the printer sees the count the panic it prints raised. The
/// split libstd lists it in `@llvm.used` with hidden linkage, which keeps it a
/// symbol through LTO's internalization.
const SHARED_TLS: &[&str] = &["9panicking11panic_count17LOCAL_PANIC_COUNT", "RUST_STD_INTERNAL_VAL"];

/// One function taken out of the LTO module, found by the v0-mangled path
/// segments its symbol must contain (the crate hash differs per toolchain).
struct Cut {
    what: &'static str,
    segments: &'static [&'static str],
}

const CUTS: [Cut; 2] = [
    Cut {
        what: "<BacktraceLock::print::DisplayBacktrace as Display>::fmt",
        segments: &["3sys9backtrace", "13BacktraceLock5print", "16DisplayBacktrace", "7Display3fmt"],
    },
    Cut {
        what: "<backtrace_rs::symbolize::gimli::Cache>::with_global::<resolve::{closure}>",
        segments: &["12backtrace_rs9symbolize5gimli", "5Cache11with_global", "7resolve"],
    },
];

/// The split for this process's toolchain, built on first use and cached on
/// disk; `None` means "link with the plain profile".
pub(super) fn release_std_split() -> Option<&'static StdSplit> {
    static SPLIT: std::sync::OnceLock<Option<StdSplit>> = std::sync::OnceLock::new();
    SPLIT
        .get_or_init(|| {
            if !cfg!(target_os = "linux")
                || almide_base::env::flag("ALMIDE_RELEASE_STD_UNSPLIT")
                || super::native_target::cross_target().is_some()
            {
                return None;
            }
            match ensure_split() {
                Ok(split) => split,
                Err(e) => {
                    if almide_base::env::flag("ALMIDE_DEBUG_STD_SPLIT") {
                        eprintln!("almide: release std split unavailable: {e}");
                    }
                    None
                }
            }
        })
        .as_ref()
}

/// The toolchain's sysroot, host triple and the directory its `llvm-tools` live in.
struct Toolchain {
    sysroot: PathBuf,
    host: String,
}

impl Toolchain {
    fn detect() -> Result<Self, String> {
        let rustc = crate::find_rustc();
        let out = std::process::Command::new(&rustc)
            .args(["--print", "sysroot"])
            .output()
            .map_err(|e| format!("rustc --print sysroot: {e}"))?;
        if !out.status.success() {
            return Err("rustc --print sysroot failed".to_string());
        }
        let sysroot = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        let host = super::crate_inputs::toolchain_identity()
            .split('|')
            .find_map(|l| l.strip_prefix("host: "))
            .ok_or("rustc -vV names no host")?
            .to_string();
        Ok(Toolchain { sysroot, host })
    }
    fn target_dir(&self) -> PathBuf {
        self.sysroot.join("lib").join("rustlib").join(&self.host)
    }
    fn lib_dir(&self) -> PathBuf {
        self.target_dir().join("lib")
    }
    fn tool(&self, name: &str) -> PathBuf {
        self.target_dir().join("bin").join(name)
    }
}

fn ensure_split() -> Result<Option<StdSplit>, String> {
    let tc = Toolchain::detect()?;
    if let Some(missing) = TOOLS.iter().find(|t| !tc.tool(t).exists()) {
        return Err(format!("{missing} not found (rustup component add llvm-tools)"));
    }
    let std_rlib = single_std_rlib(&tc.lib_dir())?;
    let rlib_len = std::fs::metadata(&std_rlib).map_err(|e| format!("{}: {e}", std_rlib.display()))?.len();
    let key = format!(
        "{:016x}",
        super::hash64(
            format!("{SPLIT_FORMAT}|{}|{}|{rlib_len}", super::crate_inputs::toolchain_identity(), std_rlib.display())
                .as_bytes()
        )
    );
    // Under the rtlib prefix, so the weekly sweep of unused runtime caches
    // (#2504) empties a split no build has linked for a week.
    let dir = super::build_dir::cache_base().join(format!("{}stdsplit-{key}", super::build_dir::RTLIB_DIR_PREFIX));
    let ready = dir.join("READY");
    let failed = dir.join("FAILED");
    let split = || StdSplit { sysroot: dir.join("sysroot"), object: dir.join("backtrace.o") };
    let use_ready = |s: StdSplit| {
        super::build_dir::touch_used(&ready);
        Ok(Some(s))
    };
    if ready.exists() {
        return use_ready(split());
    }
    if failed.exists() {
        return Ok(None);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let _lock = super::build_dir::BuildDirLock::acquire(&dir)?;
    if ready.exists() {
        return use_ready(split());
    }
    if failed.exists() {
        return Ok(None);
    }
    match build_split(&tc, &std_rlib, &dir) {
        Ok(()) => {
            std::fs::write(&ready, b"").map_err(|e| format!("{}: {e}", ready.display()))?;
            let _ = std::fs::remove_dir_all(dir.join("work"));
            Ok(Some(split()))
        }
        Err(e) => {
            // A failure is a property of this toolchain (a std whose printer
            // moved, a probe that printed differently): recorded, so every
            // later build takes the plain profile without retrying.
            let _ = std::fs::write(&failed, e.as_bytes());
            if !almide_base::env::flag("ALMIDE_DEBUG_STD_SPLIT") {
                let _ = std::fs::remove_dir_all(dir.join("work"));
            }
            Err(e)
        }
    }
}

fn single_std_rlib(lib: &Path) -> Result<PathBuf, String> {
    let found: Vec<PathBuf> = std::fs::read_dir(lib)
        .map_err(|e| format!("{}: {e}", lib.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            n.starts_with("libstd-") && n.ends_with(".rlib")
        })
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        _ => Err(format!("expected one libstd rlib in {}, found {}", lib.display(), found.len())),
    }
}

fn run(cmd: &mut std::process::Command) -> Result<std::process::Output, String> {
    let out = cmd.output().map_err(|e| format!("{cmd:?}: {e}"))?;
    if out.status.success() {
        Ok(out)
    } else {
        Err(format!("{cmd:?} failed: {}", String::from_utf8_lossy(&out.stderr).chars().take(2000).collect::<String>()))
    }
}

fn build_split(tc: &Toolchain, std_rlib: &Path, dir: &Path) -> Result<(), String> {
    let work = dir.join("work");
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(dir.join("sysroot"));
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let (names, shared, std_unpacked) = build_sysroot(tc, std_rlib, dir, &work)?;
    let merged = seed_lto_input(tc, &work)?;
    let object = dir.join("backtrace.o");
    build_object(tc, &work, &merged, &names, &shared, &Default::default(), &object)?;
    // The printer's private copies of std functions the program keeps anyway
    // (fmt, the panic path, str search…) would be linked twice: once from the
    // LTO module, once from the object (#4497). Find them by linking the seed
    // against the split, make them symbols of the split std that the object
    // references, and rebuild the object without its copies.
    let twice = defined_twice(tc, dir, &work)?;
    let external = share_with_program(tc, dir, &work, std_unpacked, &twice)?;
    if !external.is_empty() {
        build_object(tc, &work, &merged, &names, &shared, &external, &object)?;
    }
    if almide_base::env::flag("ALMIDE_DEBUG_STD_SPLIT") {
        let left = defined_twice(tc, dir, &work)?;
        eprintln!(
            "almide: std split: {} functions defined twice, {} now shared, {} left: {:?}",
            twice.len(),
            external.len(),
            left.len(),
            left
        );
    }
    probe(tc, dir, &work)
}

/// An rlib unpacked into its own work directory: its members in order, and
/// each object member's embedded bitcode disassembled next to it.
struct Unpacked {
    dir: PathBuf,
    members: Vec<String>,
    /// The object members that carry bitcode (`<member>.ll` exists).
    with_bitcode: Vec<String>,
}

impl Unpacked {
    fn new(tc: &Toolchain, rlib: &Path, dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let listed = run(std::process::Command::new(tc.tool("llvm-ar")).arg("t").arg(rlib))?;
        let members: Vec<String> = String::from_utf8_lossy(&listed.stdout).lines().map(str::to_string).collect();
        run(std::process::Command::new(tc.tool("llvm-ar")).arg("x").arg(rlib).current_dir(&dir))?;
        let mut with_bitcode = Vec::new();
        for member in members.iter().filter(|m| m.ends_with(".o")) {
            let obj = dir.join(member);
            let bc = dir.join(format!("{member}.bc"));
            // A member without `.llvmbc` (none in std's own rlibs today) has
            // nothing to rewrite.
            let dumped = std::process::Command::new(tc.tool("llvm-objcopy"))
                .arg(format!("--dump-section=.llvmbc={}", bc.display()))
                .arg(&obj)
                .output()
                .map_err(|e| format!("llvm-objcopy: {e}"))?;
            if !dumped.status.success() {
                continue;
            }
            run(std::process::Command::new(tc.tool("llvm-dis")).arg(&bc).arg("-o").arg(dir.join(format!("{member}.ll"))))?;
            with_bitcode.push(member.clone());
        }
        Ok(Unpacked { dir, members, with_bitcode })
    }
    fn ll(&self, member: &str) -> PathBuf {
        self.dir.join(format!("{member}.ll"))
    }
    fn read(&self, member: &str) -> Result<String, String> {
        std::fs::read_to_string(self.ll(member)).map_err(|e| format!("{}: {e}", self.ll(member).display()))
    }
    /// Write `text` as `member`'s new bitcode.
    fn replace(&self, tc: &Toolchain, member: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.ll(member), text).map_err(|e| format!("{}: {e}", self.ll(member).display()))?;
        let bc = self.dir.join(format!("{member}.split.bc"));
        run(std::process::Command::new(tc.tool("llvm-as")).arg(self.ll(member)).arg("-o").arg(&bc))?;
        run(std::process::Command::new(tc.tool("llvm-objcopy"))
            .arg(format!("--update-section=.llvmbc={}", bc.display()))
            .arg(self.dir.join(member)))?;
        Ok(())
    }
    /// The members, in their order, as a new rlib at `out` (a symlink there is replaced).
    fn pack(&self, tc: &Toolchain, out: &Path) -> Result<(), String> {
        let _ = std::fs::remove_file(out);
        run(std::process::Command::new(tc.tool("llvm-ar")).arg("rcs").arg(out).args(&self.members).current_dir(&self.dir))?;
        Ok(())
    }
}

/// The split sysroot: every entry of the toolchain's target dir linked, libstd
/// rewritten with the cut functions declared instead of defined in its
/// bitcode and the panic count kept a symbol. Returns the cut symbols, in
/// [`CUTS`] order, the panic count's, and libstd unpacked.
fn build_sysroot(tc: &Toolchain, std_rlib: &Path, dir: &Path, work: &Path) -> Result<(Vec<String>, String, Unpacked), String> {
    let target = dir.join("sysroot").join("lib").join("rustlib").join(&tc.host);
    let lib = target.join("lib");
    std::fs::create_dir_all(&lib).map_err(|e| format!("{}: {e}", lib.display()))?;
    for entry in std::fs::read_dir(tc.target_dir()).map_err(|e| format!("{}: {e}", tc.target_dir().display()))?.flatten() {
        if entry.file_name() != "lib" {
            symlink(&entry.path(), &target.join(entry.file_name()))?;
        }
    }
    for entry in std::fs::read_dir(tc.lib_dir()).map_err(|e| format!("{}: {e}", tc.lib_dir().display()))?.flatten() {
        if entry.path() != std_rlib {
            symlink(&entry.path(), &lib.join(entry.file_name()))?;
        }
    }

    // The rlib's members in order; the object members carry std's bitcode
    // in their `.llvmbc` section.
    let unpacked = Unpacked::new(tc, std_rlib, work.join("rlib-std"))?;
    let mut names: Vec<Option<String>> = vec![None; CUTS.len()];
    let mut shared: Option<String> = None;
    for member in &unpacked.with_bitcode {
        let mut patched = unpacked.read(member)?;
        let mut touched = false;
        if let Some(name) = find_global(&patched, SHARED_TLS)? {
            if shared.is_some() {
                return Err("the panic count is defined in two of libstd's objects".to_string());
            }
            patched = keep_global(&patched, &name)?;
            shared = Some(name);
            touched = true;
        }
        for (i, cut) in CUTS.iter().enumerate() {
            let Some(name) = find_define(&patched, cut)? else { continue };
            if names[i].is_some() {
                return Err(format!("{} is defined in two of libstd's objects", cut.what));
            }
            patched = declare_instead(&patched, &name)?;
            names[i] = Some(name);
            touched = true;
        }
        if touched {
            unpacked.replace(tc, member, &patched)?;
        }
    }
    let names: Vec<String> = names
        .into_iter()
        .zip(CUTS.iter())
        .map(|(n, cut)| n.ok_or_else(|| format!("{} is not in libstd's bitcode", cut.what)))
        .collect::<Result<_, _>>()?;
    let shared = shared.ok_or("the panic count is not in libstd's bitcode")?;
    unpacked.pack(tc, &lib.join(std_rlib.file_name().ok_or("libstd has no file name")?))?;
    Ok((names, shared, unpacked))
}

/// The function symbols the seed, linked against the split, defines twice:
/// once in its LTO module, once as a private copy in `backtrace.o`.
fn defined_twice(tc: &Toolchain, dir: &Path, work: &Path) -> Result<std::collections::BTreeSet<String>, String> {
    let split = StdSplit { sysroot: dir.join("sysroot"), object: dir.join("backtrace.o") };
    let bin = work.join("seed").join("seed-split");
    run(std::process::Command::new(crate::find_rustc())
        .arg(work.join("seed.rs"))
        .args(["--edition", "2021", "-C", "opt-level=3", "-C", "lto=fat", "-C", "codegen-units=1", "-C", "strip=debuginfo"])
        .args(split.rustc_args())
        .arg("-o")
        .arg(&bin))?;
    let in_object = defined_functions(tc, &split.object)?;
    let mut count: std::collections::HashMap<String, usize> = Default::default();
    for name in defined_function_list(tc, &bin)? {
        *count.entry(name).or_default() += 1;
    }
    Ok(in_object.into_iter().filter(|n| count.get(n).is_some_and(|c| *c > 1)).collect())
}

fn defined_functions(tc: &Toolchain, file: &Path) -> Result<std::collections::BTreeSet<String>, String> {
    Ok(defined_function_list(tc, file)?.into_iter().collect())
}

/// Every function symbol `file` defines, one entry per definition.
fn defined_function_list(tc: &Toolchain, file: &Path) -> Result<Vec<String>, String> {
    let out = run(std::process::Command::new(tc.tool("llvm-nm")).arg("--defined-only").arg(file))?;
    Ok(nm_functions(&String::from_utf8_lossy(&out.stdout)))
}

/// The function names of `llvm-nm --defined-only` output (`addr type name`, text types).
fn nm_functions(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let (_, ty, name) = (w.next()?, w.next()?, w.next()?);
            matches!(ty, "t" | "T" | "w" | "W").then(|| name.to_string())
        })
        .collect()
}

/// Make each of `wanted` a hidden symbol of the split sysroot's std crates,
/// kept through LTO (`@llvm.used`), so `backtrace.o` can reference
/// the program's copy instead of carrying its own. A function is shared only
/// when exactly one object of the crates that could hold it defines it (a
/// name with two definitions — two private copies in two codegen units — is
/// left alone). Returns the names shared.
fn share_with_program(
    tc: &Toolchain,
    dir: &Path,
    work: &Path,
    std_unpacked: Unpacked,
    wanted: &std::collections::BTreeSet<String>,
) -> Result<std::collections::HashSet<String>, String> {
    if wanted.is_empty() {
        return Ok(Default::default());
    }
    let lib = dir.join("sysroot").join("lib").join("rustlib").join(&tc.host).join("lib");
    // The crates the names mention (`4core`, `5alloc`, …): a function is
    // defined in its own crate or, for a generic instance, in a crate its
    // symbol names as the instantiating one.
    let mut rlibs: Vec<(PathBuf, Unpacked)> = Vec::new();
    let std_name = std_unpacked_name(tc)?;
    for entry in std::fs::read_dir(tc.lib_dir()).map_err(|e| format!("{}: {e}", tc.lib_dir().display()))?.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        let Some(krate) = file.strip_prefix("lib").and_then(|f| f.strip_suffix(".rlib")).and_then(|f| f.rsplit_once('-')).map(|(c, _)| c.to_string()) else {
            continue;
        };
        if file == std_name {
            continue;
        }
        let seg = format!("{}{krate}", krate.len());
        if wanted.iter().any(|n| n.contains(&seg)) {
            let u = Unpacked::new(tc, &entry.path(), work.join(format!("rlib-{krate}")))?;
            rlibs.push((lib.join(&file), u));
        }
    }
    rlibs.push((lib.join(&std_name), std_unpacked));
    // Where each wanted name is defined: (rlib index, member).
    let mut sites: std::collections::HashMap<&str, Vec<(usize, String)>> = Default::default();
    let mut texts: std::collections::HashMap<(usize, String), String> = Default::default();
    for (i, (_, u)) in rlibs.iter().enumerate() {
        for member in &u.with_bitcode {
            let text = u.read(member)?;
            let mut hit = false;
            for line in text.lines() {
                if let Some(name) = define_name(line).and_then(|n| wanted.get(n)) {
                    sites.entry(name.as_str()).or_default().push((i, member.clone()));
                    hit = true;
                }
            }
            if hit {
                texts.insert((i, member.clone()), text);
            }
        }
    }
    let unique: std::collections::HashSet<String> =
        sites.iter().filter(|(_, s)| s.len() == 1).map(|(n, _)| n.to_string()).collect();
    let mut changed: std::collections::BTreeSet<usize> = Default::default();
    for ((i, member), text) in texts {
        let patched = export_defines(&text, &unique)?;
        if patched != text {
            rlibs[i].1.replace(tc, &member, &patched)?;
            changed.insert(i);
        }
    }
    for i in changed {
        let (out, u) = &rlibs[i];
        u.pack(tc, out)?;
    }
    Ok(unique)
}

fn std_unpacked_name(tc: &Toolchain) -> Result<String, String> {
    Ok(single_std_rlib(&tc.lib_dir())?.file_name().ok_or("libstd has no file name")?.to_string_lossy().into_owned())
}

/// An IR reference to the global `name`, quoted when it is not a bare identifier.
fn ir_ref(name: &str) -> String {
    let bare = name.chars().next().is_some_and(|c| !c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$' | '-'));
    if bare { format!("@{name}") } else { format!("@\"{name}\"") }
}

/// Give every definition of a name in `names` external hidden linkage when it
/// is module-local, and list them all in `@llvm.used`, so LTO keeps
/// each one a symbol. (`@llvm.compiler.used` is not enough: rustc's LTO
/// restriction pass internalizes its members.)
fn export_defines(text: &str, names: &std::collections::HashSet<String>) -> Result<String, String> {
    let mut out = String::with_capacity(text.len() + 4096);
    let mut kept: Vec<String> = Vec::new();
    let mut listed = false;
    for line in text.lines() {
        if let Some(name) = define_name(line).filter(|n| names.contains(*n)) {
            let local = line.strip_prefix("define ").is_some_and(|r| r.starts_with("internal ") || r.starts_with("private "));
            if local {
                out.push_str("define hidden ");
                out.push_str(strip_linkage(line));
            } else {
                out.push_str(line);
            }
            kept.push(ir_ref(name));
        } else if line.starts_with("@llvm.used = ") {
            listed = true;
            out.push_str("@@USED@@");
            out.push_str(line);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if kept.is_empty() {
        return Ok(text.to_string());
    }
    let extra = kept.iter().map(|r| format!("ptr {r}")).collect::<Vec<_>>().join(", ");
    if listed {
        let mut fixed = String::with_capacity(out.len() + extra.len());
        for line in out.lines() {
            if let Some(used) = line.strip_prefix("@@USED@@") {
                fixed.push_str(&append_used(used, kept.len(), &extra)?);
            } else {
                fixed.push_str(line);
            }
            fixed.push('\n');
        }
        Ok(fixed)
    } else {
        out.push_str(&format!("@llvm.used = appending global [{} x ptr] [{extra}], section \"llvm.metadata\"\n", kept.len()));
        Ok(out)
    }
}

/// `@llvm.used = appending global [N x ptr] [a, b], section …` with `n` more entries.
fn append_used(line: &str, n: usize, extra: &str) -> Result<String, String> {
    let (count, rest) = line
        .strip_prefix("@llvm.used = appending global [")
        .and_then(|r| r.split_once(" x ptr] ["))
        .ok_or("malformed @llvm.used")?;
    let count: usize = count.parse().map_err(|_| "malformed @llvm.used count")?;
    Ok(format!("@llvm.used = appending global [{} x ptr] [{extra}, {rest}", count + n))
}

#[cfg(unix)]
fn symlink(from: &Path, to: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(from, to).map_err(|e| format!("symlink {}: {e}", to.display()))
}

#[cfg(not(unix))]
fn symlink(_from: &Path, to: &Path) -> Result<(), String> {
    Err(format!("no symlinks on this host ({})", to.display()))
}

/// The merged LTO input of a seed build with the plain sysroot (std, its
/// dependencies and rustc's allocator shim), as IR text.
fn seed_lto_input(tc: &Toolchain, work: &Path) -> Result<String, String> {
    let src = work.join("seed.rs");
    std::fs::write(&src, "fn main() { println!(\"{}\", std::env::args().count()); }\n").map_err(|e| format!("{e}"))?;
    let seed_dir = work.join("seed");
    std::fs::create_dir_all(&seed_dir).map_err(|e| format!("{}: {e}", seed_dir.display()))?;
    run(std::process::Command::new(crate::find_rustc())
        .arg(&src)
        .args(["--edition", "2021", "-C", "opt-level=3", "-C", "lto=fat", "-C", "codegen-units=1", "-C", "save-temps"])
        .arg("-o")
        .arg(seed_dir.join("seed"))
        .current_dir(&seed_dir))?;
    let input = std::fs::read_dir(&seed_dir)
        .map_err(|e| format!("{e}"))?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().ends_with(".lto.input.bc"))
        .ok_or("the seed build left no .lto.input.bc")?;
    let ll = work.join("merged.ll");
    run(std::process::Command::new(tc.tool("llvm-dis")).arg(&input).arg("-o").arg(&ll))?;
    std::fs::read_to_string(&ll).map_err(|e| format!("{}: {e}", ll.display()))
}

/// `backtrace.o`: the cut functions from the seed's merged LTO input,
/// everything they reach internalized into private copies — except the
/// functions in `external`, which the split std keeps as symbols and the
/// object references — the cut functions themselves weak and hidden.
fn build_object(
    tc: &Toolchain,
    work: &Path,
    text: &str,
    names: &[String],
    shared: &str,
    external: &std::collections::HashSet<String>,
    out: &Path,
) -> Result<(), String> {
    let ll = work.join("merged.ll");
    let mut kept = String::with_capacity(text.len());
    let mut exported = vec![false; names.len()];
    let mut referenced = false;
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if global_name(line) == Some(shared) {
            kept.push_str(&external_global(line)?);
            kept.push('\n');
            referenced = true;
            continue;
        }
        if ["@llvm.used = ", "@llvm.compiler.used = ", "@llvm.global_ctors = ", "@llvm.global_dtors = "]
            .iter()
            .any(|p| line.starts_with(p))
        {
            continue;
        }
        if let Some(i) = names.iter().position(|n| defines(line, n)) {
            kept.push_str(&format!("define weak hidden {}", strip_linkage(line)));
            exported[i] = true;
        } else if define_name(line).is_some_and(|n| external.contains(n)) {
            let head = strip_linkage(line).trim_end();
            let head = head.strip_suffix('{').unwrap_or(head).trim_end();
            kept.push_str("declare hidden ");
            kept.push_str(&declaration_head(head));
            for body in lines.by_ref() {
                if body.starts_with('}') {
                    break;
                }
            }
        } else if line.starts_with("attributes #") && line.contains("\"target-cpu\"") {
            // Function attribute groups: size over speed, the printer runs
            // once per panic.
            kept.push_str(&line.replacen("= { ", "= { optsize ", 1));
        } else {
            kept.push_str(line);
        }
        kept.push('\n');
    }
    if exported.iter().any(|e| !e) || !referenced {
        return Err("a cut function or the panic count is missing from the seed's LTO input".to_string());
    }
    std::fs::write(&ll, kept).map_err(|e| format!("{}: {e}", ll.display()))?;
    let bc = work.join("merged.bc");
    run(std::process::Command::new(tc.tool("llvm-as")).arg(&ll).arg("-o").arg(&bc))?;
    let opt_bc = work.join("backtrace.bc");
    run(std::process::Command::new(tc.tool("opt"))
        .arg("-passes=internalize,globaldce,default<O2>")
        .arg(format!("-internalize-public-api-list={}", names.join(",")))
        .arg(&bc)
        .arg("-o")
        .arg(&opt_bc))?;
    run(std::process::Command::new(tc.tool("llc"))
        .args(["-O2", "-filetype=obj", "-relocation-model=pic", "-function-sections", "-data-sections"])
        .arg(&opt_bc)
        .arg("-o")
        .arg(out))?;
    Ok(())
}

/// Build a panicking probe with the plain sysroot and with the split one,
/// and require the same panic output from both under every `RUST_BACKTRACE`
/// setting. Thread ids and addresses differ run to run and are normalized.
fn probe(_tc: &Toolchain, dir: &Path, work: &Path) -> Result<(), String> {
    let src = work.join("probe.rs");
    std::fs::write(
        &src,
        "#[inline(never)] fn inner(v: &[i64], i: usize) -> i64 { v[i] }\n\
         #[inline(never)] fn outer(n: usize) -> i64 { let v = vec![1, 2, 3]; inner(&v, n + 7) }\n\
         fn main() { println!(\"start\"); let n = std::env::args().count(); println!(\"{}\", outer(n)); }\n",
    )
    .map_err(|e| format!("{e}"))?;
    let split = StdSplit { sysroot: dir.join("sysroot"), object: dir.join("backtrace.o") };
    let build = |out: &Path, extra: &[String]| {
        run(std::process::Command::new(crate::find_rustc())
            .arg(&src)
            // The host's generic CPU: the probe runs here, whatever `--target-cpu` the builds ask for.
            .args(["--edition", "2021", "-C", "opt-level=3", "-C", "lto=fat", "-C", "codegen-units=1", "-C", "strip=debuginfo"])
            .args(extra)
            .arg("-o")
            .arg(out))
    };
    let plain = work.join("probe-plain");
    let cut = work.join("probe-split");
    build(&plain, &[])?;
    build(&cut, &split.rustc_args())?;
    for setting in [None, Some("0"), Some("1"), Some("full")] {
        let (a, b) = (panic_output(&plain, setting)?, panic_output(&cut, setting)?);
        if a != b {
            return Err(format!("the probe's panic output differs under RUST_BACKTRACE={setting:?}:\n--- plain\n{a}\n--- split\n{b}"));
        }
    }
    Ok(())
}

fn panic_output(bin: &Path, setting: Option<&str>) -> Result<String, String> {
    let mut cmd = std::process::Command::new(bin);
    cmd.env_remove("RUST_BACKTRACE").env_remove("RUST_LIB_BACKTRACE");
    if let Some(s) = setting {
        cmd.env("RUST_BACKTRACE", s);
    }
    let out = cmd.output().map_err(|e| format!("{}: {e}", bin.display()))?;
    Ok(format!(
        "status={:?}\n{}\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        normalize_panic_text(&String::from_utf8_lossy(&out.stderr))
    ))
}

/// Thread ids (`thread 'main' (1234) panicked`) and hex addresses vary per run.
fn normalize_panic_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("0x") {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 2..];
        let n = tail.bytes().take_while(u8::is_ascii_hexdigit).count();
        out.push_str(if n > 0 { "0xADDR" } else { "0x" });
        rest = &tail[n..];
    }
    out.push_str(rest);
    out.lines()
        .map(|l| match (l.find("' ("), l.find(") panicked")) {
            (Some(a), Some(b)) if a < b && l.starts_with("thread '") => format!("{}' (TID{}", &l[..a], &l[b..]),
            _ => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The name of a global variable's definition line (`@name = …`), if it is one.
fn global_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('@')?;
    let end = rest.find(" = ")?;
    let name = &rest[..end];
    let tail = &rest[end + 3..];
    (tail.contains(" global ") || tail.starts_with("global ")).then_some(name.trim_matches('"'))
}

/// The one thread-local definition whose name carries every segment; two is an error.
fn find_global(text: &str, segments: &[&str]) -> Result<Option<String>, String> {
    let mut hits = text
        .lines()
        .filter(|l| l.contains("thread_local") && !l.contains("= external "))
        .filter_map(global_name)
        .filter(|n| segments.iter().all(|s| n.contains(s)));
    let first = hits.next().map(str::to_string);
    if hits.next().is_some() {
        return Err("the panic count matches more than one thread-local".to_string());
    }
    Ok(first)
}

/// Give the definition of `name` hidden external linkage and list it in
/// `@llvm.used`, so LTO keeps it a symbol another object can reference.
fn keep_global(text: &str, name: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len() + 128);
    let (mut defined, mut listed) = (false, false);
    for line in text.lines() {
        if global_name(line) == Some(name) {
            let (head, tail) = line.split_once(" = ").ok_or("malformed global")?;
            let mut tail = tail;
            loop {
                let w = tail.split(' ').next().unwrap_or("");
                if matches!(w, "internal" | "private" | "hidden" | "dso_local") {
                    tail = tail[w.len()..].trim_start();
                } else {
                    break;
                }
            }
            out.push_str(&format!("{head} = hidden {tail}"));
            defined = true;
        } else if let Some(rest) = line.strip_prefix("@llvm.used = appending global [") {
            let (n, rest) = rest.split_once(" x ptr] [").ok_or("malformed @llvm.used")?;
            let n: usize = n.parse().map_err(|_| "malformed @llvm.used count")?;
            out.push_str(&format!("@llvm.used = appending global [{} x ptr] [ptr @{name}, {rest}", n + 1));
            listed = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !defined {
        return Err(format!("{name} has no definition"));
    }
    if !listed {
        out.push_str(&format!("@llvm.used = appending global [1 x ptr] [ptr @{name}], section \"llvm.metadata\"\n"));
    }
    Ok(out)
}

/// A thread-local definition as a hidden external declaration of the same
/// type, thread-local model and alignment.
fn external_global(line: &str) -> Result<String, String> {
    let (head, tail) = line.split_once(" = ").ok_or("malformed global")?;
    let tls = tail.split(' ').find(|w| w.starts_with("thread_local")).ok_or("not a thread-local")?;
    let after = tail.split_once(" global ").ok_or("malformed global")?.1;
    let ty = leading_type(after).ok_or("cannot read the global's type")?;
    let align = tail
        .split(", ")
        .find_map(|p| p.strip_prefix("align "))
        .map(|a| format!(", align {}", a.split_whitespace().next().unwrap_or("8")))
        .unwrap_or_default();
    Ok(format!("{head} = external hidden {tls} global {ty}{align}"))
}

/// The type an IR value starts with: a bracketed aggregate (`<{ … }>`,
/// `{ … }`, `[ … ]`) or one word.
fn leading_type(s: &str) -> Option<&str> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '{' | '[' => depth += 1,
            '>' | '}' | ']' => {
                depth -= 1;
                if depth == 0 && !(c == '}' && s[i + 1..].starts_with('>')) {
                    return Some(&s[..=i]);
                }
            }
            ' ' if depth == 0 => return Some(&s[..i]),
            _ => {}
        }
    }
    None
}

/// The function name of a `define` line, if it is one.
fn define_name(line: &str) -> Option<&str> {
    if !line.starts_with("define ") {
        return None;
    }
    let at = line.find(" @")? + 2;
    let rest = &line[at..];
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    let end = rest.find(['(', '"'])?;
    Some(&rest[..end])
}

fn defines(line: &str, name: &str) -> bool {
    define_name(line) == Some(name)
}

/// The one `define` whose symbol carries every segment of `cut`; two is an error.
fn find_define(text: &str, cut: &Cut) -> Result<Option<String>, String> {
    let mut hits = text
        .lines()
        .filter_map(define_name)
        .filter(|n| n.starts_with("_R") && cut.segments.iter().all(|s| n.contains(s)));
    let first = hits.next().map(str::to_string);
    if hits.next().is_some() {
        return Err(format!("{} matches more than one function", cut.what));
    }
    Ok(first)
}

/// `define <linkage…> rest` → `rest`: the leading linkage, preemption and
/// visibility words a definition may carry.
fn strip_linkage(line: &str) -> &str {
    let mut rest = line.strip_prefix("define ").unwrap_or(line);
    loop {
        let word = rest.split(' ').next().unwrap_or("");
        if matches!(
            word,
            "internal" | "private" | "external" | "weak" | "weak_odr" | "linkonce" | "linkonce_odr" | "available_externally"
                | "hidden" | "protected" | "default" | "dso_local" | "dso_preemptable"
        ) {
            rest = rest[word.len()..].trim_start();
        } else {
            return rest;
        }
    }
}

/// Replace the definition of `name` with a hidden external declaration.
fn declare_instead(text: &str, name: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut lines = text.lines();
    let mut done = false;
    while let Some(line) = lines.next() {
        if !done && defines(line, name) {
            let head = strip_linkage(line).trim_end();
            let head = head.strip_suffix('{').unwrap_or(head).trim_end();
            out.push_str("declare hidden ");
            out.push_str(&declaration_head(head));
            out.push('\n');
            for body in lines.by_ref() {
                if body.starts_with('}') {
                    break;
                }
            }
            done = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if done { Ok(out) } else { Err(format!("{name} has no definition to declare")) }
}

/// A definition's head as a declaration's: no metadata attachments
/// (`!dbg !12`, `!guid !3`), no personality and no comdat.
fn declaration_head(head: &str) -> String {
    let words: Vec<&str> = head.split(' ').collect();
    let mut kept = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if w.starts_with('!') && !w.starts_with("!{") && i + 1 < words.len() && words[i + 1].starts_with('!') {
            i += 2;
        } else if w == "personality" && i + 2 < words.len() {
            i += 3;
        } else if w == "comdat" || w.starts_with("comdat(") {
            i += 1;
        } else {
            kept.push(w);
            i += 1;
        }
    }
    kept.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEF: &str = "define internal fastcc void @_RINvMs0_NtNtNtCs8_3std12backtrace_rs9symbolize5gimliNtB6_5Cache11with_globalNCNvB6_7resolves_0EBc_(ptr %0, ptr %1) unnamed_addr #4 personality ptr @rust_eh_personality !dbg !93 !guid !6567 {";

    #[test]
    fn a_definition_becomes_a_hidden_declaration_without_attachments() {
        let text = format!("x\n{DEF}\nstart:\n  ret void\n}}\ny\n");
        let name = define_name(DEF).unwrap();
        let out = declare_instead(&text, name).unwrap();
        assert_eq!(
            out,
            "x\ndeclare hidden fastcc void @_RINvMs0_NtNtNtCs8_3std12backtrace_rs9symbolize5gimliNtB6_5Cache11with_globalNCNvB6_7resolves_0EBc_(ptr %0, ptr %1) unnamed_addr #4\ny\n"
        );
    }

    #[test]
    fn the_cut_is_found_by_its_path_segments_and_must_be_unique() {
        let text = format!("{DEF}\n}}\n");
        assert!(find_define(&text, &CUTS[1]).unwrap().is_some());
        assert!(find_define(&text, &CUTS[0]).unwrap().is_none());
        let twice = format!("{DEF}\n}}\n{}\n}}\n", DEF.replace("s_0EBc_", "s_1EBc_"));
        assert!(find_define(&twice, &CUTS[1]).is_err());
    }

    const TLS: &str = "@_RNvNCNKNvNtNtCs8_3std9panicking11panic_count17LOCAL_PANIC_COUNT0s_023___RUST_STD_INTERNAL_VAL = internal thread_local unnamed_addr global <{ [9 x i8], [7 x i8] }> <{ [9 x i8] zeroinitializer, [7 x i8] undef }>, align 8, !guid !34";

    #[test]
    fn the_panic_count_is_kept_a_symbol_in_std_and_referenced_from_the_object() {
        let text = format!("{TLS}\n@llvm.used = appending global [1 x ptr] [ptr @a], section \"llvm.metadata\", !guid !9\n");
        let name = find_global(&text, SHARED_TLS).unwrap().unwrap();
        let kept = keep_global(&text, &name).unwrap();
        assert!(kept.contains(&format!("{name} = hidden thread_local unnamed_addr global")), "{kept}");
        assert!(kept.contains(&format!("@llvm.used = appending global [2 x ptr] [ptr @{name}, ptr @a]")), "{kept}");
        assert_eq!(
            external_global(TLS).unwrap(),
            format!("@{name} = external hidden thread_local global <{{ [9 x i8], [7 x i8] }}>, align 8")
        );
    }

    #[test]
    fn linkage_words_are_stripped_from_the_head() {
        assert_eq!(strip_linkage("define internal hidden fastcc void @f() {"), "fastcc void @f() {");
        assert_eq!(strip_linkage("define noundef zeroext i1 @g(ptr %0) {"), "noundef zeroext i1 @g(ptr %0) {");
    }

    #[test]
    fn panic_text_normalizes_thread_ids_and_addresses() {
        let s = "thread 'main' (9417) panicked at pan.rs:1:57:\n   0:     0x5958bbc92459 - main";
        assert_eq!(normalize_panic_text(s), "thread 'main' (TID) panicked at pan.rs:1:57:\n   0:     0xADDR - main");
    }
}
