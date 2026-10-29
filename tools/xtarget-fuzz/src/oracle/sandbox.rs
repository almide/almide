//! The fixed world every run sees (docs/roadmap/active/effect-inspection-harness.md,
//! tier E1): a fresh directory seeded with the same tree, the same argv, env
//! and stdin on every leg, and a snapshot of the tree after the run. Two legs
//! that write differently disagree even when they print the same thing.
//!
//! A program that touches no effect sees an extra cwd, argv and stdin it never
//! reads, and an unchanged tree, so pure programs compare exactly as before.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Program arguments after the program name, on every leg.
pub const ARGS: &[&str] = &["alpha", "--flag", "beta", "--opt=7"];

/// Environment variables set on every leg (on top of the harness's own).
pub const ENV: &[(&str, &str)] = &[("XF_GREETING", "héllo"), ("XF_EMPTY", "")];

/// What stdin holds on every leg.
pub const STDIN: &str = "first line\nsecond λ line\n\nlast";

/// The seeded tree: (relative path, contents). Directories are implied by the
/// paths; `empty_dir/` is created explicitly.
const FILES: &[(&str, &[u8])] = &[
    ("data.txt", "line one\nline two\nλ three\n".as_bytes()),
    ("empty.txt", b""),
    ("bin.dat", &[0, 1, 2, 127, 128, 254, 255, 10, 13, 0]),
    ("sub/nested.txt", b"nested\n"),
    ("sub/deeper/x.txt", b"x"),
];

static NEXT: AtomicU64 = AtomicU64::new(0);

/// The run's temp dir, beside the tree rather than in it: `fs.create_temp_*`
/// names what it creates after the clock, so what it leaves cannot be
/// compared byte for byte (ADR-0027 D3), and C-189 lets each leg place its
/// temp dir differently. Every leg is pointed here: native and the embedded
/// host through `TMPDIR`, a stock runtime through its `/tmp` preopen.
pub fn tmp_of(dir: &Path) -> PathBuf {
    dir.with_extension("tmp")
}

/// A fresh sandbox under `scratch`, seeded with the fixed tree and the stdin
/// file. The stdin file lives beside the sandbox, so it is not in the tree.
pub fn create(scratch: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = scratch.join(format!("sbx-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("empty_dir"))?;
    let _ = std::fs::remove_dir_all(tmp_of(&dir));
    std::fs::create_dir_all(tmp_of(&dir))?;
    for (rel, bytes) in FILES {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(p, bytes)?;
    }
    // A relative link inside the tree, for `fs.is_symlink` (a host that
    // allows none leaves it out).
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink("data.txt", dir.join("link.txt"));
    let stdin = scratch.join(format!("sbx-{}-{n}.stdin", std::process::id()));
    std::fs::write(&stdin, STDIN)?;
    Ok((dir, stdin))
}

/// The tree under `dir`, one sorted line per entry: `d <path>` or
/// `f <path> <size> <fnv1a64>`. Timestamps and permissions are left out:
/// they are not observables the legs promise to agree on.
pub fn snapshot(dir: &Path) -> String {
    let mut lines = Vec::new();
    walk(dir, dir, &mut lines);
    lines.sort();
    let mut out = String::new();
    for l in lines {
        let _ = writeln!(out, "{l}");
    }
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let rel = p
            .strip_prefix(root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            out.push(format!("d {rel}"));
            walk(root, &p, out);
        } else if ft.is_symlink() {
            let target = std::fs::read_link(&p)
                .map(|t| t.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push(format!("l {rel} -> {target}"));
        } else {
            let bytes = std::fs::read(&p).unwrap_or_default();
            out.push(format!("f {rel} {} {:016x}", bytes.len(), fnv1a(&bytes)));
        }
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}

/// Remove a sandbox and its stdin file.
pub fn remove(dir: &Path, stdin: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(tmp_of(dir));
    let _ = std::fs::remove_file(stdin);
}

/// One run's sandbox, removed when dropped: the inspection harness enters a
/// fresh one for every probe run on every leg (ADR-0027 D2).
pub struct World {
    pub dir: PathBuf,
    stdin: PathBuf,
}

impl World {
    pub fn new(scratch: &Path) -> Result<World, String> {
        let (dir, stdin) = create(scratch).map_err(|e| format!("sandbox: {e}"))?;
        Ok(World { dir, stdin })
    }

    /// [`Self::new`], with the stdin a probe's `// inspect-stdin: L N` header
    /// asks for: N lines of L `a`s each (the inspection's stdin readers).
    pub fn for_program(scratch: &Path, src: &str) -> Result<World, String> {
        let w = World::new(scratch)?;
        let spec = src
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("// inspect-stdin: "));
        if let Some((len, lines)) = spec.and_then(|s| s.split_once(' ')) {
            let (len, lines): (usize, usize) = (
                len.parse().map_err(|_| "bad stdin header")?,
                lines.trim().parse().map_err(|_| "bad stdin header")?,
            );
            let line = format!("{}\n", "a".repeat(len));
            std::fs::write(&w.stdin, line.repeat(lines))
                .map_err(|e| format!("sandbox stdin: {e}"))?;
        }
        Ok(w)
    }

    /// The run's cwd, env and stdin; its argv is [`ARGS`], after the program
    /// (and after `--` for `almide run`).
    pub fn enter(&self, cmd: &mut std::process::Command) {
        cmd.current_dir(&self.dir)
            .envs(ENV.iter().copied())
            .env("TMPDIR", tmp_of(&self.dir));
        cmd.envs(super::fixture::env().iter().map(|(k, v)| (k, v)));
        // The tree's absolute path, for a run whose cwd is elsewhere (a
        // diamond's project); a stock runtime sees the tree as `.` instead.
        cmd.env("XF_SANDBOX", &self.dir);
        if let Ok(f) = std::fs::File::open(&self.stdin) {
            cmd.stdin(f);
        }
    }

    /// What a stock `wasmtime run` needs before the module to see the same
    /// world: the sandbox preopened as the guest's `.`, and the env.
    pub fn wasmtime_args(&self) -> Vec<String> {
        let mut a = vec![
            format!("--dir={}::.", self.dir.display()),
            format!("--dir={}::/tmp", tmp_of(&self.dir).display()),
        ];
        a.extend(ENV.iter().map(|(k, v)| format!("--env={k}={v}")));
        a.extend(
            super::fixture::env()
                .iter()
                .map(|(k, v)| format!("--env={k}={v}")),
        );
        a
    }
}

impl Drop for World {
    fn drop(&mut self) {
        remove(&self.dir, &self.stdin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_sandbox_snapshots_the_seeded_tree_and_sees_a_write() {
        let scratch = std::env::temp_dir().join(format!("xf-sbx-test-{}", std::process::id()));
        std::fs::create_dir_all(&scratch).unwrap();
        let (dir, stdin) = create(&scratch).unwrap();
        let before = snapshot(&dir);
        assert!(before.contains("f data.txt 27 "), "{before}");
        assert!(before.contains("d empty_dir\n"), "{before}");
        assert!(before.contains("f sub/deeper/x.txt 1 "), "{before}");
        assert!(
            !before.contains(".stdin"),
            "stdin lives outside the tree: {before}"
        );
        std::fs::write(dir.join("out.txt"), "w").unwrap();
        let after = snapshot(&dir);
        assert_ne!(before, after);
        assert!(after.contains("f out.txt 1 "), "{after}");
        remove(&dir, &stdin);
        assert!(!dir.exists() && !stdin.exists());
        let _ = std::fs::remove_dir_all(&scratch);
    }
}
