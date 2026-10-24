//! What a sweep over a project skips. Shared by every directory walker
//! (`fmt`, `test`, the LSP index, `survive`; #4186), so the rule cannot drift
//! between them.

/// A directory a sweep over a project never enters: version control and
/// other dot-directories, build output (`target`) and vendored dependencies
/// (`node_modules`). `fmt` and the LSP index apply it to what a sweep FINDS,
/// never to its root, so a directory the user names is still walked.
pub(crate) fn is_skipped_dir(name: &str) -> bool {
    (name.starts_with('.') && name != "." && name != "..") || name == "target" || name == "node_modules"
}
