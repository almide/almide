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
    File(String),
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
        Some(VfsEntry::File(content)) => Ok(content.clone()),
        Some(VfsEntry::Dir) => Err("Is a directory (os error 21)".to_string()),
        None => std::fs::read_to_string(path).map_err(|e| format!("{e}")),
    }
}

/// `prim.write_text_file` — into the overlay only. The parent must exist
/// (overlay dir, or a real directory), the same precondition `std::fs::write`
/// enforces natively.
pub(crate) fn write_text(vfs: &mut Vfs, path: &str, content: &str) -> Result<(), String> {
    let path = &normalize(path);
    if matches!(vfs.get(path.as_str()), Some(VfsEntry::Dir)) {
        return Err("Is a directory (os error 21)".to_string());
    }
    if let Some(parent) = parent_of(path) {
        let in_overlay = matches!(vfs.get(parent), Some(VfsEntry::Dir));
        if !in_overlay && !std::path::Path::new(parent).is_dir() {
            return Err(ENOENT.to_string());
        }
    }
    vfs.insert(path.to_string(), VfsEntry::File(content.to_string()));
    Ok(())
}

/// `prim.make_dir` — `create_dir_all` semantics: every ancestor becomes a dir,
/// an existing dir is Ok, an existing FILE at the path is the EEXIST error.
pub(crate) fn make_dir(vfs: &mut Vfs, path: &str) -> Result<(), String> {
    let path = &normalize(path);
    if matches!(vfs.get(path.as_str()), Some(VfsEntry::File(_))) {
        return Err("File exists (os error 17)".to_string());
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
