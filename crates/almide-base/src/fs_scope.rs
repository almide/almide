// fs_scope (#4022): `[permissions] fs = ["./data", "/tmp"]` — the directories
// a program may touch at all. ONE source for both legs: the native runtime
// splices this file into runtime/rs/src/fs.rs (`include!`, inlined by the
// codegen build script) and the embedded wasm host reaches it as
// `almide_base::fs_scope`, so the two admit the same paths and refuse with the
// same bytes.
//
// Std only, every item prefixed `fs_scope_`: the runtime is emitted as one flat
// module, where a bare helper name would collide with another file's.
//
// Resolution, not spelling: a path is admitted when the place the OS would
// reach lies under a granted root. The longest existing prefix of the
// absolute path is `canonicalize`d — every symlink and `..` along it resolved
// by the OS itself — and the not-yet-existing rest is applied lexically (it
// holds no symlink because it does not exist). A dangling symlink in that rest
// is refused: a write through it would create its target, wherever that is.
// The check runs before the operation, so a tree an attacker rewrites between
// the two (a directory swapped for a symlink) can still race it; the rooted
// API (`fs.open_root`) has the same window. Closing it needs descriptor-based
// resolution (openat2 RESOLVE_BENEATH on native, WASI preopens on wasm).

/// The refusal for a path outside every granted root, on every leg.
pub fn fs_scope_refusal(path: &str) -> String {
    format!("fs: \"{path}\" is outside [permissions] fs")
}

/// A granted root as the comparison needs it: absolute against the cwd and,
/// when it exists, with its own symlinks resolved (macOS `/tmp` is
/// `/private/tmp`).
pub fn fs_scope_root(root: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(root);
    let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(p) };
    std::fs::canonicalize(&abs).unwrap_or(abs)
}

/// Where `path` lands, or `None` when that cannot be vouched for (a dangling
/// symlink on the way).
pub fn fs_scope_resolve(path: &str) -> Option<std::path::PathBuf> {
    use std::path::{Component, Path, PathBuf};
    let p = Path::new(path);
    let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().ok()?.join(p) };
    let comps: Vec<Component> = abs.components().collect();
    let mut k = comps.len();
    while k > 0 {
        let prefix: PathBuf = comps[..k].iter().collect();
        if let Ok(mut out) = std::fs::canonicalize(&prefix) {
            for c in &comps[k..] {
                match c {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        out.pop();
                    }
                    other => {
                        out.push(other.as_os_str());
                        if std::fs::symlink_metadata(&out).is_ok_and(|m| m.file_type().is_symlink()) {
                            return None;
                        }
                    }
                }
            }
            return Some(out);
        }
        k -= 1;
    }
    None
}

/// Whether `path` lands under one of `roots` (each from [`fs_scope_root`]).
/// Every operation is judged by where the path leads, the no-follow ones
/// (`is_symlink`, `remove`) included: a link inside a granted directory that
/// points out of it can be neither asked about nor removed under the scope.
pub fn fs_scope_admits(path: &str, roots: &[std::path::PathBuf]) -> bool {
    fs_scope_resolve(path).is_some_and(|r| roots.iter().any(|root| r.starts_with(root)))
}

/// The directory a glob pattern reads: its segments before the first one
/// holding a wildcard (`.` when the first does).
pub fn fs_scope_glob_base(pattern: &str) -> String {
    let segs: Vec<&str> = pattern.split('/').take_while(|s| !s.contains(['*', '?', '['])).collect();
    let base = segs.join("/");
    if base.is_empty() {
        if pattern.starts_with('/') { "/".to_string() } else { ".".to_string() }
    } else {
        base
    }
}
