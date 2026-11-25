//! Where a build script may read from (#3361) — shared, byte-identical, by the
//! build scripts of almide-types, almide-codegen, almide-egg-lab and
//! almide-interp. The canonical copy is `crates/almide-types/buildscript/
//! in_checkout.rs`; `scripts/check-build-inputs-in-package.sh` refuses a copy
//! that drifts from it. (A copy and not a `#[path]` into a sibling crate: the
//! point of this file is that each crate builds from its own package alone.)
//!
//! Each of those crates embeds tables derived from files OUTSIDE its package
//! (`stdlib/*.almd`, `runtime/rs/src/*.rs`). The derived files are committed
//! under the crate's own `src/generated/`, and the crate compiles from them —
//! so a crate laid out on its own (`cargo vendor`, `cargo package`, Flatpak,
//! Nix: `../..` is then the CONSUMER's directory) builds from what it ships.
//! Inside an almide checkout the build script regenerates them from the
//! sources, so editing `stdlib/` or `runtime/rs/src/` still takes effect on the
//! next build; the regenerated file shows up in `git status` and is committed
//! with the edit, and the CI gate fails a commit that forgot to.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// The almide checkout this crate is being built inside, or `None` when the
/// crate stands alone. Decided by STRUCTURE — `../../crates/<this package>/
/// Cargo.toml` must be this very manifest — not by "does `../../stdlib`
/// exist": a vendored crate's `../..` is the consumer's project root, and an
/// Almide project may well have a `stdlib/` of its own.
pub fn checkout_root(manifest_dir: &Path) -> Option<PathBuf> {
    let root = manifest_dir.join("../..");
    let name = std::env::var("CARGO_PKG_NAME").ok()?;
    let candidate = std::fs::canonicalize(root.join("crates").join(name).join("Cargo.toml")).ok()?;
    let this = std::fs::canonicalize(manifest_dir.join("Cargo.toml")).ok()?;
    (candidate == this).then_some(root)
}

/// The crate's own manifest directory.
pub fn manifest_dir() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"))
}

/// Write `content` to `path` only when it differs, through a temp file and a
/// rename: an unchanged table keeps its mtime (no recompile of the crate), and
/// two builds of one checkout (two target dirs) never see a half-written file.
pub fn write_if_changed(path: &Path, content: &str) {
    if std::fs::read_to_string(path).ok().as_deref() == Some(content) {
        return;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, content).unwrap_or_else(|e| panic!("write {}: {e}", tmp.display()));
    std::fs::rename(&tmp, path).unwrap_or_else(|e| panic!("rename {} -> {}: {e}", tmp.display(), path.display()));
}
