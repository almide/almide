//! The compile cache of `almide run --target wasm` (and `almide bench`, the
//! other embedded-leg caller): an unchanged program reruns without
//! re-rendering its module.
//!
//! The native leg never pays a second rustc for the same program — its
//! binaries are content-keyed in the shared run dir (`build_native_cached`).
//! The wasm leg recompiled everything on every run: ~1 s for a 300–1600-line
//! program, slower than native's cached rerun. This cache stores the
//! structural leg's module next to those binaries, under the same rules:
//!
//! - **Where**: `<shared run dir>/target/wasm/almide-<key>` with its SHA-256
//!   in `almide-<key>.sha256` (#4180: a hit is an entry whose bytes still hash
//!   to the digest recorded when it was written, never a name that exists).
//!   `almide clean` empties the dir with everything else in it.
//! - **Eviction**: the age sweep (`evict_stale_artifacts`) and the size bound
//!   (`ALMIDE_CACHE_MAX_BYTES`, `cache_bound::bound_build_artifacts`) cover
//!   `target/wasm/` exactly as they cover `target/<profile>/`; a hit touches
//!   the entry so it counts as used.
//! - **The key**: what the structural leg reads to render the module — the
//!   entry file's path and source, the path and source of every module the
//!   resolver read off disk (the program's own modules and its packages), the
//!   dependency dirs, the working directory — plus the compiler itself (its
//!   version line and the identity of the running executable: the bundled
//!   stdlib is compiled into it) and every `ALMIDE_*` variable in the
//!   environment (the emission switches — alloc counters, allocator modes,
//!   p3 — are all spelled that way; an unrelated one only costs a miss).
//!
//! What the cache does NOT skip: the front end. Parsing, the type check
//! (warnings included), the `[permissions]` gate, IR verification and the
//! E081 availability check all run on every run before the lookup, so a hit
//! prints the same diagnostics a miss does and refuses what a miss refuses.
//! Only the structural render (the leg's own front, emit, validation) is
//! skipped. A render that walls is never cached.

use sha2::{Digest, Sha256};

/// The first line of an entry; bumped when the layout changes.
const MAGIC: &str = "almide-wasm-run-cache 1";

/// A cache entry's identity, computed from the render's inputs.
pub(super) struct Key {
    path: std::path::PathBuf,
}

impl Key {
    /// The key of rendering `file` (whose text is `source_text`) with the
    /// resolved module set `resolved` and the package dirs `dep_paths`.
    /// `None` when the cache is off for this run (`ALMIDE_VERIFIED_DEBUG`
    /// narrates the render, which a hit would silently skip).
    pub(super) fn of(
        file: &str,
        source_text: &str,
        resolved: &crate::resolve::ResolvedModules,
        dep_paths: &[(crate::project::PkgId, std::path::PathBuf)],
        verified: bool,
    ) -> Option<Self> {
        if almide_base::env::flag("ALMIDE_VERIFIED_DEBUG") {
            return None;
        }
        let mut h = Sha256::new();
        let mut field = |label: &str, value: &[u8]| {
            // Length-prefixed, so no two field sequences hash alike.
            h.update(label.as_bytes());
            h.update((value.len() as u64).to_le_bytes());
            h.update(value);
        };
        field("magic", MAGIC.as_bytes());
        field("compiler", compiler_identity().as_bytes());
        field("verified", &[verified as u8]);
        let cwd = std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
        field("cwd", cwd.as_bytes());
        field("entry", file.as_bytes());
        field("entry-source", source_text.as_bytes());
        let mut sources: Vec<(&String, &(String, String))> = resolved.sources.iter().collect();
        sources.sort_by(|a, b| a.0.cmp(b.0));
        for (name, (path, text)) in sources {
            field("module", name.as_bytes());
            field("module-path", path.as_bytes());
            field("module-source", text.as_bytes());
        }
        let mut names: Vec<&str> = resolved.modules.iter().map(|(n, ..)| n.as_str()).collect();
        names.sort_unstable();
        field("module-set", names.join(",").as_bytes());
        for (pkg, dir) in dep_paths {
            field("dep", format!("{pkg:?}={}", dir.display()).as_bytes());
        }
        let mut vars: Vec<(String, String)> = std::env::vars().filter(|(k, _)| k.starts_with("ALMIDE_")).collect();
        vars.sort();
        for (k, v) in vars {
            field("env", format!("{k}={v}").as_bytes());
        }
        let digest: String = h.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect();
        let path = super::build_dir::shared_run_project_dir().join("target").join("wasm").join(format!("almide-{digest}"));
        Some(Key { path })
    }

    /// The cached module and its host ops, when an intact entry exists.
    pub(super) fn lookup(&self) -> Option<(Vec<u8>, Vec<i32>)> {
        if !super::run::cached_binary_intact(&self.path) {
            return None;
        }
        let raw = std::fs::read(&self.path).ok()?;
        let entry = decode(&raw)?;
        super::build_dir::touch_used(&self.path);
        super::build_dir::touch_used(&super::run::binary_digest_path(&self.path));
        Some(entry)
    }

    /// Record a rendered module. Best effort: a cache that cannot be written
    /// costs the next run a render, never this run its result.
    pub(super) fn store(&self, bytes: &[u8], host_ops: &[i32]) {
        let Some(dir) = self.path.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        // Housekeeping on a miss, as the native builder does — but never
        // waiting: a cargo build holding the dir's lock sweeps it itself.
        let project_dir = super::build_dir::shared_run_project_dir();
        if let Some(_lock) = super::build_dir::BuildDirLock::try_acquire(&project_dir) {
            super::build_dir::evict_stale_artifacts(&project_dir);
            if let Some(cap) = super::cache_bound::cache_max_bytes() {
                super::cache_bound::bound_build_artifacts(&project_dir, cap);
            }
        }
        // Staged + renamed (an entry is never read half-written), then its
        // digest the same way.
        let staged = self.path.with_extension(format!("stage-{}", std::process::id()));
        if std::fs::write(&staged, encode(bytes, host_ops)).is_err() {
            let _ = std::fs::remove_file(&staged);
            return;
        }
        if std::fs::rename(&staged, &self.path).is_err() {
            let _ = std::fs::remove_file(&staged);
            return;
        }
        let _ = super::run::record_binary_digest(&self.path);
    }
}

/// The running compiler: its version line, plus the path, size and mtime of
/// its executable — a rebuilt compiler (same version string, new code) is a
/// different renderer, and hashing the binary itself would cost more than the
/// render it saves.
fn compiler_identity() -> String {
    let exe = std::env::current_exe().ok();
    let meta = exe.as_ref().and_then(|p| std::fs::metadata(p).ok());
    let mtime = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    format!(
        "{}|{}|{}|{}",
        env!("ALMIDE_VERSION_LINE"),
        exe.map(|p| p.display().to_string()).unwrap_or_default(),
        meta.map_or(0, |m| m.len()),
        mtime
    )
}

/// `MAGIC\n<op>,<op>,…\n<module bytes>`.
fn encode(bytes: &[u8], host_ops: &[i32]) -> Vec<u8> {
    let ops = host_ops.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
    let mut out = format!("{MAGIC}\n{ops}\n").into_bytes();
    out.extend_from_slice(bytes);
    out
}

/// The inverse of [`encode`]; `None` for anything it did not write.
fn decode(raw: &[u8]) -> Option<(Vec<u8>, Vec<i32>)> {
    let rest = raw.strip_prefix(format!("{MAGIC}\n").as_bytes())?;
    let nl = rest.iter().position(|&b| b == b'\n')?;
    let ops_line = std::str::from_utf8(&rest[..nl]).ok()?;
    let ops = if ops_line.is_empty() {
        Vec::new()
    } else {
        ops_line.split(',').map(|s| s.parse().ok()).collect::<Option<Vec<i32>>>()?
    };
    Some((rest[nl + 1..].to_vec(), ops))
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn an_entry_round_trips() {
        let bytes = b"\0asm\x01\0\0\0\n\nbody".to_vec();
        for ops in [vec![], vec![3], vec![1, -2, 40]] {
            assert_eq!(decode(&encode(&bytes, &ops)), Some((bytes.clone(), ops.clone())));
        }
    }

    #[test]
    fn a_foreign_file_is_not_an_entry() {
        assert_eq!(decode(b"\0asm\x01\0\0\0"), None);
        assert_eq!(decode(b"almide-wasm-run-cache 1\nx\n"), None);
    }
}
