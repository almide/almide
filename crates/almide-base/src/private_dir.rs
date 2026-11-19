//! A fresh directory only this user can enter, for scratch files a tool hands
//! to another process (#4853). A predictable name in a shared temp directory
//! (`/tmp/almide-build-<pid>.wasm`) lets another local user plant a symlink
//! there first, so a write clobbers its target, or swap the file between the
//! write and the reader's read. A name that already exists — whatever it is —
//! is never reused: the directory is CREATED (never opened), mode 0700, and
//! the files inside it are written there.
use std::path::{Path, PathBuf};

/// A new private directory under `base`, named `<stem>-<pid>-<seed>-<n>`;
/// every name that already exists is skipped, never entered or followed.
pub fn private_dir_in(base: &Path, stem: &str, seed: u128) -> std::io::Result<PathBuf> {
    for attempt in 0..64u32 {
        let dir = base.join(format!("{stem}-{}-{seed}-{attempt}", std::process::id()));
        let mut b = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
        match b.create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("no free private directory name for {stem}")))
}

/// [`private_dir_in`] the system temp directory, seeded by the clock.
pub fn private_scratch_dir(stem: &str) -> std::io::Result<PathBuf> {
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    private_dir_in(&std::env::temp_dir(), stem, seed)
}
