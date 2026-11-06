//! What a tool that serves one project may touch: paths confined to the
//! project root, and the directories a sweep over the project skips.
//!
//! Shared by `almide mcp` (#4178), the language server (#4185) and every
//! directory walker (`fmt`, `test`, the LSP index, `survive`; #4186), so
//! the rules cannot drift between them.

/// `p` resolved against `base`, with every symlink in its existing part
/// followed, required to stay under `root`. A value starting with `-` is
/// refused before anything else: it would be a flag, not a path.
pub(crate) fn confined(root: &std::path::Path, base: &std::path::Path, p: &str, key: &str) -> Result<std::path::PathBuf, String> {
    if p.is_empty() || p.starts_with('-') {
        return Err(format!("argument `{}` = `{}` is not a path: a tool argument is never a flag", key, p));
    }
    let joined = base.join(p);
    // The longest prefix that exists is canonicalized (symlinks and `..`
    // resolved by the OS); the rest does not exist, so it holds no symlink,
    // and its `..` are resolved lexically.
    let mut head = joined.as_path();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let real_head = loop {
        if let Ok(c) = head.canonicalize() {
            break c;
        }
        match (head.parent(), head.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                head = parent;
            }
            _ => return Err(format!("argument `{}` = `{}` does not resolve", key, p)),
        }
    };
    let mut real = real_head;
    for part in rest.iter().rev() {
        if part == ".." {
            real.pop();
        } else if part != "." {
            real.push(part);
        }
    }
    if !real.starts_with(root) {
        return Err(format!(
            "argument `{}` = `{}` is outside the project root {} — a tool serving a project only reads that project",
            key,
            p,
            root.display()
        ));
    }
    Ok(real)
}

/// A directory a sweep over a project never enters: version control and
/// other dot-directories, build output (`target`) and vendored dependencies
/// (`node_modules`). `fmt` and the LSP index apply it to what a sweep FINDS,
/// never to its root, so a directory the user names is still walked.
pub(crate) fn is_skipped_dir(name: &str) -> bool {
    (name.starts_with('.') && name != "." && name != "..") || name == "target" || name == "node_modules"
}
