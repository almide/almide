//! The sandboxed fs floor (#1218): an in-memory overlay per interpreter.
//!
//! The backend legs run as CHILD PROCESSES (harness tempdir; wasmtime with a
//! preopen), so their fs effects are isolated per run. The interp runs
//! IN-PROCESS, and `cargo test` runs the ledger and voting gates in parallel
//! threads — a real-`std::fs` floor would have two threads racing the same
//! fixture's writes in one shared cwd (`chdir` is process-global, no help).
//! So: WRITES land in this overlay, never on disk; READS consult the overlay
//! first and fall back to the REAL filesystem read-only — a fixture observes
//! its own writes plus the host truth, which is exactly what the other two
//! legs observe. Deterministic, race-free, grep-provably write-free
//! (`std::fs::` appears here only in read forms).
//!
//! Error strings mirror the native runtime's `io_err` (= `Display` of
//! `std::io::Error`, `runtime/rs/src/fs.rs`); overlay-synthesized errors use
//! the exact `(os error N)` spellings, which are identical on the linux and
//! macos hosts the suite runs on.

use std::collections::HashMap;

pub(crate) enum VfsEntry {
    /// The file's BYTES — not a String: `fs.write_bytes` lands raw bytes
    /// (a latin1 line, a truncated sequence) and the text read must then
    /// answer the backends' InvalidData error, not silently re-encode.
    File(Vec<u8>),
    Dir,
}

pub(crate) type Vfs = HashMap<String, VfsEntry>;

fn parent_of(path: &str) -> Option<&str> {
    path.rsplit_once('/').map(|(p, _)| p).filter(|p| !p.is_empty())
}

/// Overlay-key normalization: `.` segments and doubled slashes collapse, so
/// `/tmp/./x` and `/tmp/x` name the SAME entry — the exact identity C-042's
/// fixture pins (`fs.write(path)` then `fs.read_text` through the
/// `./`-normalized spelling of the same path; the wasm leg's `$path_norm`
/// makes the same collapse). Without this the read misses the overlay and
/// falls back to the real fs — where a leftover from another leg's earlier
/// run answers with stale bytes (measured: the voting gate caught exactly
/// that). `..` is deliberately NOT resolved: none of the legs' contracts pin
/// it, and a wrong collapse would be a wrong vote.
pub(crate) fn normalize(path: &str) -> String {
    let absolute = path.starts_with('/');
    let joined = path
        .split('/')
        .filter(|seg| !seg.is_empty() && *seg != ".")
        .collect::<Vec<_>>()
        .join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

const ENOENT: &str = "No such file or directory (os error 2)";

/// `prim.read_text_file` — overlay first, then a READ-ONLY real-fs fallback.
pub(crate) fn read_text(vfs: &Vfs, path: &str) -> Result<String, String> {
    let path = &normalize(path);
    match vfs.get(path.as_str()) {
        // The exact `std::fs::read_to_string` InvalidData Display the native
        // runtime's `io_err` forwards (and the wasm text floor spells, #1506).
        Some(VfsEntry::File(content)) => String::from_utf8(content.clone())
            .map_err(|_| "stream did not contain valid UTF-8".to_string()),
        Some(VfsEntry::Dir) => Err("Is a directory (os error 21)".to_string()),
        None => std::fs::read_to_string(path).map_err(|e| format!("{e}")),
    }
}

/// `prim.write_text_file` — into the overlay only. The parent must exist
/// (overlay dir, or a real directory), the same precondition `std::fs::write`
/// enforces natively.
/// The content is BYTES: what `prim.write_text_file` really carries once a
/// stdlib body has filled an `alloc_str` block byte by byte
/// (`fs.write_bytes` / `fs.write_bytes_raw`) — a String arrives as its UTF-8.
pub(crate) fn write_bytes(vfs: &mut Vfs, path: &str, content: &[u8]) -> Result<(), String> {
    let path = &normalize(path);
    if matches!(vfs.get(path.as_str()), Some(VfsEntry::Dir)) {
        return Err(EISDIR.to_string());
    }
    if ancestor_is_file(vfs, path) {
        return Err(ENOTDIR.to_string());
    }
    if let Some(parent) = parent_of(path) {
        let in_overlay = matches!(vfs.get(parent), Some(VfsEntry::Dir));
        if !in_overlay && !std::path::Path::new(parent).is_dir() {
            return Err(ENOENT.to_string());
        }
    }
    vfs.insert(path.to_string(), VfsEntry::File(content.to_vec()));
    Ok(())
}

/// A path component ABOVE `path` that is a file — in the overlay or on the
/// real filesystem: every write-side floor answers the native ENOTDIR for
/// it (`std::fs::write` / `create_dir_all` through a plain file), before any
/// "parent missing" verdict.
fn ancestor_is_file(vfs: &Vfs, path: &str) -> bool {
    let mut p = path;
    while let Some(parent) = parent_of(p) {
        if matches!(vfs.get(parent), Some(VfsEntry::File(_)))
            || std::path::Path::new(parent).is_file()
        {
            return true;
        }
        p = parent;
    }
    false
}

/// `prim.make_dir` — `create_dir_all` semantics: every ancestor becomes a dir,
/// an existing dir is Ok, an existing FILE at the path is the EEXIST error.
pub(crate) fn make_dir(vfs: &mut Vfs, path: &str) -> Result<(), String> {
    let path = &normalize(path);
    if matches!(vfs.get(path.as_str()), Some(VfsEntry::File(_))) {
        return Err("File exists (os error 17)".to_string());
    }
    if ancestor_is_file(vfs, path) {
        return Err(ENOTDIR.to_string());
    }
    let mut p = path.as_str();
    loop {
        vfs.insert(p.to_string(), VfsEntry::Dir);
        match parent_of(p) {
            Some(pp) => p = pp,
            None => break,
        }
    }
    Ok(())
}

/// `prim.path_exists` — overlay hit, or the real filesystem (read-only).
pub(crate) fn exists(vfs: &Vfs, path: &str) -> bool {
    let norm = normalize(path);
    vfs.contains_key(&norm) || std::path::Path::new(path).exists()
}

/// `prim.remove_all` — removes an overlay subtree. A path that exists ONLY on
/// the real filesystem is `Err(Unsupported)`-shaped at the caller (the overlay
/// is read-only toward the host; silently "succeeding" without removing would
/// be a wrong vote), and a path that exists nowhere is the native ENOENT.
pub(crate) enum RemoveOutcome {
    Removed,
    HostOnly,
    Missing,
}

pub(crate) fn remove_all(vfs: &mut Vfs, path: &str) -> RemoveOutcome {
    let path = &normalize(path);
    let sub_prefix = format!("{path}/");
    let keys: Vec<String> = vfs
        .keys()
        .filter(|k| k == &path || k.starts_with(&sub_prefix))
        .cloned()
        .collect();
    if keys.is_empty() {
        return if std::path::Path::new(path).exists() {
            RemoveOutcome::HostOnly
        } else {
            RemoveOutcome::Missing
        };
    }
    for k in keys {
        vfs.remove(&k);
    }
    RemoveOutcome::Removed
}

const ENOTDIR: &str = "Not a directory (os error 20)";
const EISDIR: &str = "Is a directory (os error 21)";

/// `prim.read_bytes_file` — the bytes floor: the overlay's bytes as they
/// were written, else a read-only real-fs `std::fs::read`.
pub(crate) fn read_bytes(vfs: &Vfs, path: &str) -> Result<Vec<u8>, String> {
    let path = &normalize(path);
    match vfs.get(path.as_str()) {
        Some(VfsEntry::File(content)) => Ok(content.clone()),
        Some(VfsEntry::Dir) => Err(EISDIR.to_string()),
        None => std::fs::read(path).map_err(|e| format!("{e}")),
    }
}

/// `prim.path_filestat` — the three WASI filestat fields the stdlib bodies
/// read back (`filetype@16`: 3 = directory, 4 = regular file; `size@32`;
/// `mtim@48` in nanoseconds), or `None` for a path that exists nowhere. An
/// overlay entry stamps the CURRENT time: the backend legs write real files
/// and stat them in the same run, so "now" is the faithful reading of the
/// field they see (a fixture can only assert its shape, never its value).
pub(crate) fn stat(vfs: &Vfs, path: &str) -> Option<(u8, u64, i64)> {
    stat_with(vfs, path, true)
}

/// `prim.path_filestat_nofollow` — the same query without following a
/// symlink at the leaf (filetype 7 = symbolic link on the real fs; the
/// overlay holds no symlinks, so its answer is `stat`'s).
pub(crate) fn stat_nofollow(vfs: &Vfs, path: &str) -> Option<(u8, u64, i64)> {
    stat_with(vfs, path, false)
}

fn stat_with(vfs: &Vfs, path: &str, follow: bool) -> Option<(u8, u64, i64)> {
    let norm = normalize(path);
    let now_ns = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0)
    };
    match vfs.get(norm.as_str()) {
        Some(VfsEntry::File(content)) => Some((4, content.len() as u64, now_ns())),
        Some(VfsEntry::Dir) => Some((3, 0, now_ns())),
        None => {
            let md = if follow {
                std::fs::metadata(path).ok()?
            } else {
                std::fs::symlink_metadata(path).ok()?
            };
            let ftype = if md.file_type().is_symlink() {
                7
            } else if md.is_dir() {
                3
            } else if md.is_file() {
                4
            } else {
                0
            };
            let mtime = md
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0);
            Some((ftype, md.len(), mtime))
        }
    }
}

/// `prim.read_dir` — the sorted names under `path`: the overlay's direct
/// children (a file or a dir written under it) merged with the real
/// directory's entries when one exists there, `names.sort()`ed like native
/// `almide_rt_fs_list_dir` (the wasm leg sorts the same way). A missing path
/// is the native ENOENT, a file the native ENOTDIR.
pub(crate) fn read_dir(vfs: &Vfs, path: &str) -> Result<Vec<String>, String> {
    let norm = normalize(path);
    let overlay_dir = matches!(vfs.get(norm.as_str()), Some(VfsEntry::Dir));
    if matches!(vfs.get(norm.as_str()), Some(VfsEntry::File(_))) {
        return Err(ENOTDIR.to_string());
    }
    let real = std::path::Path::new(path);
    let mut names: Vec<String> = Vec::new();
    if real.is_dir() {
        let entries = std::fs::read_dir(path).map_err(|e| format!("{e}"))?;
        for entry in entries {
            let e = entry.map_err(|e| format!("{e}"))?;
            names.push(e.file_name().to_string_lossy().to_string());
        }
    } else if !overlay_dir {
        return Err(if real.exists() { ENOTDIR.to_string() } else { ENOENT.to_string() });
    }
    let prefix = format!("{norm}/");
    for k in vfs.keys() {
        if let Some(rest) = k.strip_prefix(&prefix) {
            let child = rest.split('/').next().unwrap_or(rest).to_string();
            if !child.is_empty() && !names.contains(&child) {
                names.push(child);
            }
        }
    }
    names.sort();
    Ok(names)
}

/// `prim.rename` over the overlay. `HostOnly` is the source that exists only
/// on the real filesystem — the overlay is read-only toward the host, and a
/// rename that "succeeded" without moving anything would be a wrong vote,
/// so the caller abstains. A missing source is the native ENOENT; a
/// destination whose parent exists nowhere is ENOENT too (`std::fs::rename`
/// does not create directories); an existing destination file is replaced,
/// as on both legs.
pub(crate) enum RenameOutcome {
    Renamed,
    HostOnly,
    Failed(String),
}

pub(crate) fn rename(vfs: &mut Vfs, src: &str, dst: &str) -> RenameOutcome {
    let src = normalize(src);
    let dst = normalize(dst);
    let sub_prefix = format!("{src}/");
    let keys: Vec<String> = vfs
        .keys()
        .filter(|k| **k == src || k.starts_with(&sub_prefix))
        .cloned()
        .collect();
    if keys.is_empty() {
        return if std::path::Path::new(&src).exists() {
            RenameOutcome::HostOnly
        } else {
            RenameOutcome::Failed(ENOENT.to_string())
        };
    }
    if let Some(parent) = parent_of(&dst) {
        let in_overlay = matches!(vfs.get(parent), Some(VfsEntry::Dir));
        if !in_overlay && !std::path::Path::new(parent).is_dir() {
            return RenameOutcome::Failed(ENOENT.to_string());
        }
    }
    let moved: Vec<(String, VfsEntry)> = keys
        .into_iter()
        .filter_map(|k| {
            let entry = vfs.remove(&k)?;
            let tail = &k[src.len()..];
            Some((format!("{dst}{tail}"), entry))
        })
        .collect();
    for (k, e) in moved {
        vfs.insert(k, e);
    }
    RenameOutcome::Renamed
}
