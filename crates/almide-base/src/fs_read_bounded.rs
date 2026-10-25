// fs_read_bounded (#4201): the whole-file reader every leg's `fs.read_*`
// shares — regular files only, at most FS_READ_MAX_BYTES of them.
//
// `std::fs::read` of `/dev/zero`, of a FIFO or of a huge file allocates until
// the process is killed; a `file_size` pre-check cannot defend it (a race,
// and `/dev/zero` reports 0). So the reader:
//
// - opens with O_NONBLOCK (unix), so opening a FIFO with no writer returns
//   at once instead of blocking;
// - fstats the OPENED descriptor, never the path, so a file swapped in after
//   the open is not the one judged; anything that is neither a regular file
//   nor a directory errs `Not a regular file`. A directory keeps the read's
//   own EISDIR (`Is a directory (os error 21)`, C-215), unchanged;
// - reads through `take(cap + 1)` and errs past `cap`, so a file that grows
//   during the read (or a size the stat got wrong: procfs reports 0) is
//   bounded by the bytes actually read, not by the stat.
//
// Both refusals are library errors (no raw errno), so `fs_io_error_text`
// answers their text verbatim: `fs.read_text("/dev/zero"): Not a regular file`
// and `fs.read_bytes("big"): File too large (the read limit is 1073741824
// bytes)` on every leg.
//
// ONE source, three consumers: `almide_base::fs_errno` `include!`s it (the
// texts the wasm renderers spell), and almide-rt-core's fs_scoped_core.rs
// does, whose `almide_sfs::read_bounded` both the native runtime
// (runtime/rs/src/fs.rs, through fs_beneath_core.rs) and the embedded wasm
// host (almide-wasm-run host_fs.rs) call. Splice discipline: no `crate::`
// paths, no `mod`, no `use` lines, every name prefixed `fs_` / `Fs` / `FS_`.

/// The most bytes one whole-file read returns: 1 GiB. Past it the read errs
/// [`FS_READ_TOO_LARGE_TEXT`]; the streaming readers (`fs.fold_lines`,
/// `fs.for_each_line`) are the way to walk a larger file.
pub const FS_READ_MAX_BYTES: u64 = 1_073_741_824;

/// The text a read of a device, FIFO or socket answers.
pub const FS_READ_NOT_REGULAR_TEXT: &str = "Not a regular file";

/// The text a read past [`FS_READ_MAX_BYTES`] answers.
pub const FS_READ_TOO_LARGE_TEXT: &str = "File too large (the read limit is 1073741824 bytes)";

/// Whether `e` is one of this reader's two refusals (not an OS error).
pub fn fs_read_bounded_refusal(e: &std::io::Error) -> bool {
    e.raw_os_error().is_none() && matches!(e.to_string().as_str(), FS_READ_NOT_REGULAR_TEXT | FS_READ_TOO_LARGE_TEXT)
}

/// The refusal of an opened entry that is neither a regular file nor a
/// directory (a directory's read errs EISDIR by itself).
pub fn fs_read_bounded_kind(is_file: bool, is_dir: bool) -> std::io::Result<()> {
    if is_file || is_dir {
        Ok(())
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, FS_READ_NOT_REGULAR_TEXT))
    }
}

/// At most `cap` bytes of `r`; more than `cap` errs [`FS_READ_TOO_LARGE_TEXT`].
pub fn fs_read_bounded_from(r: &mut dyn std::io::Read, cap: u64) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(r, cap.saturating_add(1)), &mut out)?;
    if out.len() as u64 > cap {
        return Err(fs_read_bounded_too_large());
    }
    Ok(out)
}

/// The past-the-cap refusal.
pub fn fs_read_bounded_too_large() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::FileTooLarge, FS_READ_TOO_LARGE_TEXT)
}

/// The bounded read of an opened file: its descriptor's kind judged, then at
/// most `cap` bytes.
pub fn fs_read_bounded_file(f: &mut std::fs::File, cap: u64) -> std::io::Result<Vec<u8>> {
    let m = f.metadata()?;
    fs_read_bounded_kind(m.is_file(), m.is_dir())?;
    // A stat already past the cap errs before a byte is read (no 1 GiB
    // allocation to learn it); the `take` below stays the authority, since a
    // stat can under-report (procfs, a file growing during the read).
    if m.is_file() && m.len() > cap {
        return Err(fs_read_bounded_too_large());
    }
    fs_read_bounded_from(f, cap)
}

/// O_NONBLOCK where the platform's value is known; 0 elsewhere (the open
/// then blocks on a FIFO as before, and the fstat still refuses it once a
/// writer arrives). 0 under Miri too, whose `open` shim refuses flags it does
/// not model: the generated crates the miri-generated sweep interprets keep
/// running their fs reads (a FIFO is not among its fixtures).
#[cfg(all(not(miri), target_os = "linux", not(any(target_arch = "mips", target_arch = "mips64", target_arch = "sparc", target_arch = "sparc64"))))]
pub const FS_READ_O_NONBLOCK: i32 = 0o4000;
#[cfg(all(not(miri), any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd", target_os = "dragonfly")))]
pub const FS_READ_O_NONBLOCK: i32 = 0x4;
#[cfg(any(
    miri,
    not(any(
        all(target_os = "linux", not(any(target_arch = "mips", target_arch = "mips64", target_arch = "sparc", target_arch = "sparc64"))),
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))
))]
pub const FS_READ_O_NONBLOCK: i32 = 0;

/// The read-only open of `p` the bounded reader makes: O_NONBLOCK, so a FIFO
/// does not block the open.
pub fn fs_read_bounded_open(p: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.read(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut o, FS_READ_O_NONBLOCK);
    o.open(p)
}

/// The whole regular file at `p`, at most `cap` bytes (the unscoped read).
pub fn fs_read_bounded_path(p: &std::path::Path, cap: u64) -> std::io::Result<Vec<u8>> {
    fs_read_bounded_file(&mut fs_read_bounded_open(p)?, cap)
}

/// [`fs_read_bounded_path`] as UTF-8 text: the bytes, or `read_to_string`'s
/// own invalid-UTF-8 error.
pub fn fs_read_bounded_text(bytes: Vec<u8>) -> std::io::Result<String> {
    String::from_utf8(bytes).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "stream did not contain valid UTF-8"))
}
