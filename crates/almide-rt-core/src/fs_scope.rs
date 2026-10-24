// fs_scope (#4022): `[permissions] fs = ["./data", "/tmp"]` — the directories
// a program may touch at all. ONE source for both legs: fs_scoped_core.rs
// includes it, so it reaches the native runtime through runtime/rs/src/fs.rs
// (the codegen build script inlines the include chain) and the embedded wasm
// host as `almide_rt_core::fs_beneath_core`; the two admit the same paths and
// refuse with the same bytes.
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
//
// This resolution only DECIDES (which root, which segments under it); it is
// not what the operation follows. The operation runs beneath a descriptor of
// that root held since its first use (fs_scoped_core.rs, #4084), so a tree
// rewritten between the decision and the call (a directory swapped for a
// symlink) is refused by the call itself instead of escaping. The p3
// component's preopens are resolved beneath their descriptor by the runtime
// (cap-std) the same way.

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
    fs_scope_resolve_parts(path).map(|(out, _)| out)
}

/// [`fs_scope_resolve`], with the error the OS gives a path whose `..`
/// steps back over a component that is no directory (`Some`): the OS walks
/// through that component and fails there (ENOENT, ENOTDIR), where the
/// resolution here steps back over it.
pub fn fs_scope_resolve_parts(path: &str) -> Option<(std::path::PathBuf, Option<std::io::Error>)> {
    use std::path::{Component, Path, PathBuf};
    let p = Path::new(path);
    let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().ok()?.join(p) };
    let comps: Vec<Component> = abs.components().collect();
    let dotdot = comps.iter().enumerate().skip(1).filter(|(_, c)| matches!(c, Component::ParentDir)).find_map(|(j, _)| {
        let before: PathBuf = comps[..j].iter().collect();
        match std::fs::metadata(&before) {
            Err(e) => Some(e),
            Ok(m) if !m.is_dir() => Some(std::io::Error::from_raw_os_error(20)),
            Ok(_) => None,
        }
    });
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
            return Some((out, dotdot));
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
