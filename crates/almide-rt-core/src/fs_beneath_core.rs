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
// parent directory (a per-segment walk when the kernel has no openat2), on
// macOS a per-segment `openat(dir, seg, O_NOFOLLOW | O_DIRECTORY)` walk, on
// Windows a per-segment NtCreateFile relative to the held directory handle
// with FILE_OPEN_REPARSE_POINT and the opened entry's reparse attribute
// refused (#4293), then the last segment opened, removed or listed from its
// parent's descriptor the same way. A symlink (on Windows any reparse point:
// a symlink, a junction) met anywhere answers `Escape`; a
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
// On a platform without the descriptor calls (fs_beneath_sys.rs: Linux,
// 64- and 32-bit, macOS and Windows have them) the same functions refuse
// every call below the root ([`fs_beneath_unsupported`]) instead of checking
// every prefix and then making the plain call — the pre-#4084 check, which a
// concurrent rewrite could race. Fail-closed: a program there gets an err,
// never an unbounded path.
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

/// Whether this build has the descriptor calls a rooted or scoped fs call
/// is made with (fs_beneath_sys.rs): Linux (64- and 32-bit), macOS and
/// Windows (#4293). Where it is
/// false those calls are refused, and the toolchain refuses
/// `[permissions] fs` before a build ([`fs_scope_platform_check`]).
pub const FS_BENEATH_SUPPORTED: bool = cfg!(any(target_os = "linux", target_os = "macos", windows));

/// The refusal's text where [`FS_BENEATH_SUPPORTED`] is false.
pub const FS_BENEATH_UNSUPPORTED: &str = "fs: a rooted or [permissions] fs call needs descriptor-relative opens (Linux, macOS, Windows), which this platform's build does not have, so it is refused rather than checked by path";

/// The refusal a rooted or scoped call answers where the platform has no
/// descriptor calls: `Unsupported`, with [`FS_BENEATH_UNSUPPORTED`].
pub fn fs_beneath_unsupported() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Unsupported, FS_BENEATH_UNSUPPORTED)
}

/// The toolchain's build-time half (#4022 on such a platform): `Err` with
/// the reason when `[permissions] fs` is set and this build has no
/// descriptor calls, so the scope is refused before anything runs.
pub fn fs_scope_platform_check(scoped: bool) -> Result<(), String> {
    if scoped && !FS_BENEATH_SUPPORTED {
        return Err(format!(
            "`[permissions] fs` cannot be enforced on this platform ({}-bit {}): {}. Remove the key to run unbounded, or build on Linux, macOS or Windows.",
            usize::BITS,
            std::env::consts::OS,
            FS_BENEATH_UNSUPPORTED,
        ));
    }
    Ok(())
}

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
/// segment (the Almide side never sends one), or a segment that is not ONE
/// plain name on this OS.
///
/// Two guards hold on a platform such as Windows, and both stay:
/// - #4301: a segment must be exactly one `Component::Normal`. On Windows
///   `C:` is a prefix and `\` a separator, and `PathBuf::push` of a prefixed
///   or rooted segment REPLACES the path, so `C:/elsewhere` would leave the
///   root (the Almide side only knows `/`). The check runs here, before any
///   platform half, so it holds wherever this file is built — including a
///   descriptor-relative Windows half. On Windows a segment holding `:` (a
///   drive or an alternate data stream: `a.txt:s` is one `Normal`
///   component), a control character or a Win32 wildcard is refused too,
///   the same names the Windows half's `nt_name` refuses (#4293).
/// - Where the build has no descriptor calls (neither Linux, macOS nor
///   Windows), the platform half refuses every call below the root
///   ([`fs_beneath_unsupported`]) instead of checking and then calling.
fn fs_beneath_segments(rel: &str) -> Result<Vec<&str>, FsBeneathError> {
    if rel.is_empty() {
        return Ok(Vec::new());
    }
    let segs: Vec<&str> = rel.split('/').collect();
    if segs.iter().any(|s| s.is_empty() || *s == "." || *s == ".." || !fs_beneath_segment_is_one_name(s)) {
        return Err(FsBeneathError::Escape);
    }
    Ok(segs)
}

/// #4301: `seg` is exactly one `Component::Normal` equal to itself on this
/// OS — no drive prefix, no root, no separator other than `/`, and on
/// Windows no stream colon, control character or wildcard.
fn fs_beneath_segment_is_one_name(seg: &str) -> bool {
    let mut cs = std::path::Path::new(seg).components();
    let one = matches!(
        (cs.next(), cs.next()),
        (Some(std::path::Component::Normal(n)), None) if n == std::ffi::OsStr::new(seg)
    );
    one && !(cfg!(windows) && seg.chars().any(|c| c < ' ' || ":*?\"<>|".contains(c)))
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

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod fs_beneath_imp {
    type OwnedFd = super::almide_fs_beneath_sys::Handle;
    type FsBeneathError = super::FsBeneathError;

    /// A failed open of `name` under `dir`: `Escape` when `name` is a
    /// symlink, the open's own error otherwise.
    pub fn classify(dir: &OwnedFd, name: &str, e: std::io::Error) -> FsBeneathError {
        match super::almide_fs_beneath_sys::symlink_at(dir, name) {
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
            match super::almide_fs_beneath_sys::openat2_dir(root, &parents.join("/")) {
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
            cur = Some(match super::almide_fs_beneath_sys::open_at(at, seg, super::almide_fs_beneath_sys::Open::Dir) {
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
        let fd = super::almide_fs_beneath_sys::open_at(&p, last, super::almide_fs_beneath_sys::Open::Meta).map_err(|e| classify(&p, last, e))?;
        super::fs_read_bounded_file(&mut std::fs::File::from(fd), super::FS_READ_MAX_BYTES).map_err(FsBeneathError::Io)
    }

    pub fn write(dir: &str, parents: &[&str], last: &str, data: &[u8]) -> Result<(), FsBeneathError> {
        let p = parent(dir, parents, false, &mut |_| {})?;
        let fd = super::almide_fs_beneath_sys::open_at(&p, last, super::almide_fs_beneath_sys::Open::WriteTrunc).map_err(|e| classify(&p, last, e))?;
        std::io::Write::write_all(&mut std::fs::File::from(fd), data).map_err(FsBeneathError::Io)
    }

    pub fn list(dir: &str, segs: &[&str]) -> Result<Vec<String>, FsBeneathError> {
        let fd = match segs.split_last() {
            None => super::almide_sfs::open_dir(dir).map_err(FsBeneathError::Io)?,
            Some((last, parents)) => {
                let p = parent(dir, parents, false, &mut |_| {})?;
                super::almide_fs_beneath_sys::open_at(&p, last, super::almide_fs_beneath_sys::Open::Dir).map_err(|e| classify(&p, last, e))?
            }
        };
        let names = super::almide_fs_beneath_sys::list_fd(fd).map_err(FsBeneathError::Io)?;
        Ok(names.iter().map(|n| n.to_string_lossy().into_owned()).collect())
    }

    pub fn exists(dir: &str, parents: &[&str], last: &str) -> Result<bool, FsBeneathError> {
        let p = match parent(dir, parents, false, &mut |_| {}) {
            Ok(p) => p,
            Err(FsBeneathError::Escape) => return Err(FsBeneathError::Escape),
            Err(FsBeneathError::Io(_)) => return Ok(false),
        };
        match super::almide_fs_beneath_sys::symlink_at(&p, last) {
            Ok(true) => Err(FsBeneathError::Escape),
            Ok(false) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    pub fn remove(dir: &str, parents: &[&str], last: &str) -> Result<(), FsBeneathError> {
        let p = parent(dir, parents, false, &mut |_| {})?;
        if let Ok(true) = super::almide_fs_beneath_sys::symlink_at(&p, last) {
            return Err(FsBeneathError::Escape);
        }
        let is_dir = super::almide_fs_beneath_sys::open_at(&p, last, super::almide_fs_beneath_sys::Open::Dir).is_ok();
        super::almide_fs_beneath_sys::unlink_at(&p, last, is_dir).map_err(FsBeneathError::Io)
    }
}

/// Elsewhere: no descriptor calls, so every call below the root is refused
/// (fail-closed; see the file header).
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod fs_beneath_imp {
    type FsBeneathError = super::FsBeneathError;

    fn refused<T>() -> Result<T, FsBeneathError> {
        Err(FsBeneathError::Io(super::fs_beneath_unsupported()))
    }

    pub fn read(_dir: &str, _parents: &[&str], _last: &str, _walk_only: bool, _hook: &mut dyn FnMut(usize)) -> Result<Vec<u8>, FsBeneathError> {
        refused()
    }

    pub fn write(_dir: &str, _parents: &[&str], _last: &str, _data: &[u8]) -> Result<(), FsBeneathError> {
        refused()
    }

    pub fn list(_dir: &str, _segs: &[&str]) -> Result<Vec<String>, FsBeneathError> {
        refused()
    }

    pub fn exists(_dir: &str, _parents: &[&str], _last: &str) -> Result<bool, FsBeneathError> {
        refused()
    }

    pub fn remove(_dir: &str, _parents: &[&str], _last: &str) -> Result<(), FsBeneathError> {
        refused()
    }
}
