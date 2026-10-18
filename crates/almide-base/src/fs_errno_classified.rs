// fs_errno_classified (C-273, owner ruling 2026-10-17): the fs errnos whose
// NUMBER or TEXT differs between the hosts the legs are compared on, each
// CLASSIFIED to one errno-free text every leg answers — the shape
// almide-rt-core's net_error_core gave socket errors. `fs.remove` of a
// non-empty directory reads `fs.remove("d"): Directory not empty` natively on
// linux and macos, on the embedded host, in the stock p1 artifact, in the p3
// component and in the interp VFS alike.
//
// The text is the strerror wording every compared host shares, without the
// `(os error N)` suffix (the number is what differs). The native legs
// classify on `std::io::ErrorKind`, never on a platform's number; the wasm
// renderers key the same rows on the WASI preview1 code.
//
// ONE source, two consumers: `almide_base::fs_errno` `include!`s it, and so
// does `runtime/rs/src/fs.rs` (the embed resolver inlines it into the
// generated runtime crate). Splice discipline: no `crate::` paths, no `mod`,
// no `use` lines, every name prefixed `fs_` / `Fs` / `FS_`.

/// One classified errno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsClassifiedErrno {
    /// The POSIX name (`ENOTEMPTY`).
    pub name: &'static str,
    /// The WASI preview1 `errno` value the wasm hosts report.
    pub wasi: u16,
    /// The kind `std` decodes the errno to — what the native legs match on.
    pub kind: std::io::ErrorKind,
    /// The POSIX number on linux and on macos (documentation and the table
    /// test; no leg classifies by it).
    pub linux: i32,
    pub macos: i32,
    /// The text every leg answers after `fs.<call>(<operand>): `.
    pub text: &'static str,
}

pub const FS_ENOTEMPTY: FsClassifiedErrno = FsClassifiedErrno {
    name: "ENOTEMPTY",
    wasi: 55,
    kind: std::io::ErrorKind::DirectoryNotEmpty,
    linux: 39,
    macos: 66,
    text: "Directory not empty",
};
pub const FS_ENAMETOOLONG: FsClassifiedErrno = FsClassifiedErrno {
    name: "ENAMETOOLONG",
    wasi: 37,
    kind: std::io::ErrorKind::InvalidFilename,
    linux: 36,
    macos: 63,
    text: "File name too long",
};
/// Linux spells it `Invalid cross-device link`, macos `Cross-device link`:
/// the text is the tail both share.
pub const FS_EXDEV: FsClassifiedErrno = FsClassifiedErrno {
    name: "EXDEV",
    wasi: 75,
    kind: std::io::ErrorKind::CrossesDevices,
    linux: 18,
    macos: 18,
    text: "Cross-device link",
};

/// Every classified row. Append, never insert: the p1 fs service lays its
/// statics out in this order.
pub const FS_CLASSIFIED_ERRNOS: &[FsClassifiedErrno] = &[FS_ENOTEMPTY, FS_ENAMETOOLONG, FS_EXDEV];

/// The classified text of an OS error, if its kind is a classified row's.
/// A library error of the same kind (no raw errno) is not classified.
pub fn fs_classified_text(e: &std::io::Error) -> Option<&'static str> {
    e.raw_os_error()?;
    FS_CLASSIFIED_ERRNOS.iter().find(|r| r.kind == e.kind()).map(|r| r.text)
}

/// The text an fs failure answers on every leg: the classified row's text,
/// else the host's own `Display` (the Display rows are identical on every
/// compared host, so that IS the table's text).
pub fn fs_io_error_text(e: &std::io::Error) -> String {
    match fs_classified_text(e) {
        Some(t) => t.to_string(),
        None => e.to_string(),
    }
}
