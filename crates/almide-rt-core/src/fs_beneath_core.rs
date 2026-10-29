// fs_beneath_core (#4084): the rooted fs family's calls (`fs.root_*`, C-392)
// made by descriptor, so the refusal of a symlink is part of the call itself
// and a tree rewritten between a check and a use cannot steer it out of its
// root. ONE source for two consumers:
//
// - runtime/rs/src/fs.rs `include!`s this file, and the native
//   `almide_rt_fs_root_*` intrinsics are thin wrappers over these functions;
// - almide-wasm-run links this crate and serves the embedded host's rooted
//   ops 120..=125 with the same functions.
//
// The root directory is opened by its path (its own symlinks followed: the
// root is the program's choice, made at `fs.open_root`); everything under it
// is reached segment by segment from that descriptor, never following a
// symlink: on Linux one openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS) for the
// parent directory (a per-segment walk when the kernel has no openat2),
// elsewhere a per-segment `openat(dir, seg, O_NOFOLLOW | O_DIRECTORY)` walk,
// then the last segment opened, removed or listed from its parent's
// descriptor with O_NOFOLLOW. A symlink met anywhere answers `Escape`; a
// directory swapped for a symlink after the walk passed it is not followed,
// because the walk already holds the directory itself, not its name.
//
// `rel` is the segment list the Almide side normalized (`.` and `..` applied,
// no leading `/`), joined by `/`; a `..`, `.` or empty segment is refused here
// too, so the host never trusts its caller for the bound. Errors other than
// the escape are the plain call's own io errors, which each consumer spells
// with its `io_err` over [`fs_beneath_joined`] — the bytes the plain call on
// `<root>/<rel>` answers.
//
// On a platform without the descriptor calls (fs_beneath_sys.rs: 64-bit
// Linux and macOS have them) the same functions check every prefix for a
// symlink and then make the plain call — the pre-#4084 check, which a
// concurrent rewrite can race.
//
// Under `[permissions] fs` (fs_scoped_core.rs, included below) the root
// itself is opened beneath the granted directory it falls under, so a rooted
// call cannot be steered out of the grant through the root's own path either;
// without the key it is opened by its path, as before.
//
// Splice discipline (lib.rs): no `crate::` paths, no `mod`, no `use` lines.

include!("fs_beneath_sys.rs");
include!("fs_scoped_core.rs");

/// The refusal a rooted call answers for a symlink on its way; the Almide
/// side completes it with the path the program wrote (`fs: path escapes
/// root: <path>`).
pub const FS_BENEATH_ESCAPE: &str = "fs: path escapes root";

/// Why a rooted call failed.
#[derive(Debug)]
pub enum FsBeneathError {
    /// A symlink, `..` or absolute step on the way: the path leaves the root.
    Escape,
    /// The plain call's own failure on `<root>/<rel>`.
    Io(std::io::Error),
}

/// `<root>/<rel>`, or the root itself for an empty `rel`: the operand the
/// plain call's error names.
pub fn fs_beneath_joined(dir: &str, rel: &str) -> String {
    if rel.is_empty() { dir.to_string() } else { format!("{dir}/{rel}") }
}

/// `rel`'s segments, or `Escape` for an absolute path or a `.` / `..` / empty
/// segment (the Almide side never sends one).
fn fs_beneath_segments(rel: &str) -> Result<Vec<&str>, FsBeneathError> {
    if rel.is_empty() {
        return Ok(Vec::new());
    }
    let segs: Vec<&str> = rel.split('/').collect();
    if segs.iter().any(|s| s.is_empty() || *s == "." || *s == "..") {
        return Err(FsBeneathError::Escape);
    }
    Ok(segs)
}

/// The whole file `<root>/<rel>`.
pub fn fs_beneath_read(dir: &str, rel: &str) -> Result<Vec<u8>, FsBeneathError> {
    fs_beneath_read_with(dir, rel, false, &mut |_| {})
}

/// `<root>/<rel>` as text: the bytes, or the plain reader's invalid-UTF-8 error.
pub fn fs_beneath_read_text(dir: &str, rel: &str) -> Result<String, FsBeneathError> {
    let bytes = fs_beneath_read(dir, rel)?;
    String::from_utf8(bytes).map_err(|_| {
        FsBeneathError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, "stream did not contain valid UTF-8"))
    })
}

/// Write `data` to `<root>/<rel>`, created or truncated.
pub fn fs_beneath_write(dir: &str, rel: &str, data: &[u8]) -> Result<(), FsBeneathError> {
    let segs = fs_beneath_segments(rel)?;
    let Some((last, parents)) = segs.split_last() else {
        return almide_sfs::write(dir, data).map_err(FsBeneathError::Io);
    };
    fs_beneath_imp::write(dir, parents, last, data)
}

/// The sorted entry names of the directory `<root>/<rel>`.
pub fn fs_beneath_list(dir: &str, rel: &str) -> Result<Vec<String>, FsBeneathError> {
    let segs = fs_beneath_segments(rel)?;
    let mut names = fs_beneath_imp::list(dir, &segs)?;
    names.sort_unstable();
    Ok(names)
}

/// Whether `<root>/<rel>` exists; `Escape` when a symlink is on the way (the
/// last segment included).
pub fn fs_beneath_exists(dir: &str, rel: &str) -> Result<bool, FsBeneathError> {
    let segs = fs_beneath_segments(rel)?;
    let Some((last, parents)) = segs.split_last() else {
        return Ok(almide_sfs::exists(dir));
    };
    fs_beneath_imp::exists(dir, parents, last)
}

/// Remove the file or empty directory `<root>/<rel>`.
pub fn fs_beneath_remove(dir: &str, rel: &str) -> Result<(), FsBeneathError> {
    let segs = fs_beneath_segments(rel)?;
    let Some((last, parents)) = segs.split_last() else {
        let r = if almide_sfs::is_dir(dir) { almide_sfs::remove_dir(dir) } else { almide_sfs::remove_file(dir) };
        return r.map_err(FsBeneathError::Io);
    };
    fs_beneath_imp::remove(dir, parents, last)
}

/// [`fs_beneath_read`] with the walk exposed for the race tests
/// (tests/fs_beneath_core.rs): `walk_only` skips openat2, and `hook(i)` runs
/// after the walk holds the directory of segment `i` (`0` = the root) and
/// before it opens the next one — the window a concurrent rewrite would use.
#[doc(hidden)]
pub fn fs_beneath_read_with(dir: &str, rel: &str, walk_only: bool, hook: &mut dyn FnMut(usize)) -> Result<Vec<u8>, FsBeneathError> {
    let segs = fs_beneath_segments(rel)?;
    let Some((last, parents)) = segs.split_last() else {
        return almide_sfs::read_bounded(dir).map_err(FsBeneathError::Io);
    };
    fs_beneath_imp::read(dir, parents, last, walk_only, hook)
}

#[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
mod fs_beneath_imp {
    type OwnedFd = std::os::fd::OwnedFd;
    type FsBeneathError = super::FsBeneathError;

    /// A failed open of `name` under `dir`: `Escape` when `name` is a
    /// symlink, the open's own error otherwise.
    pub fn classify(dir: &OwnedFd, name: &str, e: std::io::Error) -> FsBeneathError {
        match super::almide_fs_beneath_sys::symlink_at(std::os::fd::AsFd::as_fd(&dir), name) {
            Ok(true) => FsBeneathError::Escape,
            _ => FsBeneathError::Io(e),
        }
    }

    /// The directory `<root>/<parents...>`, reached without following any
    /// symlink below the root.
    fn parent(dir: &str, parents: &[&str], walk_only: bool, hook: &mut dyn FnMut(usize)) -> Result<OwnedFd, FsBeneathError> {
        let root = super::almide_sfs::open_dir(dir).map_err(FsBeneathError::Io)?;
        if parents.is_empty() {
            hook(0);
            return Ok(root);
        }
        parent_from(&root, parents, walk_only, hook)
    }

    /// The directory `<root>/<parents...>` from the held root descriptor,
    /// without following any symlink below it (the root itself is
    /// duplicated for an empty `parents`).
    pub fn parent_from(root: &OwnedFd, parents: &[&str], walk_only: bool, hook: &mut dyn FnMut(usize)) -> Result<OwnedFd, FsBeneathError> {
        if parents.is_empty() {
            hook(0);
            return root.try_clone().map_err(FsBeneathError::Io);
        }
        #[cfg(target_os = "linux")]
        if !walk_only {
            match super::almide_fs_beneath_sys::openat2_dir(std::os::fd::AsFd::as_fd(root), &parents.join("/")) {
                Some(Ok(fd)) => {
                    hook(parents.len());
                    return Ok(fd);
                }
                // ELOOP 40: a symlink on the way; EXDEV 18: a step out.
                Some(Err(e)) if matches!(e.raw_os_error(), Some(40 | 18)) => return Err(FsBeneathError::Escape),
                // Any other failure is told apart by the walk below: a
                // symlink as the LAST segment answers ENOTDIR under
                // O_NOFOLLOW | O_DIRECTORY before RESOLVE_NO_SYMLINKS'
                // ELOOP, and the walk's per-segment check names it an
                // escape; a missing or non-directory segment fails there
                // with the plain call's own errno.
                Some(Err(_)) | None => {}
            }
        }
        let _ = walk_only;
        let mut cur: Option<OwnedFd> = None;
        for (i, seg) in parents.iter().enumerate() {
            hook(i);
            let at = cur.as_ref().unwrap_or(root);
            cur = Some(match super::almide_fs_beneath_sys::open_at(std::os::fd::AsFd::as_fd(at), seg, super::almide_fs_beneath_sys::Open::Dir) {
                Ok(next) => next,
                Err(e) => return Err(classify(at, seg, e)),
            });
        }
        hook(parents.len());
        cur.ok_or(FsBeneathError::Escape)
    }

    pub fn read(dir: &str, parents: &[&str], last: &str, walk_only: bool, hook: &mut dyn FnMut(usize)) -> Result<Vec<u8>, FsBeneathError> {
        let p = parent(dir, parents, walk_only, hook)?;
        // `Meta` is the O_NONBLOCK read-only open: a FIFO does not block it,
        // and the bounded read refuses it by its descriptor (#4201).
        let fd = super::almide_fs_beneath_sys::open_at(std::os::fd::AsFd::as_fd(&p), last, super::almide_fs_beneath_sys::Open::Meta).map_err(|e| classify(&p, last, e))?;
        super::fs_read_bounded_file(&mut std::fs::File::from(fd), super::FS_READ_MAX_BYTES).map_err(FsBeneathError::Io)
    }

    pub fn write(dir: &str, parents: &[&str], last: &str, data: &[u8]) -> Result<(), FsBeneathError> {
        let p = parent(dir, parents, false, &mut |_| {})?;
        let fd = super::almide_fs_beneath_sys::open_at(std::os::fd::AsFd::as_fd(&p), last, super::almide_fs_beneath_sys::Open::WriteTrunc).map_err(|e| classify(&p, last, e))?;
        std::io::Write::write_all(&mut std::fs::File::from(fd), data).map_err(FsBeneathError::Io)
    }

    pub fn list(dir: &str, segs: &[&str]) -> Result<Vec<String>, FsBeneathError> {
        let fd = match segs.split_last() {
            None => super::almide_sfs::open_dir(dir).map_err(FsBeneathError::Io)?,
            Some((last, parents)) => {
                let p = parent(dir, parents, false, &mut |_| {})?;
                super::almide_fs_beneath_sys::open_at(std::os::fd::AsFd::as_fd(&p), last, super::almide_fs_beneath_sys::Open::Dir).map_err(|e| classify(&p, last, e))?
            }
        };
        let names = super::almide_fs_beneath_sys::list_fd(fd).map_err(FsBeneathError::Io)?;
        Ok(names.iter().map(|n| String::from_utf8_lossy(n).to_string()).collect())
    }

    pub fn exists(dir: &str, parents: &[&str], last: &str) -> Result<bool, FsBeneathError> {
        let p = match parent(dir, parents, false, &mut |_| {}) {
            Ok(p) => p,
            Err(FsBeneathError::Escape) => return Err(FsBeneathError::Escape),
            Err(FsBeneathError::Io(_)) => return Ok(false),
        };
        match super::almide_fs_beneath_sys::symlink_at(std::os::fd::AsFd::as_fd(&p), last) {
            Ok(true) => Err(FsBeneathError::Escape),
            Ok(false) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    pub fn remove(dir: &str, parents: &[&str], last: &str) -> Result<(), FsBeneathError> {
        let p = parent(dir, parents, false, &mut |_| {})?;
        if let Ok(true) = super::almide_fs_beneath_sys::symlink_at(std::os::fd::AsFd::as_fd(&p), last) {
            return Err(FsBeneathError::Escape);
        }
        let is_dir = super::almide_fs_beneath_sys::open_at(std::os::fd::AsFd::as_fd(&p), last, super::almide_fs_beneath_sys::Open::Dir).is_ok();
        super::almide_fs_beneath_sys::unlink_at(std::os::fd::AsFd::as_fd(&p), last, is_dir).map_err(FsBeneathError::Io)
    }
}

/// The path-based fallback: every prefix checked for a symlink, then the
/// plain call (racy — see the file header).
#[cfg(not(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64")))]
mod fs_beneath_imp {
    type FsBeneathError = super::FsBeneathError;

    fn checked(dir: &str, segs: &[&str]) -> Result<std::path::PathBuf, FsBeneathError> {
        let mut p = std::path::PathBuf::from(dir);
        for seg in segs {
            // A segment must be ONE plain name on this OS. On Windows `C:` is a
            // prefix and `\` a separator, and `PathBuf::push` of a prefixed or
            // rooted segment REPLACES the path, so `C:/elsewhere` would leave
            // the root (the Almide side only knows `/`).
            if !segment_is_one_name(seg) {
                return Err(FsBeneathError::Escape);
            }
            p.push(seg);
            if std::fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(FsBeneathError::Escape);
            }
        }
        Ok(p)
    }

    fn segment_is_one_name(seg: &str) -> bool {
        let mut cs = std::path::Path::new(seg).components();
        matches!(
            (cs.next(), cs.next()),
            (Some(std::path::Component::Normal(n)), None) if n == std::ffi::OsStr::new(seg)
        )
    }

    fn all<'a>(parents: &[&'a str], last: &'a str) -> Vec<&'a str> {
        parents.iter().copied().chain(std::iter::once(last)).collect()
    }

    pub fn read(dir: &str, parents: &[&str], last: &str, _walk_only: bool, hook: &mut dyn FnMut(usize)) -> Result<Vec<u8>, FsBeneathError> {
        let p = checked(dir, &all(parents, last))?;
        hook(parents.len());
        super::fs_read_bounded_path(&p, super::FS_READ_MAX_BYTES).map_err(FsBeneathError::Io)
    }

    pub fn write(dir: &str, parents: &[&str], last: &str, data: &[u8]) -> Result<(), FsBeneathError> {
        std::fs::write(checked(dir, &all(parents, last))?, data).map_err(FsBeneathError::Io)
    }

    pub fn list(dir: &str, segs: &[&str]) -> Result<Vec<String>, FsBeneathError> {
        let p = checked(dir, segs)?;
        let mut names = Vec::new();
        for e in std::fs::read_dir(p).map_err(FsBeneathError::Io)? {
            names.push(e.map_err(FsBeneathError::Io)?.file_name().to_string_lossy().to_string());
        }
        Ok(names)
    }

    pub fn exists(dir: &str, parents: &[&str], last: &str) -> Result<bool, FsBeneathError> {
        Ok(checked(dir, &all(parents, last))?.exists())
    }

    pub fn remove(dir: &str, parents: &[&str], last: &str) -> Result<(), FsBeneathError> {
        let p = checked(dir, &all(parents, last))?;
        let r = if p.is_dir() { std::fs::remove_dir(&p) } else { std::fs::remove_file(&p) };
        r.map_err(FsBeneathError::Io)
    }
}
