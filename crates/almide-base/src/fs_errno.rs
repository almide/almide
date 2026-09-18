//! The fs errno vocabulary every leg spells — ONE table (#2206, C-215).
//!
//! An `fs.*` failure reads `fs.read_text("/nope/x"): No such file or
//! directory (os error 2)` on every leg: the native runtime and the embedded
//! host hand the suffix to `std::io::Error`'s `Display`, and the three
//! renderers that cannot — the incumbent WAT's static data, the p3 component
//! shim's message pack, the interp VFS — used to spell it by hand, each from
//! its own list, kept equal by comment. This module is the list. The WAT and
//! the p3 pack are rendered FROM it at emit time, the VFS reads it, and
//! `tests/fs_errno_table.rs` walks every row against the `Display` of the
//! host the suite runs on, so a spelling can only drift by failing a test.
//!
//! Rows are the errnos the wasm hosts can produce whose NUMBER and TEXT are
//! identical on the linux and macos hosts the legs are compared on. An errno
//! that differs between them (`ENOTEMPTY` is 39 on linux and 66 on macos)
//! cannot be spelled statically without breaking C-215 on one host, so it is
//! deliberately not a row: a renderer that cannot spell a code falls back to
//! its generic text, and the gate pins that fallback as a divergence to
//! close, not a row to invent.

/// One errno the legs spell: its WASI preview1 code, its POSIX number and
/// the exact `std::io::Error` `Display` text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsErrno {
    /// The POSIX name (`ENOENT`).
    pub name: &'static str,
    /// The WASI preview1 `errno` value the wasm hosts report.
    pub wasi: u16,
    /// The POSIX `errno` number — the `(os error N)` suffix.
    pub os: i32,
    /// `std::io::Error::from_raw_os_error(os).to_string()`, verbatim.
    pub text: &'static str,
}

const fn row(name: &'static str, wasi: u16, os: i32, text: &'static str) -> FsErrno {
    FsErrno { name, wasi, os, text }
}

pub const ENOENT: FsErrno = row("ENOENT", 44, 2, "No such file or directory (os error 2)");
pub const EACCES: FsErrno = row("EACCES", 2, 13, "Permission denied (os error 13)");
pub const ENOTDIR: FsErrno = row("ENOTDIR", 54, 20, "Not a directory (os error 20)");
pub const EISDIR: FsErrno = row("EISDIR", 31, 21, "Is a directory (os error 21)");
pub const EEXIST: FsErrno = row("EEXIST", 20, 17, "File exists (os error 17)");
pub const EPERM: FsErrno = row("EPERM", 63, 1, "Operation not permitted (os error 1)");
pub const EINVAL: FsErrno = row("EINVAL", 28, 22, "Invalid argument (os error 22)");
pub const EBADF: FsErrno = row("EBADF", 8, 9, "Bad file descriptor (os error 9)");
pub const EIO: FsErrno = row("EIO", 29, 5, "Input/output error (os error 5)");

/// Every row, in the order the renderers lay them out. Append, never insert:
/// the incumbent WAT derives its static-data addresses from this order.
pub const FS_ERRNOS: &[FsErrno] = &[ENOENT, EACCES, ENOTDIR, EISDIR, EEXIST, EPERM, EINVAL, EBADF, EIO];

/// The row for a WASI preview1 errno, if the table spells it.
pub fn by_wasi(errno: u16) -> Option<&'static FsErrno> {
    FS_ERRNOS.iter().find(|r| r.wasi == errno)
}

/// The row for a POSIX errno number, if the table spells it.
pub fn by_os(os: i32) -> Option<&'static FsErrno> {
    FS_ERRNOS.iter().find(|r| r.os == os)
}

/// Rust's own `std::io::ErrorKind::WriteZero` message (`write_all` accepting
/// zero bytes) — a const of the standard library, not an OS string, so it is
/// the same on every host.
pub const WRITE_ZERO_TEXT: &str = "failed to write whole buffer";
/// Rust's own `read_to_string` `InvalidData` message — likewise host-independent.
pub const INVALID_UTF8_TEXT: &str = "stream did not contain valid UTF-8";
