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
//!   bitcode (every rlib it does not rewrite is a symlink, and the rewritten
//!   ones keep their machine code and metadata), so the LTO module no longer
//!   reaches the printer, and
//! * one object, `backtrace.o`, holding the same two functions compiled from
//!   the same std bitcode with everything they reach as private copies —
//!   except the per-thread panic count, which it references in the program's
//!   std ([`SHARED_TLS`]), so the thread-local block is today's byte for byte.
//!   Its definitions are weak: in a link without LTO (a build script of a
//!   cargo dependency) std's own definition wins.
//! * no second copy of a std function both sides link (#4497). The printer
//!   reaches std functions programs link too — `fmt::write`, `str` search,
//!   UTF-8 checks, `RawVec` growth, path walking, the panic path — and
//!   private copies of those were linked twice, once from the LTO module and
//!   once from the object (20-32 KB per binary). The split finds them by
//!   linking a seed program against the split (at opt-level 0, so nothing is
//!   inlined away) and listing the names defined twice, then keeps one copy:
//!   - a function that holds no state of the object's own is MOVED: the
//!     object defines it (weak, hidden, at speed rather than `optsize`) and
//!     the split crates' bitcode carries it `available_externally`, so the
//!     LTO module still inlines it where it likes but never emits a copy —
//!     a program that does not call it pays nothing, and the LTO module
//!     code-generates less than before;
//!   - a function that reaches a static, a thread-local or an
//!     address-compared constant the object would own a private copy of (the
//!     panic machinery, the hook lock, panic_unwind's canary) is REFERENCED:
//!     the program defines it (`@llvm.used`) and the object calls the
//!     program's, so there is one copy of the state.
//!   A name with two definitions in the crates' bitcode, or one an alias or
//!   `@llvm.used` pins, keeps its private copy.
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
//! `llvm-dis`, `llvm-as`, `llvm-objcopy`, `llvm-ar`, `llvm-nm`). Without it, on any
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

/// The prefix of the attribute-group ids that copy a group without `optsize`.
const FAST_GROUP: u32 = 9000;

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

/// The split for this process's toolchain and the build's `--target-cpu`
/// ([`super::native_target::release_target_cpu`]), built on first use and
/// cached on disk; `None` means "link with the plain profile".
pub(super) fn release_std_split() -> Option<&'static StdSplit> {
    type Splits = std::collections::HashMap<Option<String>, Option<&'static StdSplit>>;
    static SPLITS: std::sync::OnceLock<std::sync::Mutex<Splits>> = std::sync::OnceLock::new();
    if !cfg!(target_os = "linux")
        || almide_base::env::flag("ALMIDE_RELEASE_STD_UNSPLIT")
        || super::native_target::cross_target().is_some()
    {
        return None;
    }
    let cpu = super::native_target::release_target_cpu();
    let mut splits = SPLITS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    *splits.entry(cpu.clone()).or_insert_with(|| match ensure_split(cpu.as_deref()) {
        Ok(split) => split.map(|s| &*Box::leak(Box::new(s))),
        Err(e) => {
            if almide_base::env::flag("ALMIDE_DEBUG_STD_SPLIT") {
                eprintln!("almide: release std split unavailable: {e}");
            }
            None
        }
    })
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

fn ensure_split(cpu: Option<&str>) -> Result<Option<StdSplit>, String> {
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
    // The object is compiled for the build's CPU: the std functions it
    // shares with the program are the program's code (#4497), and were
    // compiled for that CPU in its LTO module before.
    let label = cpu.map_or_else(|| "generic".to_string(), |c| c.chars().map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' }).collect());
    let obj_dir = dir.join(format!("cpu-{label}"));
    let ready = obj_dir.join("READY");
    let split = || StdSplit { sysroot: dir.join("sysroot"), object: obj_dir.join("backtrace.o") };
    let use_ready = |s: StdSplit| {
        super::build_dir::touch_used(&dir.join("READY"));
        Ok(Some(s))
    };
    let failed = |d: &Path| d.join("FAILED").exists();
    if ready.exists() {
        return use_ready(split());
    }
    if failed(&dir) || failed(&obj_dir) {
        return Ok(None);
    }
    std::fs::create_dir_all(&obj_dir).map_err(|e| format!("{}: {e}", obj_dir.display()))?;
    let _lock = super::build_dir::BuildDirLock::acquire(&dir)?;
    if ready.exists() {
        return use_ready(split());
    }
    if failed(&dir) || failed(&obj_dir) {
        return Ok(None);
    }
    // A failure is a property of this toolchain (a std whose printer moved)
    // or of this CPU (a probe this host cannot run, or that printed
    // differently): recorded, so every later build takes the plain profile
    // without retrying.
    let record = |d: &Path, e: String| {
        let _ = std::fs::write(d.join("FAILED"), e.as_bytes());
        if !almide_base::env::flag("ALMIDE_DEBUG_STD_SPLIT") {
            let _ = std::fs::remove_dir_all(d.join("work"));
        }
        Err(e)
    };
    let info = match SplitInfo::read(&dir) {
        Some(info) => info,
        None => match build_sysroot_split(&tc, &std_rlib, &dir) {
            Ok(info) => {
                info.write(&dir)?;
                let _ = std::fs::remove_dir_all(dir.join("work"));
                info
            }
            Err(e) => return record(&dir, e),
        },
    };
    match build_cpu_object(&tc, &dir, &obj_dir, &info, cpu) {
        Ok(()) => {
            std::fs::write(&ready, b"").map_err(|e| format!("{}: {e}", ready.display()))?;
            std::fs::write(dir.join("READY"), b"").map_err(|e| format!("{}: {e}", dir.display()))?;
            let _ = std::fs::remove_dir_all(obj_dir.join("work"));
            Ok(Some(split()))
        }
        Err(e) => record(&obj_dir, e),
    }
}

/// What the toolchain phase decided, kept next to the sysroot so an object
/// for another CPU is built against the same split: the cut functions, the
/// shared thread-local, and the shared std functions.
struct SplitInfo {
    cuts: Vec<String>,
    tls: String,
    shared: Shared,
}

impl SplitInfo {
    const FILE: &'static str = "SPLIT";
    fn write(&self, dir: &Path) -> Result<(), String> {
        let mut text = String::new();
        for c in &self.cuts {
            text.push_str(&format!("cut {c}\n"));
        }
        text.push_str(&format!("tls {}\n", self.tls));
        let mut moved: Vec<&String> = self.shared.moved.iter().collect();
        moved.sort();
        for m in moved {
            text.push_str(&format!("moved {m}\n"));
        }
        let mut referenced: Vec<&String> = self.shared.referenced.iter().collect();
        referenced.sort();
        for r in referenced {
            text.push_str(&format!("referenced {r}\n"));
        }
        std::fs::write(dir.join(Self::FILE), text).map_err(|e| format!("{}: {e}", dir.display()))
    }
    fn read(dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(dir.join(Self::FILE)).ok()?;
        let mut info = SplitInfo { cuts: Vec::new(), tls: String::new(), shared: Shared::default() };
        for line in text.lines() {
            match line.split_once(' ')? {
                ("cut", n) => info.cuts.push(n.to_string()),
                ("tls", n) => info.tls = n.to_string(),
                ("moved", n) => {
                    info.shared.moved.insert(n.to_string());
                }
                ("referenced", n) => {
                    info.shared.referenced.insert(n.to_string());
                }
                _ => return None,
            }
        }
        (info.cuts.len() == CUTS.len() && !info.tls.is_empty()).then_some(info)
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

/// The toolchain phase: the split sysroot, and the decision which std
/// functions the object shares with the program (#4497), made against an
/// object for the generic CPU.
fn build_sysroot_split(tc: &Toolchain, std_rlib: &Path, dir: &Path) -> Result<SplitInfo, String> {
    let work = dir.join("work");
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(dir.join("sysroot"));
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let (cuts, tls, std_unpacked) = build_sysroot(tc, std_rlib, dir, &work)?;
    let merged = seed_lto_input(tc, &work, None)?;
    let object = work.join("backtrace.o");
    build_object(tc, &work, &merged, &cuts, &tls, &Shared::default(), &object)?;
    // The printer's private copies of std functions the program links too
    // (fmt, the panic path, str search…) would be linked twice: once from the
    // LTO module, once from the object. Find them by linking the seed against
    // the split; the seed links at opt-level 0 so that nothing is inlined
    // away and every std function its code reaches is a definition to match.
    let twice = defined_twice(tc, &dir.join("sysroot"), &object, &work, 0)?;
    let optimized = work.join("backtrace.ll");
    run(std::process::Command::new(tc.tool("llvm-dis")).arg(work.join("backtrace.bc")).arg("-o").arg(&optimized))?;
    let ir = std::fs::read_to_string(&optimized).map_err(|e| format!("{}: {e}", optimized.display()))?;
    let stateful = stateful_functions(&ir, &twice);
    let shared = share_with_program(tc, dir, &work, std_unpacked, &twice, &stateful)?;
    if almide_base::env::flag("ALMIDE_DEBUG_STD_SPLIT") {
        eprintln!(
            "almide: std split: {} functions defined twice, {} moved to the object, {} kept in the program",
            twice.len(),
            shared.moved.len(),
            shared.referenced.len()
        );
    }
    Ok(SplitInfo { cuts, tls, shared })
}

/// The CPU phase: `backtrace.o` for `cpu`, and the probe that gates it.
fn build_cpu_object(tc: &Toolchain, dir: &Path, obj_dir: &Path, info: &SplitInfo, cpu: Option<&str>) -> Result<(), String> {
    let work = obj_dir.join("work");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let merged = seed_lto_input(tc, &work, cpu)?;
    let object = obj_dir.join("backtrace.o");
    build_object(tc, &work, &merged, &info.cuts, &info.tls, &info.shared, &object)?;
    let split = StdSplit { sysroot: dir.join("sysroot"), object };
    if almide_base::env::flag("ALMIDE_DEBUG_STD_SPLIT") {
        let left = defined_twice(tc, &split.sysroot, &split.object, &work, 3)?;
        eprintln!("almide: std split ({}): {} functions still defined twice: {left:?}", cpu.unwrap_or("generic"), left.len());
    }
    probe(&split, &work, cpu)
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
fn defined_twice(
    tc: &Toolchain,
    sysroot: &Path,
    object: &Path,
    work: &Path,
    opt_level: u8,
) -> Result<std::collections::BTreeSet<String>, String> {
    let split = StdSplit { sysroot: sysroot.to_path_buf(), object: object.to_path_buf() };
    let bin = work.join("seed").join(format!("seed-split-O{opt_level}"));
    run(std::process::Command::new(crate::find_rustc())
        .arg(work.join("seed.rs"))
        .args(["--edition", "2021", "-C", &format!("opt-level={opt_level}"), "-C", "lto=fat", "-C", "codegen-units=1", "-C", "strip=debuginfo"])
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
    stateful: &std::collections::HashSet<String>,
) -> Result<Shared, String> {
    if wanted.is_empty() {
        return Ok(Shared::default());
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
    // A name another global points at (an alias, a `@llvm.used` entry) needs
    // a real definition in its module, so it keeps its copy.
    let pinned: std::collections::HashSet<&str> = texts
        .values()
        .flat_map(|t| t.lines())
        .filter(|l| l.starts_with("@llvm.used = ") || l.starts_with("@llvm.compiler.used = ") || (l.starts_with('@') && l.contains(" alias ")))
        .flat_map(|l| l.split("ptr @").skip(1).map(|r| r.trim_start_matches('"').split(['"', ',', ']', ' ', ')']).next().unwrap_or("")))
        .collect();
    let unique = sites.iter().filter(|(_, s)| s.len() == 1).map(|(n, _)| n.to_string());
    let mut shared = Shared::default();
    for name in unique {
        if stateful.contains(&name) {
            shared.referenced.insert(name);
        } else if !pinned.contains(name.as_str()) {
            shared.moved.insert(name);
        }
    }
    let mut changed: std::collections::BTreeSet<usize> = Default::default();
    for ((i, member), text) in texts {
        let patched = export_defines(&available_defines(&text, &shared.moved), &shared.referenced)?;
        if patched != text {
            rlibs[i].1.replace(tc, &member, &patched)?;
            changed.insert(i);
        }
    }
    for i in changed {
        let (out, u) = &rlibs[i];
        u.pack(tc, out)?;
    }
    Ok(shared)
}

/// The std functions `backtrace.o` shares with the program instead of
/// carrying a private copy of each (#4497).
#[derive(Default)]
struct Shared {
    /// Stateless ones: the object defines them (weak, hidden), the split
    /// std's bitcode carries them `available_externally` — the program
    /// inlines what it likes and calls the object's copy for the rest.
    moved: std::collections::HashSet<String>,
    /// Ones that reach a static or thread-local the object would otherwise
    /// own a private copy of (the panic machinery, the hook lock, the current
    /// thread): the program defines them (kept with `@llvm.used`) and the
    /// object references the program's, so there is one copy of the state.
    referenced: std::collections::HashSet<String>,
}

/// The functions of `candidates` that reach, in `ir` (an optimized module's
/// text), state the module defines — a mutable `global`, a thread-local, or
/// a constant whose address is compared — through calls or referenced constants that pass only
/// through functions outside `candidates`. External globals (libc's, the
/// shared panic count) are one object either way. A candidate reached on the
/// way is where the search stops: if it is stateful it is referenced in the
/// program, so what lies behind it is the program's state, not the object's.
fn stateful_functions(ir: &str, candidates: &std::collections::BTreeSet<String>) -> std::collections::HashSet<String> {
    let mut refs: std::collections::HashMap<String, Vec<String>> = Default::default();
    let mut state: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in ir.lines() {
        if let Some(f) = current.as_ref() {
            if line.starts_with('}') {
                current = None;
            } else {
                let f = f.clone();
                refs.entry(f).or_default().extend(ir_refs(line));
            }
            continue;
        }
        if let Some(name) = define_name(line) {
            let name = name.to_string();
            refs.entry(name.clone()).or_default().extend(ir_refs(line));
            if line.trim_end().ends_with('{') {
                current = Some(name);
            }
            continue;
        }
        let Some((head, tail)) = line.split_once(" = ").filter(|_| line.starts_with('@')) else { continue };
        let name = head[1..].trim_matches('"').to_string();
        let words: Vec<&str> = tail.split(' ').take_while(|w| !w.starts_with('[') && !w.starts_with('{') && !w.starts_with('<')).collect();
        if words.contains(&"external") || words.contains(&"extern_weak") {
            continue;
        }
        // A constant whose address is significant (no `unnamed_addr`) is
        // identity, not data: panic_unwind's exception canary is compared by
        // address, and a second copy reads as a foreign exception.
        let identity = !words.contains(&"unnamed_addr") && !words.contains(&"local_unnamed_addr");
        if words.contains(&"global") || (words.contains(&"constant") && identity) {
            state.push(name.clone());
        }
        refs.entry(name).or_default().extend(ir_refs(tail));
    }
    let mut callers: std::collections::HashMap<&str, Vec<&str>> = Default::default();
    for (from, to) in &refs {
        for t in to {
            callers.entry(t.as_str()).or_default().push(from.as_str());
        }
    }
    let mut seen: std::collections::HashSet<&str> = state.iter().map(String::as_str).collect();
    let mut queue: Vec<&str> = seen.iter().copied().collect();
    let mut found = std::collections::HashSet::new();
    while let Some(n) = queue.pop() {
        for c in callers.get(n).into_iter().flatten() {
            if candidates.contains(*c) {
                found.insert(c.to_string());
            } else if seen.insert(c) {
                queue.push(c);
            }
        }
    }
    found
}

/// The `@names` a line of IR mentions.
fn ir_refs(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(i) = rest.find('@') {
        rest = &rest[i + 1..];
        let name = if let Some(q) = rest.strip_prefix('"') {
            q.split('"').next().unwrap_or("")
        } else {
            let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$' | '-'))).unwrap_or(rest.len());
            &rest[..end]
        };
        if !name.is_empty() && !name.starts_with("llvm.") {
            out.push(name.to_string());
        }
    }
    out
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
    let kept: Vec<String> = text.lines().filter_map(define_name).filter(|n| names.contains(*n)).map(ir_ref).collect();
    if kept.is_empty() {
        return Ok(text.to_string());
    }
    let extra = kept.iter().map(|r| format!("ptr {r}")).collect::<Vec<_>>().join(", ");
    let mut out = String::with_capacity(text.len() + extra.len() + 128);
    let mut listed = false;
    for line in text.lines() {
        if define_name(line).is_some_and(|n| names.contains(n)) {
            out.push_str(&hidden_define(line));
        } else if line.starts_with("@llvm.used = ") {
            out.push_str(&append_used(line, kept.len(), &extra)?);
            listed = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !listed {
        out.push_str(&format!("@llvm.used = appending global [{} x ptr] [{extra}], section \"llvm.metadata\"\n", kept.len()));
    }
    Ok(out)
}

/// A definition as an external hidden one: module-local linkage
/// (`internal`, `private`) dropped, any other linkage (`linkonce_odr`, …)
/// kept, and the visibility hidden. Hidden makes it `dso_local`, so the
/// program's calls to it stay direct rather than going through the GOT.
fn hidden_define(line: &str) -> String {
    let mut rest = line.strip_prefix("define ").unwrap_or(line);
    let mut linkage = Vec::new();
    loop {
        let word = rest.split(' ').next().unwrap_or("");
        match word {
            "internal" | "private" | "external" | "hidden" | "protected" | "default" | "dso_local" | "dso_preemptable" => {}
            "weak" | "weak_odr" | "linkonce" | "linkonce_odr" => linkage.push(word),
            _ => break,
        }
        rest = rest[word.len()..].trim_start();
    }
    linkage.push("hidden");
    format!("define {} {rest}", linkage.join(" "))
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
/// Turn every definition of a name in `names` into an `available_externally`
/// one: the LTO module may still inline the body, but never emits a copy of
/// its own — what is left calls the one `backtrace.o` defines. Hidden, so
/// those calls are direct; no comdat, which `available_externally` cannot
/// carry. (Not `@llvm.used` on a symbol the LTO module keeps: that holds a
/// copy in every program whether it calls it or not, and costs each build
/// the time to optimize and code-generate it.)
fn available_defines(text: &str, names: &std::collections::HashSet<String>) -> String {
    let mut out = String::with_capacity(text.len() + 4096);
    for line in text.lines() {
        if define_name(line).is_some_and(|n| names.contains(n)) {
            out.push_str("define available_externally hidden ");
            let rest: Vec<&str> =
                strip_linkage(line).split(' ').filter(|w| *w != "comdat" && !w.starts_with("comdat(")).collect();
            out.push_str(&rest.join(" "));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}


#[cfg(unix)]
fn symlink(from: &Path, to: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(from, to).map_err(|e| format!("symlink {}: {e}", to.display()))
}

#[cfg(not(unix))]
fn symlink(_from: &Path, to: &Path) -> Result<(), String> {
    Err(format!("no symlinks on this host ({})", to.display()))
}

/// The seed program: its LTO input holds the printer's code, and what its
/// LTO module keeps decides which std functions the object shares with the
/// program instead of copying (#4497). It touches what the printer reaches
/// and almide programs commonly link too — formatting (`{:#?}` of strings),
/// UTF-8 checks, file reads and metadata, path walking, `io::Error`, sorting,
/// division — so those land once, in the LTO module. A function shared here
/// that a program would not otherwise link is kept there for the printer: it
/// moves out of the object rather than being added.
const SEED: &str = r#"use std::io::Read;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n = args.len();
    let mut bytes = Vec::new();
    if let Ok(mut f) = std::fs::File::open(&args[0]) {
        let _ = f.read_to_end(&mut bytes);
    }
    let len = std::fs::metadata(&args[0]).map(|m| m.len()).unwrap_or(0);
    let p = std::path::Path::new(&args[0]);
    let rel = p.strip_prefix("/").map(|r| r.components().count()).unwrap_or(0);
    let root = p.components().next_back() == Some(std::path::Component::RootDir);
    let canon = std::fs::canonicalize(p).map(|c| c.as_os_str().len()).unwrap_or(0);
    let lossy = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]).chars().count();
    let strict = std::str::from_utf8(&bytes[..n.min(bytes.len())]).is_ok();
    let mut sorted: Vec<&String> = args.iter().collect();
    sorted.sort();
    let err = std::io::Error::new(std::io::ErrorKind::Other, "seed");
    println!("{n} {len} {rel} {root} {canon} {lossy} {strict} {sorted:#?} {err} {}", 100 / n);
}
"#;

/// `-C target-cpu=<cpu>`, when the build names one.
fn cpu_args(cpu: Option<&str>) -> Vec<String> {
    cpu.map(|c| vec!["-C".to_string(), format!("target-cpu={c}")]).unwrap_or_default()
}

/// The merged LTO input of a seed build with the plain sysroot (std, its
/// dependencies and rustc's allocator shim) for `cpu`, as IR text: its
/// function attributes carry that CPU, so the object is compiled for it.
fn seed_lto_input(tc: &Toolchain, work: &Path, cpu: Option<&str>) -> Result<String, String> {
    let src = work.join("seed.rs");
    std::fs::write(&src, SEED).map_err(|e| format!("{e}"))?;
    let seed_dir = work.join("seed");
    std::fs::create_dir_all(&seed_dir).map_err(|e| format!("{}: {e}", seed_dir.display()))?;
    run(std::process::Command::new(crate::find_rustc())
        .arg(&src)
        .args(["--edition", "2021", "-C", "opt-level=3", "-C", "lto=fat", "-C", "codegen-units=1", "-C", "save-temps"])
        .args(cpu_args(cpu))
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
    with: &Shared,
    out: &Path,
) -> Result<(), String> {
    let ll = work.join("merged.ll");
    let mut kept = String::with_capacity(text.len());
    let mut exported = vec![false; names.len()];
    let mut referenced = false;
    // The function attribute groups that get `optsize` (the printer runs once
    // per panic); a moved function is the program's code now and keeps its
    // speed, through a copy of its group without it.
    let sized: std::collections::HashSet<&str> = text
        .lines()
        .filter(|l| l.starts_with("attributes #") && l.contains("\"target-cpu\""))
        .filter_map(|l| l["attributes #".len()..].split(' ').next())
        .collect();
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
        } else if define_name(line).is_some_and(|n| with.moved.contains(n)) {
            let head = format!("define weak hidden {}", strip_linkage(line));
            let words: Vec<String> = head
                .split(' ')
                .map(|w| match w.strip_prefix('#') {
                    Some(id) if sized.contains(id) => format!("#{FAST_GROUP}{id}"),
                    _ => w.to_string(),
                })
                .collect();
            kept.push_str(&words.join(" "));
        } else if define_name(line).is_some_and(|n| with.referenced.contains(n)) {
            let head = strip_linkage(line).trim_end();
            let head = head.strip_suffix('{').unwrap_or(head).trim_end();
            kept.push_str("declare hidden ");
            kept.push_str(&declaration_head(head));
            for body in lines.by_ref() {
                if body.starts_with('}') {
                    break;
                }
            }
        } else if let Some(id) = line.strip_prefix("attributes #").and_then(|r| r.split(' ').next()).filter(|id| sized.contains(id)) {
            kept.push_str(&format!("attributes #{FAST_GROUP}{id}{}\n", &line["attributes #".len() + id.len()..]));
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
    // O3 and the release LLVM options: the moved std functions are the
    // program's code and were optimized that way in its LTO module; the
    // printer's own functions carry `optsize` whatever the level.
    run(std::process::Command::new(tc.tool("opt"))
        .arg("-passes=internalize,globaldce,default<O3>")
        .args(super::native_target::RELEASE_LLVM_ARGS)
        .arg(format!(
            "-internalize-public-api-list={}",
            names.iter().chain(with.moved.iter()).map(String::as_str).collect::<Vec<_>>().join(",")
        ))
        .arg(&bc)
        .arg("-o")
        .arg(&opt_bc))?;
    run(std::process::Command::new(tc.tool("llc"))
        .args(["-O3", "-filetype=obj", "-relocation-model=pic", "-function-sections", "-data-sections"])
        .arg(&opt_bc)
        .arg("-o")
        .arg(out))?;
    Ok(())
}

const PROBE: &str = r#"#[inline(never)] fn inner(v: &[i64], i: usize) -> i64 { v[i] }
#[inline(never)] fn outer(n: usize) -> i64 { let v = vec![1, 2, 3]; inner(&v, n + 7) }
fn main() {
    println!("start");
    let n = std::env::args().count();
    let worker = std::thread::Builder::new().name("worker".into()).spawn(move || {
        let items: Vec<String> = (0..n).map(|i| format!("{i}")).collect();
        items.get(9).expect("no tenth item").len()
    });
    println!("joined: {}", worker.map(|h| h.join().is_err()).unwrap_or(false));
    println!("caught: {}", std::panic::catch_unwind(|| outer(n)).is_err());
    println!("{}", outer(n));
}
"#;

/// Build a panicking probe with the plain sysroot and with the split one,
/// and require the same panic output from both under every `RUST_BACKTRACE`
/// setting. Thread ids and addresses differ run to run and are normalized.
fn probe(split: &StdSplit, work: &Path, cpu: Option<&str>) -> Result<(), String> {
    let src = work.join("probe.rs");
    // A panic on a named thread (the thread's name is state the printer
    // reads), one caught by `catch_unwind` (panic_unwind's canary decides
    // whether the exception is Rust's), and one that ends the process.
    std::fs::write(&src, PROBE).map_err(|e| format!("{e}"))?;
    let build = |out: &Path, extra: &[String]| {
        run(std::process::Command::new(crate::find_rustc())
            .arg(&src)
            // The object's CPU: the probe runs here, so a CPU this host
            // cannot run fails it and that CPU links with the plain profile.
            .args(["--edition", "2021", "-C", "opt-level=3", "-C", "lto=fat", "-C", "codegen-units=1", "-C", "strip=debuginfo"])
            .args(cpu_args(cpu))
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
    fn a_shared_function_becomes_available_externally_without_its_comdat() {
        let names: std::collections::HashSet<String> = ["f", "g"].iter().map(|s| s.to_string()).collect();
        let text = "define internal fastcc void @f() unnamed_addr #1 {\n  ret void\n}\ndefine linkonce_odr hidden void @g() unnamed_addr comdat #2 {\n  ret void\n}\ndefine void @h() {\n  ret void\n}\n";
        let out = available_defines(text, &names);
        assert!(out.contains("define available_externally hidden fastcc void @f() unnamed_addr #1 {"), "{out}");
        assert!(out.contains("define available_externally hidden void @g() unnamed_addr #2 {"), "{out}");
        assert!(out.contains("define void @h() {"), "{out}");
        assert_eq!(available_defines("define void @h() {\n}\n", &names), "define void @h() {\n}\n");
    }

    #[test]
    fn a_shared_function_becomes_a_kept_hidden_symbol() {
        let names: std::collections::HashSet<String> = ["f", "g", "a$b"].iter().map(|s| s.to_string()).collect();
        let text = "define internal fastcc void @f() unnamed_addr #1 {\n  ret void\n}\ndefine void @g() {\n  ret void\n}\ndefine private void @\"a$b\"() {\n  ret void\n}\ndefine void @h() {\n  ret void\n}\n";
        let out = export_defines(text, &names).unwrap();
        assert!(out.contains("define hidden fastcc void @f() unnamed_addr #1 {"), "{out}");
        assert!(out.contains("define hidden void @g() {"), "{out}");
        assert!(out.contains("define hidden void @\"a$b\"() {"), "{out}");
        assert!(out.contains("define void @h() {"), "{out}");
        assert_eq!(hidden_define("define linkonce_odr dso_local void @k() comdat {"), "define linkonce_odr hidden void @k() comdat {");
        assert!(out.ends_with("@llvm.used = appending global [3 x ptr] [ptr @f, ptr @g, ptr @\"a$b\"], section \"llvm.metadata\"\n"), "{out}");
        let listed = format!("{text}@llvm.used = appending global [1 x ptr] [ptr @x], section \"llvm.metadata\"\n");
        let out = export_defines(&listed, &names).unwrap();
        assert!(out.contains("@llvm.used = appending global [4 x ptr] [ptr @f, ptr @g, ptr @\"a$b\", ptr @x], section"), "{out}");
        assert_eq!(out.matches("@llvm.used").count(), 1, "{out}");
        assert_eq!(export_defines("define void @h() {\n}\n", &names).unwrap(), "define void @h() {\n}\n");
    }

    #[test]
    fn state_is_found_through_private_callees_and_stops_at_a_candidate() {
        let ir = "@state = internal global i32 0, align 4\n\
                  @text = private unnamed_addr constant [1 x i8] c\"a\", align 1\n\
                  @canary = internal constant i8 0, align 1\n\
                  @ext = external global i32\n\
                  define internal void @f() {\n  store i32 1, ptr @state, align 4\n  ret void\n}\n\
                  define internal void @g() {\n  call void @f()\n  ret void\n}\n\
                  define internal void @c() {\n  call void @g()\n  ret void\n}\n\
                  define internal void @d() {\n  call void @c()\n  ret void\n}\n\
                  define internal ptr @e() {\n  ret ptr @canary\n}\n\
                  define internal ptr @h() {\n  %x = load i32, ptr @ext, align 4\n  ret ptr @text\n}\n";
        let candidates: std::collections::BTreeSet<String> = ["c", "d", "e", "h"].iter().map(|s| s.to_string()).collect();
        let mut found: Vec<String> = stateful_functions(ir, &candidates).into_iter().collect();
        found.sort();
        // c reaches @state through private g and f; e returns an address-compared
        // constant; d reaches state only through candidate c; h reads only an
        // external global and an unnamed_addr constant.
        assert_eq!(found, ["c", "e"]);
        assert_eq!(ir_refs("call void @f(ptr @\"a b\", ptr @llvm.memcpy)"), ["f", "a b"]);
    }

    #[test]
    fn nm_listing_yields_function_definitions_only() {
        let listing = "0000000000001000 T main\n0000000000001010 t _RNvf\n0000000000002000 D data\n0000000000001020 W weakf\n0000000000003000 b bss\n";
        assert_eq!(nm_functions(listing), ["main", "_RNvf", "weakf"]);
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
