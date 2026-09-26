//! The one read path for `.almd` sources, and the overlay `almide survive`
//! (#2147) uses to judge an edit WITHOUT writing it.
//!
//! `almide survive FILE --with EDIT` must answer "what would check / test /
//! the contract fixtures say if FILE held this text?" while the file on disk
//! stays byte-for-byte what it was. It does so by running the ordinary
//! commands with the overlay armed: every compiler read of FILE's source
//! returns the proposed text instead of the disk bytes; every other file is
//! read from disk. The overlay is armed either in-process ([`set`]) or, for a
//! child `almide` process, through two registered switches —
//! `ALMIDE_SURVIVE_OVERLAY` (the path whose reads are replaced) and
//! `ALMIDE_SURVIVE_OVERLAY_TEXT` (a file outside the tree holding the text).
//!
//! **Every source read goes through [`read_to_string`].** A read that bypassed
//! it would judge the before-text on the after-leg and report a stale verdict
//! as a survival — the one wrong answer this command cannot afford. The sites
//! are the entry parse (`compile_driver::parse_file`, `check --json`'s own
//! parse), module resolution (`resolve::parse_module_source`), test discovery
//! and the test report's name recovery.
//!
//! Paths are compared by their canonical form, so `./a.almd`, `a.almd` and
//! the absolute spelling are one file.

use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

/// The file whose reads are replaced, and the text they return.
#[derive(Clone, Debug)]
struct Overlay {
    target: PathBuf,
    text: String,
}

fn slot() -> &'static RwLock<Option<Overlay>> {
    static SLOT: OnceLock<RwLock<Option<Overlay>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(from_env()))
}

/// The overlay a parent `almide survive` armed for this child process.
///
/// Both switches or neither: a target without readable text is a broken
/// hand-off, and reading the disk file instead would silently judge the wrong
/// program — so it aborts rather than guessing.
fn from_env() -> Option<Overlay> {
    let target = almide_base::env::var("ALMIDE_SURVIVE_OVERLAY")?;
    let text_path = almide_base::env::var("ALMIDE_SURVIVE_OVERLAY_TEXT").unwrap_or_else(|| {
        eprintln!("ALMIDE_SURVIVE_OVERLAY is set but ALMIDE_SURVIVE_OVERLAY_TEXT is not — refusing to read the disk file in its place");
        std::process::exit(2);
    });
    let text = std::fs::read_to_string(&text_path).unwrap_or_else(|e| {
        eprintln!("ALMIDE_SURVIVE_OVERLAY_TEXT: cannot read {}: {}", text_path, e);
        std::process::exit(2);
    });
    Some(Overlay { target: canonical(Path::new(&target)), text })
}

/// The canonical spelling of `path`; the lexical absolute path when the file
/// does not exist (a canonicalize failure must not make two spellings of one
/// file compare unequal when they are lexically the same).
pub fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map(|d| d.join(path)).unwrap_or_else(|_| path.to_path_buf())
        }
    })
}

/// Arm the overlay in this process: reads of `target` return `text`.
pub fn set(target: &Path, text: String) {
    let mut g = slot().write().unwrap_or_else(|e| e.into_inner());
    *g = Some(Overlay { target: canonical(target), text });
}

/// Disarm the overlay in this process.
pub fn clear() {
    let mut g = slot().write().unwrap_or_else(|e| e.into_inner());
    *g = None;
}

/// The overlaid text for `path`, when the overlay is armed on exactly it.
pub fn overlaid(path: &Path) -> Option<String> {
    let g = slot().read().unwrap_or_else(|e| e.into_inner());
    let ov = g.as_ref()?;
    (canonical(path) == ov.target).then(|| ov.text.clone())
}

/// `std::fs::read_to_string`, overlay-aware. THE read path for `.almd` source.
pub fn read_to_string(path: impl AsRef<Path>) -> std::io::Result<String> {
    let path = path.as_ref();
    match overlaid(path) {
        Some(text) => Ok(text),
        None => std::fs::read_to_string(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_overlay_replaces_exactly_its_target_and_clears() {
        let dir = std::env::temp_dir().join(format!("almide-overlay-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.almd");
        let b = dir.join("b.almd");
        std::fs::write(&a, "disk a").unwrap();
        std::fs::write(&b, "disk b").unwrap();
        set(&a, "proposed a".to_string());
        // A second spelling of the same file is the same file.
        let dotted = dir.join(".").join("a.almd");
        assert_eq!(read_to_string(&dotted).unwrap(), "proposed a");
        assert_eq!(read_to_string(&b).unwrap(), "disk b");
        clear();
        assert_eq!(read_to_string(&a).unwrap(), "disk a");
        // The disk file was never touched.
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "disk a");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
