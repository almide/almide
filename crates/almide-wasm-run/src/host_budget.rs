//! One linear-memory budget per guest (#4135).
//!
//! A run is more than one wasmtime store: the run's own instance, the
//! instance-parallel fan's worker instances (`host_fan.rs`, up to the
//! available parallelism) and a `testing.assert_aborts` site's child
//! instance (op 117). Each store used to carry its own
//! `StoreLimits::memory_size(cap)`, so N workers could together hold N × the
//! cap the run was given. Every store of a run now meters its linear-memory
//! growth against ONE shared [`Budget`]: a growth is granted only when the
//! sum over all of the run's live stores stays within the cap.
//!
//! What a refusal means depends on who asked:
//! - a fan worker's refused growth fails its chunk, and a failed offer runs
//!   sequentially in the run's own instance — unobservable, the fan's own
//!   fallback rule;
//! - the run's own instance first takes back the memory its idle pooled fan
//!   workers hold (only pooled workers exist while it runs: an offer runs
//!   inside a host call, with the guest stopped), so a single instance can
//!   still reach the whole cap exactly as before;
//! - a refusal that remains is the guest's `memory.grow` answering -1, which
//!   the emitted allocator turns into C-197's `Error: out of memory`, exit 1.
//!
//! A run that names no cap (the product runner, `almide run --target wasm`)
//! gets [`default_cap`]: the least of the wasm32 address space (4 GiB), the
//! memory limit of the cgroup the host runs in, and the machine's physical
//! memory — read from the environment, never a chosen number (owner ruling
//! 2026-10-26, C-197). In a container or on a machine smaller than 4 GiB the
//! guest's `memory.grow` is refused at the limit the environment sets, and the
//! run ends in C-197's `Error: out of memory` instead of being killed by the
//! kernel. A cap the caller names overrides it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The wasm32 address space: the most one instance's memory can reach, and
/// the budget of a run that names none when the environment says nothing
/// smaller.
pub(crate) const WASM32_SPACE: usize = 1 << 32;

/// The budget of a run that names none — THE one place it is decided: the
/// least of [`WASM32_SPACE`], the host's cgroup memory limit and its physical
/// memory, read once per process.
pub(crate) fn default_cap() -> usize {
    static CAP: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CAP.get_or_init(|| cap_from(&Env::system()))
}

/// Where the limits are read from: the real `/` in production; a test points
/// `root` at a directory laid out like it (`proc/self/cgroup`,
/// `proc/meminfo`, `sys/fs/cgroup/...`).
pub(crate) struct Env {
    root: PathBuf,
    /// Ask the OS (macOS `hw.memsize`) when `proc/meminfo` is absent.
    ask_os: bool,
}

impl Env {
    fn system() -> Self {
        Env { root: PathBuf::from("/"), ask_os: true }
    }

    fn read(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(rel)).ok()
    }
}

/// The least of 4 GiB and every limit `env` reveals; 4 GiB when it reveals
/// none.
pub(crate) fn cap_from(env: &Env) -> usize {
    [cgroup_limit(env), physical_memory(env)].into_iter().flatten().fold(WASM32_SPACE, usize::min)
}

/// The tightest memory limit along this process's cgroup path: cgroup v2
/// `memory.max` (`max` = none), else v1 `memory.limit_in_bytes` (its
/// "unlimited" is a huge number, which the 4 GiB bound absorbs).
fn cgroup_limit(env: &Env) -> Option<usize> {
    let table = env.read("proc/self/cgroup")?;
    let v2 = table.lines().find_map(|l| l.strip_prefix("0::"));
    if let Some(path) = v2
        && let Some(n) = tightest(env, "sys/fs/cgroup", path, "memory.max")
    {
        return Some(n);
    }
    let v1 = table.lines().find_map(|l| {
        let mut parts = l.splitn(3, ':');
        let (_, controllers, path) = (parts.next()?, parts.next()?, parts.next()?);
        controllers.split(',').any(|c| c == "memory").then_some(path)
    })?;
    tightest(env, "sys/fs/cgroup/memory", v1, "memory.limit_in_bytes")
}

/// The least `file` value from the cgroup at `path` up to the hierarchy's
/// root (a parent's limit binds its children).
fn tightest(env: &Env, mount: &str, path: &str, file: &str) -> Option<usize> {
    let mut dir = Path::new(mount).join(path.trim_start_matches('/'));
    let mut least: Option<usize> = None;
    loop {
        let value = env.read(&dir.join(file).to_string_lossy()).and_then(|v| v.trim().parse::<u64>().ok());
        if let Some(v) = value {
            let v = usize::try_from(v).unwrap_or(usize::MAX);
            least = Some(least.map_or(v, |l| l.min(v)));
        }
        if dir == Path::new(mount) || !dir.pop() {
            return least;
        }
    }
}

/// Physical memory: Linux `MemTotal` (kB), else macOS `hw.memsize` through
/// `/usr/sbin/sysctl` (an absolute path, never PATH; the workspace forbids
/// the `unsafe` a direct sysctl(3) call needs).
fn physical_memory(env: &Env) -> Option<usize> {
    if let Some(info) = env.read("proc/meminfo") {
        let kb = info.lines().find_map(|l| l.strip_prefix("MemTotal:"))?.trim().trim_end_matches("kB").trim().parse::<u64>().ok()?;
        return usize::try_from(kb.saturating_mul(1024)).ok();
    }
    if env.ask_os && cfg!(target_os = "macos") {
        let out = std::process::Command::new("/usr/sbin/sysctl").args(["-n", "hw.memsize"]).output().ok()?;
        return String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().ok().and_then(|n| usize::try_from(n).ok());
    }
    None
}

/// A run's linear-memory budget, shared by every store of the run.
pub(crate) struct Budget {
    cap: usize,
    used: AtomicUsize,
}

impl Budget {
    pub(crate) fn new(cap: Option<usize>) -> Arc<Self> {
        Arc::new(Budget { cap: cap.unwrap_or_else(default_cap), used: AtomicUsize::new(0) })
    }

    /// The bytes all of the run's live stores hold now.
    #[cfg(test)]
    pub(crate) fn used(&self) -> usize {
        self.used.load(Ordering::SeqCst)
    }
}

/// One store's share of its run's [`Budget`]: what it was granted, given
/// back when the store is dropped.
pub(crate) struct Meter {
    budget: Arc<Budget>,
    mine: usize,
    /// The last grant, given back if wasmtime then fails the growth.
    last: usize,
}

impl Meter {
    pub(crate) fn new(budget: Arc<Budget>) -> Self {
        Meter { budget, mine: 0, last: 0 }
    }

    /// Grant `delta` more bytes if the run's total stays within the cap.
    pub(crate) fn reserve(&mut self, delta: usize) -> bool {
        let cap = self.budget.cap;
        let granted = self
            .budget
            .used
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |u| u.checked_add(delta).filter(|&n| n <= cap))
            .is_ok();
        if granted {
            self.mine += delta;
            self.last = delta;
        }
        granted
    }

    /// Give back the last grant (wasmtime could not perform the growth).
    pub(crate) fn undo_last(&mut self) {
        self.budget.used.fetch_sub(self.last, Ordering::SeqCst);
        self.mine -= self.last;
        self.last = 0;
    }
}

impl Drop for Meter {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.mine, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::{Budget, Env, Meter, WASM32_SPACE, cap_from};

    /// A directory laid out like `/` with the given files.
    fn env_with(files: &[(&str, &str)]) -> (Env, std::path::PathBuf) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!("almide-budget-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&root);
        for (rel, text) in files {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
            std::fs::write(p, text).expect("write");
        }
        (Env { root: root.clone(), ask_os: false }, root)
    }

    const MIB: usize = 1 << 20;

    /// The ruling of 2026-10-26: an uncapped run's budget follows the
    /// environment — the tightest cgroup limit on the path, physical memory,
    /// 4 GiB — and is 4 GiB when nothing can be read. Negative control
    /// (2026-10-26): with `cap_from` folding without the `min`, the first row
    /// failed, "v2 child" left 4294967296 right 67108864.
    #[test]
    fn the_default_cap_is_the_least_limit_the_environment_sets() {
        let gib2 = "MemTotal:        2097152 kB\n";
        let rows: [(&str, Vec<(&str, &str)>, usize); 6] = [
            // cgroup v2: a child's limit under an unlimited parent
            ("v2 child", vec![("proc/self/cgroup", "0::/app/job\n"), ("sys/fs/cgroup/app/memory.max", "max\n"), ("sys/fs/cgroup/app/job/memory.max", "67108864\n"), ("proc/meminfo", gib2)], 64 * MIB),
            // a parent's limit binds its unlimited child
            ("v2 parent", vec![("proc/self/cgroup", "0::/app/job\n"), ("sys/fs/cgroup/app/memory.max", "134217728\n"), ("sys/fs/cgroup/app/job/memory.max", "max\n"), ("proc/meminfo", gib2)], 128 * MIB),
            // no cgroup limit: physical memory
            ("v2 unlimited", vec![("proc/self/cgroup", "0::/\n"), ("sys/fs/cgroup/memory.max", "max\n"), ("proc/meminfo", gib2)], 2048 * MIB),
            // cgroup v1
            ("v1", vec![("proc/self/cgroup", "12:pids:/x\n4:cpu,memory:/x\n"), ("sys/fs/cgroup/memory/x/memory.limit_in_bytes", "268435456\n"), ("proc/meminfo", gib2)], 256 * MIB),
            // v1 "unlimited" and more RAM than wasm32 can address: 4 GiB
            ("v1 unlimited", vec![("proc/self/cgroup", "4:memory:/\n"), ("sys/fs/cgroup/memory/memory.limit_in_bytes", "9223372036854771712\n"), ("proc/meminfo", "MemTotal: 67108864 kB\n")], WASM32_SPACE),
            // nothing readable
            ("unknown", vec![], WASM32_SPACE),
        ];
        for (name, files, want) in rows {
            let (env, root) = env_with(&files);
            assert_eq!(cap_from(&env), want, "{name}");
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn two_meters_share_one_cap_and_give_it_back_when_dropped() {
        let b = Budget::new(Some(100));
        let mut one = Meter::new(b.clone());
        let mut two = Meter::new(b.clone());
        assert!(one.reserve(60));
        assert!(!two.reserve(41), "60 + 41 exceeds the one budget");
        assert!(two.reserve(40));
        assert_eq!(b.used(), 100);
        two.undo_last();
        assert_eq!(b.used(), 60);
        drop(one);
        assert_eq!(b.used(), 0);
        assert!(two.reserve(100));
    }
}
