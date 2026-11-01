// fs_beneath_sys (#4084, #4293): the descriptor syscalls the rooted fs family
// and the `[permissions] fs` scope need and std has no form of. On unix:
// openat (relative to a directory descriptor, with O_NOFOLLOW), readlinkat
// (is this entry a symlink, without following it), unlinkat, mkdirat,
// renameat, fdopendir / readdir (list the directory a descriptor holds), and
// on Linux openat2 (RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS). On Windows the
// same calls made with NtCreateFile relative to a directory handle
// (`OBJECT_ATTRIBUTES.RootDirectory`) and FILE_OPEN_REPARSE_POINT, so a
// reparse point (a symlink, a junction, any other) is opened itself and
// refused, never traversed. The walk built on them (fs_beneath_core.rs, safe
// code) is what keeps a rooted call inside its root when the tree is
// rewritten between the check and the use.
//
// The second sanctioned `unsafe` module of almide-rt-core, beside
// net_unix_sys.rs, for the same reason: native and the embedded wasm host run
// this one module, so they refuse the same trees. Every handle passed in is
// borrowed from a [`Handle`] the caller holds for the whole call; every handle
// returned is owned (close-on-exec / not inheritable).
//
// Platforms: Linux (64- and 32-bit) and macOS, whose flag values and
// `struct dirent` / `struct stat` layouts are spelled below, and Windows.
// Elsewhere `FS_BENEATH_SUPPORTED` is false and every rooted or scoped call
// is refused (fs_beneath_core.rs), and the toolchain refuses
// `[permissions] fs` before a build: there is no path-based check left to
// race.
//
// Both halves answer the same interface: `Handle`, `Open`, `open_at`,
// `symlink_at`, `unlink_at`, `mkdir_at`, `rename_at`, `list_fd`, `meta_at`,
// `open_dir_path`, `identity` and the error helpers, so the walk and the
// scope above are one source on every platform.
//
// Splice discipline (lib.rs): no `crate::` paths, no `use` lines; each
// inline `mod` carries the lint allowance.

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[allow(unsafe_code)]
mod almide_fs_beneath_sys {
    /// The held directory (or entry) a call is made relative to.
    pub type Handle = std::os::fd::OwnedFd;
    type CInt = i32;
    /// `mode_t`: u16 on macOS, u32 on Linux.
    #[cfg(target_os = "macos")]
    type ModeT = u16;
    #[cfg(target_os = "linux")]
    type ModeT = u32;

    #[cfg(target_os = "macos")]
    mod flags {
        pub const O_WRONLY: i32 = 0x1;
        pub const O_NONBLOCK: i32 = 0x4;
        pub const O_APPEND: i32 = 0x8;
        pub const O_NOFOLLOW: i32 = 0x100;
        pub const O_CREAT: i32 = 0x200;
        pub const O_TRUNC: i32 = 0x400;
        pub const O_EXCL: i32 = 0x800;
        pub const O_DIRECTORY: i32 = 0x10_0000;
        pub const O_CLOEXEC: i32 = 0x100_0000;
        pub const AT_REMOVEDIR: i32 = 0x80;
        pub const AT_SYMLINK_NOFOLLOW: i32 = 0x20;
        /// `struct stat` (64-bit inode): dev i32 @0, mode u16 @4, nlink u16 @6,
        /// ino u64 @8, uid @16, gid @20, rdev @24, the four timespecs from @32
        /// (mtime @48), size i64 @96, blocks @104, blksize @112, flags, gen,
        /// lspare, qspare[2]: 144 bytes.
        pub const STAT_SIZE: usize = 144;
        pub const STAT_MODE: usize = 4;
        pub const STAT_INO: usize = 8;
        pub const STAT_MTIME: usize = 48;
        pub const STAT_SIZE_AT: usize = 96;
        /// `struct dirent` (64-bit inode): ino u64, seekoff u64, reclen u16, namlen u16, type u8, name.
        pub const DIRENT_NAME: usize = 21;
        pub const O_LARGEFILE: i32 = 0;
    }
    #[cfg(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "arm", target_arch = "powerpc64", target_arch = "powerpc")))]
    mod flags {
        pub const O_WRONLY: i32 = 0o1;
        pub const O_CREAT: i32 = 0o100;
        pub const O_APPEND: i32 = 0o2000;
        pub const O_NONBLOCK: i32 = 0o4000;
        pub const O_TRUNC: i32 = 0o1000;
        pub const O_EXCL: i32 = 0o200;
        pub const O_DIRECTORY: i32 = 0o40000;
        pub const O_NOFOLLOW: i32 = 0o100000;
        pub const O_CLOEXEC: i32 = 0o2000000;
        /// A descriptor that names the entry without opening it for I/O: no
        /// read permission on the entry is needed, and fstat answers it.
        pub const O_PATH: i32 = 0o10000000;
        pub const AT_REMOVEDIR: i32 = 0x200;
        /// A file past 2 GiB opens on a 32-bit target (the kernel sets it
        /// itself on 64-bit); arm's value, powerpc's differs.
        #[cfg(all(target_pointer_width = "32", target_arch = "powerpc"))]
        pub const O_LARGEFILE: i32 = 0o200000;
        #[cfg(all(target_pointer_width = "32", not(target_arch = "powerpc")))]
        pub const O_LARGEFILE: i32 = 0o400000;
        #[cfg(target_pointer_width = "64")]
        pub const O_LARGEFILE: i32 = 0;
        /// `struct dirent64` (`readdir64` on 32-bit glibc, `readdir` on 64-bit
        /// glibc and on musl): ino u64, off i64, reclen u16, type u8, name.
        pub const DIRENT_NAME: usize = 19;
    }
    #[cfg(all(target_os = "linux", not(any(target_arch = "aarch64", target_arch = "arm", target_arch = "powerpc64", target_arch = "powerpc"))))]
    mod flags {
        pub const O_WRONLY: i32 = 0o1;
        pub const O_CREAT: i32 = 0o100;
        pub const O_APPEND: i32 = 0o2000;
        pub const O_NONBLOCK: i32 = 0o4000;
        pub const O_TRUNC: i32 = 0o1000;
        pub const O_EXCL: i32 = 0o200;
        pub const O_DIRECTORY: i32 = 0o200000;
        pub const O_NOFOLLOW: i32 = 0o400000;
        pub const O_CLOEXEC: i32 = 0o2000000;
        /// A descriptor that names the entry without opening it for I/O: no
        /// read permission on the entry is needed, and fstat answers it.
        pub const O_PATH: i32 = 0o10000000;
        pub const AT_REMOVEDIR: i32 = 0x200;
        /// A file past 2 GiB opens on a 32-bit target (the kernel sets it
        /// itself on 64-bit).
        #[cfg(target_pointer_width = "32")]
        pub const O_LARGEFILE: i32 = 0o100000;
        #[cfg(target_pointer_width = "64")]
        pub const O_LARGEFILE: i32 = 0;
        /// `struct dirent64` (`readdir64` on 32-bit glibc, `readdir` on 64-bit
        /// glibc and on musl): ino u64, off i64, reclen u16, type u8, name.
        pub const DIRENT_NAME: usize = 19;
    }

    #[repr(C)]
    struct Dir {
        _opaque: [u8; 0],
    }

    // SAFETY: the platform C library's own declarations of these functions
    // (POSIX.1-2008), with C `int` as i32 and `size_t` / `ssize_t` as the
    // pointer-width integers; on macOS x86_64 the 64-bit-inode variants, on
    // 32-bit glibc `readdir64` (the 64-bit-inode entry DIRENT_NAME spells).
    unsafe extern "C" {
        fn openat(dirfd: CInt, path: *const std::ffi::c_char, flags: CInt, ...) -> CInt;
        fn readlinkat(dirfd: CInt, path: *const std::ffi::c_char, buf: *mut u8, len: usize) -> isize;
        fn unlinkat(dirfd: CInt, path: *const std::ffi::c_char, flags: CInt) -> CInt;
        fn mkdirat(dirfd: CInt, path: *const std::ffi::c_char, mode: ModeT) -> CInt;
        fn renameat(olddirfd: CInt, old: *const std::ffi::c_char, newdirfd: CInt, new: *const std::ffi::c_char) -> CInt;
        #[cfg_attr(all(target_os = "macos", target_arch = "x86_64"), link_name = "fdopendir$INODE64")]
        fn fdopendir(fd: CInt) -> *mut Dir;
        #[cfg_attr(all(target_os = "macos", target_arch = "x86_64"), link_name = "readdir$INODE64")]
        #[cfg_attr(all(target_os = "linux", target_env = "gnu", target_pointer_width = "32"), link_name = "readdir64")]
        fn readdir(dir: *mut Dir) -> *const u8;
        #[cfg_attr(all(target_os = "macos", target_arch = "x86_64"), link_name = "rewinddir$INODE64")]
        fn rewinddir(dir: *mut Dir);
        fn closedir(dir: *mut Dir) -> CInt;
        #[cfg(target_os = "linux")]
        fn syscall(num: std::ffi::c_long, ...) -> std::ffi::c_long;
        #[cfg(target_os = "linux")]
        fn __errno_location() -> *mut CInt;
        #[cfg(target_os = "macos")]
        fn __error() -> *mut CInt;
        #[cfg(target_os = "macos")]
        #[cfg_attr(target_arch = "x86_64", link_name = "fstatat$INODE64")]
        fn fstatat(dirfd: CInt, path: *const std::ffi::c_char, buf: *mut u8, flags: CInt) -> CInt;
    }

    fn cname(name: &str) -> std::io::Result<std::ffi::CString> {
        std::ffi::CString::new(name).map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
    }

    fn raw(h: &Handle) -> CInt {
        std::os::fd::AsRawFd::as_raw_fd(h)
    }

    fn owned(fd: CInt) -> std::io::Result<std::os::fd::OwnedFd> {
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: `fd` was just returned by a successful open in this process
        // and nothing else refers to it: owning it here is what closes it.
        Ok(unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) })
    }

    /// How a single entry is opened, never following a symlink at it.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Open {
        /// A directory to walk or list.
        Dir,
        /// A file to read.
        Read,
        /// A file to write: created (0o666 before the umask) or truncated.
        WriteTrunc,
        /// A new file only (O_EXCL: an existing entry, a symlink included,
        /// fails EEXIST), private to its owner (0o600).
        CreateNew,
        /// A file to append to: created (0o666 before the umask) when missing.
        Append,
        /// An entry to take the metadata of: read-only and non-blocking (a
        /// FIFO answers at once), whatever its type.
        Meta,
    }

    fn open_flags(how: Open) -> CInt {
        let base = flags::O_NOFOLLOW | flags::O_CLOEXEC | flags::O_LARGEFILE;
        match how {
            Open::Dir => base | flags::O_DIRECTORY,
            Open::Read => base,
            Open::WriteTrunc => base | flags::O_WRONLY | flags::O_CREAT | flags::O_TRUNC,
            Open::CreateNew => base | flags::O_WRONLY | flags::O_CREAT | flags::O_EXCL,
            Open::Append => base | flags::O_WRONLY | flags::O_CREAT | flags::O_APPEND,
            Open::Meta => base | flags::O_NONBLOCK,
        }
    }

    /// `name` (one path segment) under `dir`, opened as `how` says; a symlink
    /// at `name` fails (ELOOP, or ENOTDIR for a directory open).
    pub fn open_at(dir: &Handle, name: &str, how: Open) -> std::io::Result<std::os::fd::OwnedFd> {
        let c = cname(name)?;
        let mode: CInt = if matches!(how, Open::CreateNew) { 0o600 } else { 0o666 };
        // SAFETY: `c` is a NUL-terminated string alive for the call and `dir`
        // is borrowed from a held OwnedFd; the mode is read only with O_CREAT.
        let fd = unsafe { openat(raw(dir), c.as_ptr(), open_flags(how), mode) };
        owned(fd)
    }

    /// Whether `name` under `dir` is itself a symlink: `Ok(true)` when it is,
    /// `Ok(false)` when it exists and is not, the error (ENOENT, ENOTDIR, ...)
    /// otherwise. readlinkat answers EINVAL for an entry that is no link.
    pub fn symlink_at(dir: &Handle, name: &str) -> std::io::Result<bool> {
        let c = cname(name)?;
        let mut buf = [0u8; 1];
        // SAFETY: `buf` is a live, writable buffer of the length passed; `c`
        // and `dir` as in `open_at`.
        let n = unsafe { readlinkat(raw(dir), c.as_ptr(), buf.as_mut_ptr(), buf.len()) };
        if n >= 0 {
            return Ok(true);
        }
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() == Some(22) {
            return Ok(false);
        }
        Err(e)
    }

    /// Remove the entry `name` under `dir` (a directory when `is_dir`);
    /// a symlink there is removed itself, never followed.
    pub fn unlink_at(dir: &Handle, name: &str, is_dir: bool) -> std::io::Result<()> {
        let c = cname(name)?;
        let how = if is_dir { flags::AT_REMOVEDIR } else { 0 };
        // SAFETY: as in `open_at`.
        let r = unsafe { unlinkat(raw(dir), c.as_ptr(), how) };
        if r < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
    }

    /// Create the directory `name` under `dir` with `mode` (before the
    /// umask); a
    /// symlink at `name` is not followed (EEXIST).
    pub fn mkdir_at(dir: &Handle, name: &str, mode: u16) -> std::io::Result<()> {
        let c = cname(name)?;
        // SAFETY: as in `open_at`.
        let r = unsafe { mkdirat(raw(dir), c.as_ptr(), ModeT::from(mode)) };
        if r < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
    }

    /// Rename the entry `from` under `from_dir` to `to` under `to_dir`;
    /// neither name is followed when it is a symlink.
    pub fn rename_at(from_dir: &Handle, from: &str, to_dir: &Handle, to: &str) -> std::io::Result<()> {
        let (a, b) = (cname(from)?, cname(to)?);
        // SAFETY: two NUL-terminated strings alive for the call and two
        // directory descriptors borrowed from held OwnedFds.
        let r = unsafe { renameat(raw(from_dir), a.as_ptr(), raw(to_dir), b.as_ptr()) };
        if r < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
    }

    /// The entry names (`.` and `..` left out) of the directory `fd` holds,
    /// in the order the system answers them, from the start whatever a
    /// duplicate of the descriptor read before. The descriptor is consumed.
    pub fn list_fd(fd: Handle) -> std::io::Result<Vec<std::ffi::OsString>> {
        let raw = std::os::fd::IntoRawFd::into_raw_fd(fd);
        // SAFETY: `raw` is a directory descriptor this call owns; on success
        // the stream owns it and closedir closes it, on failure it is closed
        // here.
        let dir = unsafe { fdopendir(raw) };
        if dir.is_null() {
            let e = std::io::Error::last_os_error();
            drop(owned(raw));
            return Err(e);
        }
        // A duplicate of a held descriptor shares its offset: a listing made
        // before left it at the end, so the stream starts over.
        // SAFETY: `dir` is the live stream opened above.
        unsafe { rewinddir(dir) };
        let mut names = Vec::new();
        let result = loop {
            set_errno(0);
            // SAFETY: `dir` is the live stream opened above, read on this
            // thread only; a non-null entry stays valid until the next call.
            let ent = unsafe { readdir(dir) };
            if ent.is_null() {
                let e = std::io::Error::last_os_error();
                break if e.raw_os_error().unwrap_or(0) == 0 { Ok(()) } else { Err(e) };
            }
            // SAFETY: the entry's name is a NUL-terminated string at the
            // platform's fixed offset inside the entry readdir returned.
            let name = unsafe { std::ffi::CStr::from_ptr(ent.add(flags::DIRENT_NAME) as *const std::ffi::c_char) };
            let bytes = name.to_bytes();
            if bytes != b"." && bytes != b".." {
                names.push(<std::ffi::OsString as std::os::unix::ffi::OsStringExt>::from_vec(bytes.to_vec()));
            }
        };
        // SAFETY: `dir` is the stream opened above, closed once.
        unsafe { closedir(dir) };
        result.map(|()| names)
    }

    fn set_errno(v: CInt) {
        // SAFETY: the thread's own errno cell, which the C library keeps
        // alive for the thread's life.
        #[cfg(target_os = "linux")]
        unsafe {
            *__errno_location() = v;
        }
        // SAFETY: as above.
        #[cfg(target_os = "macos")]
        unsafe {
            *__error() = v;
        }
    }

    /// The metadata of the entry `name` under `dir` itself (a symlink there
    /// is described, not followed), taken by descriptor without opening the
    /// entry for I/O — so an entry the process may not read is described
    /// too, with no path involved. Linux: an O_PATH | O_NOFOLLOW descriptor
    /// and its fstat. macOS: fstatat(AT_SYMLINK_NOFOLLOW) on `dir`.
    #[cfg(target_os = "linux")]
    pub fn meta_at(dir: &Handle, name: &str) -> std::io::Result<super::FsScopedMeta> {
        let c = cname(name)?;
        let how = flags::O_PATH | flags::O_NOFOLLOW | flags::O_CLOEXEC | flags::O_LARGEFILE;
        // SAFETY: as in `open_at`; O_PATH reads no mode argument.
        let fd = owned(unsafe { openat(raw(dir), c.as_ptr(), how) })?;
        std::fs::File::from(fd).metadata().map(super::FsScopedMeta::from)
    }

    #[cfg(target_os = "macos")]
    pub fn meta_at(dir: &Handle, name: &str) -> std::io::Result<super::FsScopedMeta> {
        let c = cname(name)?;
        let mut buf = [0u8; flags::STAT_SIZE];
        // SAFETY: `buf` is a live, writable buffer of `struct stat`'s size on
        // this platform (64-bit inodes); `c` and `dir` as in `open_at`.
        let r = unsafe { fstatat(raw(dir), c.as_ptr(), buf.as_mut_ptr(), flags::AT_SYMLINK_NOFOLLOW) };
        if r < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let at = |o: usize| i64::from_ne_bytes(buf[o..o + 8].try_into().unwrap_or([0; 8]));
        let mode = u16::from_ne_bytes([buf[flags::STAT_MODE], buf[flags::STAT_MODE + 1]]);
        let (secs, nanos) = (at(flags::STAT_MTIME), at(flags::STAT_MTIME + 8));
        let modified = if secs >= 0 {
            std::time::UNIX_EPOCH.checked_add(std::time::Duration::new(secs as u64, nanos.clamp(0, 999_999_999) as u32))
        } else {
            std::time::UNIX_EPOCH.checked_sub(std::time::Duration::from_secs(secs.unsigned_abs()))
        };
        let dev = i32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as u64;
        let ino = at(flags::STAT_INO) as u64;
        Ok(super::FsScopedMeta::from_mode(u32::from(mode), at(flags::STAT_SIZE_AT).max(0) as u64, modified, (dev, ino)))
    }

    /// Linux openat2(2): `rel` (segments joined by `/`) under `dir` as a
    /// directory, with RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS — the kernel
    /// refuses any symlink and any step out of `dir` (ELOOP / EXDEV). `None`
    /// when the kernel has no openat2 (ENOSYS, before 5.6) or a sandbox
    /// refuses it (EPERM, E2BIG): the caller walks segment by segment.
    #[cfg(target_os = "linux")]
    pub fn openat2_dir(dir: &Handle, rel: &str) -> Option<std::io::Result<std::os::fd::OwnedFd>> {
        #[repr(C)]
        struct OpenHow {
            flags: u64,
            mode: u64,
            resolve: u64,
        }
        const SYS_OPENAT2: std::ffi::c_long = 437;
        const RESOLVE_NO_SYMLINKS: u64 = 0x04;
        const RESOLVE_BENEATH: u64 = 0x08;
        let c = match cname(rel) {
            Ok(c) => c,
            Err(e) => return Some(Err(e)),
        };
        let how = OpenHow { flags: open_flags(Open::Dir) as u64, mode: 0, resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS };
        // SAFETY: openat2's argument list — a directory descriptor borrowed
        // from a held OwnedFd, a NUL-terminated path and an open_how struct
        // alive for the call, and that struct's size.
        let fd = unsafe {
            syscall(SYS_OPENAT2, raw(dir) as std::ffi::c_long, c.as_ptr(), &how as *const OpenHow, std::mem::size_of::<OpenHow>())
        };
        if fd < 0 {
            let e = std::io::Error::last_os_error();
            // ENOSYS 38, EPERM 1 (seccomp), E2BIG 7: no openat2 here.
            if matches!(e.raw_os_error(), Some(38 | 1 | 7)) {
                return None;
            }
            return Some(Err(e));
        }
        Some(owned(fd as CInt))
    }

    /// A directory (or any entry) opened by its path, its own symlinks
    /// followed: a granted root or a rooted call's root, which is the
    /// program's choice.
    pub fn open_dir_path(p: &std::path::Path) -> std::io::Result<Handle> {
        std::fs::File::open(p).map(Handle::from)
    }

    /// An open entry's identity: its device and inode.
    pub fn identity(f: &std::fs::File) -> std::io::Result<(u64, u64)> {
        let m = f.metadata()?;
        Ok((std::os::unix::fs::MetadataExt::dev(&m), std::os::unix::fs::MetadataExt::ino(&m)))
    }

    /// EEXIST: an entry is already at the name.
    pub fn err_exists() -> std::io::Error {
        std::io::Error::from_raw_os_error(17)
    }

    /// ENOTDIR: a segment on the way is no directory.
    pub fn err_not_dir() -> std::io::Error {
        std::io::Error::from_raw_os_error(20)
    }

    /// Whether a directory open failed because the entry is no directory.
    pub fn is_not_dir(e: &std::io::Error) -> bool {
        e.raw_os_error() == Some(20)
    }

    /// Whether a directory open of a listed entry failed because there is no
    /// level to descend into: no directory (ENOTDIR) or a symlink, never
    /// followed (ELOOP: 40 on Linux, 62 on macOS).
    pub fn is_not_a_level(e: &std::io::Error) -> bool {
        matches!(e.raw_os_error(), Some(20 | 40 | 62))
    }

    /// Whether the entry is gone (ENOENT).
    pub fn is_gone(e: &std::io::Error) -> bool {
        e.raw_os_error() == Some(2)
    }
}

/// Windows (#4293): every entry below a held directory handle is opened by
/// NtCreateFile with that handle as `OBJECT_ATTRIBUTES.RootDirectory` and ONE
/// name — no separator, no `..`, no drive, UNC or device prefix, no `:stream`
/// (`nt_name`) — and with FILE_OPEN_REPARSE_POINT, so a reparse point there (a
/// symlink, a junction, any other kind) is opened itself instead of followed.
/// Every open that would go on from it — a directory to walk, a file to read
/// or write — checks the opened entry's attributes and refuses a reparse
/// point (ERROR_CANT_RESOLVE_FILENAME, which the walk names an escape): the
/// check is on the handle already held, so a rename after it cannot swap the
/// entry. Removal and rename act on the entry itself, never through it.
#[cfg(windows)]
#[allow(unsafe_code)]
mod almide_fs_beneath_sys {
    /// The held directory (or entry) a call is made relative to.
    pub type Handle = std::os::windows::io::OwnedHandle;
    type RawHandle = *mut std::ffi::c_void;
    type Void = std::ffi::c_void;

    // Fields the system reads or writes, not Rust.
    #[allow(dead_code)]
    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *const u16,
    }

    // Fields the system reads or writes, not Rust.
    #[allow(dead_code)]
    #[repr(C)]
    struct ObjectAttributes {
        length: u32,
        root_directory: RawHandle,
        object_name: *const UnicodeString,
        attributes: u32,
        security_descriptor: *const Void,
        security_quality_of_service: *const Void,
    }

    /// IO_STATUS_BLOCK: a pointer-sized status union, then ULONG_PTR.
    // Fields the system reads or writes, not Rust.
    #[allow(dead_code)]
    #[repr(C)]
    struct IoStatusBlock {
        status: usize,
        information: usize,
    }

    /// FILE_ATTRIBUTE_TAG_INFORMATION.
    // Fields the system reads or writes, not Rust.
    #[allow(dead_code)]
    #[repr(C)]
    struct FileAttributeTag {
        attributes: u32,
        reparse_tag: u32,
    }

    /// FILE_RENAME_INFORMATION up to its name: the flags union (BOOLEAN
    /// ReplaceIfExists / ULONG Flags), RootDirectory, FileNameLength; the
    /// UTF-16 name follows FileNameLength directly.
    // Fields the system reads or writes, not Rust.
    #[allow(dead_code)]
    #[repr(C)]
    struct RenameHead {
        flags: u32,
        root: RawHandle,
        name_len: u32,
    }

    const SYNCHRONIZE: u32 = 0x0010_0000;
    const DELETE: u32 = 0x0001_0000;
    const READ_CONTROL: u32 = 0x0002_0000;
    /// FILE_LIST_DIRECTORY on a directory.
    const FILE_READ_DATA: u32 = 0x1;
    const FILE_WRITE_DATA: u32 = 0x2;
    const FILE_APPEND_DATA: u32 = 0x4;
    const FILE_READ_EA: u32 = 0x8;
    const FILE_WRITE_EA: u32 = 0x10;
    const FILE_TRAVERSE: u32 = 0x20;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
    const FILE_GENERIC_READ: u32 = READ_CONTROL | FILE_READ_DATA | FILE_READ_ATTRIBUTES | FILE_READ_EA | SYNCHRONIZE;
    const FILE_GENERIC_WRITE: u32 = READ_CONTROL | FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES | FILE_WRITE_EA | FILE_APPEND_DATA | SYNCHRONIZE;
    /// FILE_SHARE_READ | WRITE | DELETE: what std opens with.
    const FILE_SHARE_ALL: u32 = 0x7;
    const FILE_OPEN: u32 = 1;
    const FILE_CREATE: u32 = 2;
    const FILE_OPEN_IF: u32 = 3;
    const FILE_DIRECTORY_FILE: u32 = 0x1;
    const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x20;
    const FILE_NON_DIRECTORY_FILE: u32 = 0x40;
    const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    /// Win32's own name matching (CreateFile without POSIX semantics).
    const OBJ_CASE_INSENSITIVE: u32 = 0x40;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    // FILE_INFORMATION_CLASS (NtSetInformationFile / NtQueryInformationFile).
    const FILE_RENAME_INFORMATION: u32 = 10;
    const FILE_DISPOSITION_INFORMATION: u32 = 13;
    const FILE_ATTRIBUTE_TAG_INFORMATION: u32 = 35;
    const FILE_DISPOSITION_INFORMATION_EX: u32 = 64;
    const FILE_RENAME_INFORMATION_EX: u32 = 65;
    /// FILE_DISPOSITION_DELETE | POSIX_SEMANTICS | IGNORE_READONLY_ATTRIBUTE:
    /// the name goes at once, as unlink's does.
    const DISPOSITION_EX_FLAGS: u32 = 0x1 | 0x2 | 0x10;
    /// FILE_RENAME_REPLACE_IF_EXISTS | POSIX_SEMANTICS |
    /// IGNORE_READONLY_ATTRIBUTE: an existing target is replaced, as
    /// rename's is.
    const RENAME_EX_FLAGS: u32 = 0x1 | 0x2 | 0x40;
    // FILE_INFO_BY_HANDLE_CLASS (GetFileInformationByHandleEx).
    const FILE_FULL_DIRECTORY_INFO: i32 = 14;
    const FILE_FULL_DIRECTORY_RESTART_INFO: i32 = 15;
    /// FILE_FULL_DIR_INFO: NextEntryOffset @0, FileAttributes @56,
    /// FileNameLength (bytes) @60, FileName @68.
    const DIR_NEXT: usize = 0;
    const DIR_NAME_LEN: usize = 60;
    const DIR_NAME: usize = 68;
    // Win32 errors.
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_NO_MORE_FILES: i32 = 18;
    const ERROR_INVALID_NAME: i32 = 123;
    const ERROR_FILENAME_EXCED_RANGE: i32 = 206;
    const ERROR_ALREADY_EXISTS: i32 = 183;
    /// STATUS_NOT_A_DIRECTORY's Win32 code.
    const ERROR_DIRECTORY: i32 = 267;
    /// The answer for a reparse point met where the walk would go on.
    const ERROR_CANT_RESOLVE_FILENAME: i32 = 1921;

    // SAFETY: ntdll's documented native API (winternl.h, ntifs.h): HANDLE a
    // pointer, ULONG u32, NTSTATUS i32, the structs above laid out as there.
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtCreateFile(handle: *mut RawHandle, access: u32, attrs: *const ObjectAttributes, iosb: *mut IoStatusBlock, allocation: *const i64, file_attributes: u32, share: u32, disposition: u32, options: u32, ea: *const Void, ea_len: u32) -> i32;
        fn NtSetInformationFile(handle: RawHandle, iosb: *mut IoStatusBlock, info: *const Void, len: u32, class: u32) -> i32;
        fn NtQueryInformationFile(handle: RawHandle, iosb: *mut IoStatusBlock, info: *mut Void, len: u32, class: u32) -> i32;
        fn RtlNtStatusToDosError(status: i32) -> u32;
    }

    // SAFETY: kernel32's declarations (fileapi.h), BOOL as i32.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandleEx(handle: RawHandle, class: i32, info: *mut Void, size: u32) -> i32;
        fn GetFileInformationByHandle(handle: RawHandle, info: *mut u32) -> i32;
    }

    fn raw(h: &impl std::os::windows::io::AsRawHandle) -> RawHandle {
        std::os::windows::io::AsRawHandle::as_raw_handle(h)
    }

    fn nt_err(status: i32) -> std::io::Error {
        // SAFETY: a pure lookup from one status code to another.
        std::io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(status) } as i32)
    }

    /// An information class this volume or system does not know: the
    /// caller falls back to the older class.
    fn unsupported_class(status: i32) -> bool {
        // STATUS_INVALID_PARAMETER, NOT_SUPPORTED, INVALID_INFO_CLASS,
        // NOT_IMPLEMENTED.
        matches!(status as u32, 0xC000_000D | 0xC000_00BB | 0xC000_0003 | 0xC000_0002)
    }

    /// `name` as ONE NT name, or ERROR_INVALID_NAME: empty, `.`, `..`, a
    /// separator (`\` or `/`), a drive or stream colon, a control character
    /// or a Win32 wildcard is refused, so a call never names a parent, an
    /// absolute or prefixed path (drive, UNC, `\\?\`, device) or an
    /// alternate data stream.
    fn nt_name(name: &str) -> std::io::Result<Vec<u16>> {
        if name.is_empty() || name == "." || name == ".." || name.chars().any(|c| c < ' ' || "\\/:*?\"<>|".contains(c)) {
            return Err(std::io::Error::from_raw_os_error(ERROR_INVALID_NAME));
        }
        let w: Vec<u16> = name.encode_utf16().collect();
        if w.len() * 2 > usize::from(u16::MAX) {
            return Err(std::io::Error::from_raw_os_error(ERROR_FILENAME_EXCED_RANGE));
        }
        Ok(w)
    }

    /// NtCreateFile of `name` under `dir`, never following a reparse point
    /// at it; synchronous, shared as std shares, not inheritable.
    fn nt_open(dir: &Handle, name: &str, access: u32, disposition: u32, options: u32) -> std::io::Result<Handle> {
        let w = nt_name(name)?;
        let bytes = (w.len() * 2) as u16;
        let us = UnicodeString { length: bytes, maximum_length: bytes, buffer: w.as_ptr() };
        let oa = ObjectAttributes {
            length: std::mem::size_of::<ObjectAttributes>() as u32,
            root_directory: raw(dir),
            object_name: &us,
            attributes: OBJ_CASE_INSENSITIVE,
            security_descriptor: std::ptr::null(),
            security_quality_of_service: std::ptr::null(),
        };
        let mut h: RawHandle = std::ptr::null_mut();
        let mut iosb = IoStatusBlock { status: 0, information: 0 };
        let options = options | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT;
        // SAFETY: `h`, `iosb`, `oa` and the name `us` points at are live for
        // the call; `dir` is borrowed from a held handle.
        let status = unsafe {
            NtCreateFile(&mut h, access | SYNCHRONIZE | FILE_READ_ATTRIBUTES, &oa, &mut iosb, std::ptr::null(), FILE_ATTRIBUTE_NORMAL, FILE_SHARE_ALL, disposition, options, std::ptr::null(), 0)
        };
        if status < 0 {
            return Err(nt_err(status));
        }
        // SAFETY: `h` was just returned by a successful NtCreateFile in this
        // process and nothing else refers to it: owning it closes it.
        Ok(unsafe { <Handle as std::os::windows::io::FromRawHandle>::from_raw_handle(h) })
    }

    fn tag(h: &Handle) -> std::io::Result<FileAttributeTag> {
        let mut t = FileAttributeTag { attributes: 0, reparse_tag: 0 };
        let mut iosb = IoStatusBlock { status: 0, information: 0 };
        let len = std::mem::size_of::<FileAttributeTag>() as u32;
        // SAFETY: `t` and `iosb` are live, writable and of the length passed;
        // `h` is a held handle opened with FILE_READ_ATTRIBUTES.
        let status = unsafe { NtQueryInformationFile(raw(h), &mut iosb, (&mut t as *mut FileAttributeTag).cast(), len, FILE_ATTRIBUTE_TAG_INFORMATION) };
        if status < 0 {
            return Err(nt_err(status));
        }
        Ok(t)
    }

    fn is_reparse(h: &Handle) -> std::io::Result<bool> {
        Ok(tag(h)?.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0)
    }

    fn set_info(h: &Handle, info: *const Void, len: usize, class: u32) -> i32 {
        let mut iosb = IoStatusBlock { status: 0, information: 0 };
        // SAFETY: `info` points at `len` live bytes laid out as `class`
        // requires (the callers build them), `h` is a held handle.
        unsafe { NtSetInformationFile(raw(h), &mut iosb, info, len as u32, class) }
    }

    /// How a single entry is opened, never following a reparse point at it.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Open {
        /// A directory to walk or list.
        Dir,
        /// A file to read.
        Read,
        /// A file to write: created or truncated.
        WriteTrunc,
        /// A new file only: an existing entry, a reparse point included,
        /// fails ERROR_ALREADY_EXISTS.
        CreateNew,
        /// A file to append to: created when missing.
        Append,
        /// An entry to take the metadata of, whatever its type.
        Meta,
    }

    /// `name` (one path segment) under `dir`, opened as `how` says; a
    /// reparse point at `name` fails (ERROR_CANT_RESOLVE_FILENAME, or
    /// ERROR_DIRECTORY / ERROR_ACCESS_DENIED where its type does not fit).
    /// A truncating write truncates only after the check, on the handle
    /// the check was made on.
    pub fn open_at(dir: &Handle, name: &str, how: Open) -> std::io::Result<Handle> {
        let (access, disposition, options) = match how {
            Open::Dir => (FILE_READ_DATA | FILE_TRAVERSE, FILE_OPEN, FILE_DIRECTORY_FILE),
            Open::Read | Open::Meta => (FILE_GENERIC_READ, FILE_OPEN, 0),
            Open::WriteTrunc => (FILE_GENERIC_WRITE, FILE_OPEN_IF, FILE_NON_DIRECTORY_FILE),
            Open::CreateNew => (FILE_GENERIC_WRITE, FILE_CREATE, FILE_NON_DIRECTORY_FILE),
            Open::Append => (FILE_GENERIC_WRITE & !FILE_WRITE_DATA, FILE_OPEN_IF, FILE_NON_DIRECTORY_FILE),
        };
        let h = nt_open(dir, name, access, disposition, options)?;
        if is_reparse(&h)? {
            return Err(err_loop());
        }
        if how == Open::WriteTrunc {
            let f = std::fs::File::from(h);
            f.set_len(0)?;
            return Ok(Handle::from(f));
        }
        Ok(h)
    }

    /// Whether `name` under `dir` is itself a reparse point (a symlink, a
    /// junction, any other kind): `Ok(true)` when it is, `Ok(false)` when it
    /// exists and is not, the open's error otherwise.
    pub fn symlink_at(dir: &Handle, name: &str) -> std::io::Result<bool> {
        is_reparse(&nt_open(dir, name, 0, FILE_OPEN, 0)?)
    }

    /// Remove the entry `name` under `dir` (a directory when `is_dir`); a
    /// reparse point there is removed itself, never followed.
    pub fn unlink_at(dir: &Handle, name: &str, is_dir: bool) -> std::io::Result<()> {
        let h = nt_open(dir, name, DELETE, FILE_OPEN, 0)?;
        let t = tag(&h)?;
        let (d, link) = (t.attributes & FILE_ATTRIBUTE_DIRECTORY != 0, t.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0);
        if is_dir && (!d || link) {
            return Err(err_not_dir());
        }
        if !is_dir && d && !link {
            return Err(std::io::Error::from_raw_os_error(ERROR_ACCESS_DENIED));
        }
        let ex = DISPOSITION_EX_FLAGS;
        let status = set_info(&h, (&ex as *const u32).cast(), 4, FILE_DISPOSITION_INFORMATION_EX);
        if status >= 0 {
            return Ok(());
        }
        if !unsupported_class(status) {
            return Err(nt_err(status));
        }
        let delete: u8 = 1;
        let status = set_info(&h, (&delete as *const u8).cast(), 1, FILE_DISPOSITION_INFORMATION);
        if status < 0 { Err(nt_err(status)) } else { Ok(()) }
    }

    /// Create the directory `name` under `dir` (`mode` has no Windows form,
    /// as with std's DirBuilder); an entry at `name` is ERROR_ALREADY_EXISTS.
    pub fn mkdir_at(dir: &Handle, name: &str, mode: u16) -> std::io::Result<()> {
        let _ = mode;
        nt_open(dir, name, FILE_READ_DATA, FILE_CREATE, FILE_DIRECTORY_FILE).map(drop)
    }

    /// Rename the entry `from` under `from_dir` to `to` under `to_dir`
    /// (an existing target replaced); neither name is followed when it is a
    /// reparse point.
    pub fn rename_at(from_dir: &Handle, from: &str, to_dir: &Handle, to: &str) -> std::io::Result<()> {
        let w = nt_name(to)?;
        let h = nt_open(from_dir, from, DELETE, FILE_OPEN, 0)?;
        let at = std::mem::offset_of!(RenameHead, name_len) + 4;
        let len = std::cmp::max(at + w.len() * 2, std::mem::size_of::<RenameHead>());
        let build = |flags: u32| -> Vec<u64> {
            let mut buf = vec![0u64; len.div_ceil(8)];
            let p = buf.as_mut_ptr().cast::<RenameHead>();
            // SAFETY: `buf` holds at least `len` bytes, 8-aligned, so the head
            // fits at its start and the name at `at` (an even offset) after
            // it; each field is written in place, the name copied from `w`.
            unsafe {
                std::ptr::addr_of_mut!((*p).flags).write(flags);
                std::ptr::addr_of_mut!((*p).root).write(raw(to_dir));
                std::ptr::addr_of_mut!((*p).name_len).write((w.len() * 2) as u32);
                std::ptr::copy_nonoverlapping(w.as_ptr(), buf.as_mut_ptr().cast::<u8>().add(at).cast::<u16>(), w.len());
            }
            buf
        };
        let ex = build(RENAME_EX_FLAGS);
        let status = set_info(&h, ex.as_ptr().cast(), len, FILE_RENAME_INFORMATION_EX);
        if status >= 0 {
            return Ok(());
        }
        if !unsupported_class(status) {
            return Err(nt_err(status));
        }
        // The older class: ReplaceIfExists TRUE in the union's BOOLEAN.
        let old = build(1);
        let status = set_info(&h, old.as_ptr().cast(), len, FILE_RENAME_INFORMATION);
        if status < 0 { Err(nt_err(status)) } else { Ok(()) }
    }

    fn u32_at(b: &[u8], o: usize) -> Option<u32> {
        Some(u32::from_ne_bytes(b.get(o..o + 4)?.try_into().ok()?))
    }

    /// The entry names (`.` and `..` left out) of the directory `fd` holds,
    /// in the order the system answers them, from the start whatever a
    /// duplicate of the handle read before. The handle is consumed.
    pub fn list_fd(fd: Handle) -> std::io::Result<Vec<std::ffi::OsString>> {
        let mut buf = vec![0u64; 8192];
        let mut names = Vec::new();
        let mut class = FILE_FULL_DIRECTORY_RESTART_INFO;
        loop {
            let size = (buf.len() * 8) as u32;
            // SAFETY: `buf` is live, writable, 8-aligned and `size` bytes
            // long; `fd` is a directory handle this call owns.
            let ok = unsafe { GetFileInformationByHandleEx(raw(&fd), class, buf.as_mut_ptr().cast(), size) };
            if ok == 0 {
                let e = std::io::Error::last_os_error();
                if e.raw_os_error() == Some(ERROR_NO_MORE_FILES) {
                    return Ok(names);
                }
                return Err(e);
            }
            class = FILE_FULL_DIRECTORY_INFO;
            let bytes: Vec<u8> = buf.iter().flat_map(|w| w.to_ne_bytes()).collect();
            let mut at = 0usize;
            loop {
                let bad = || std::io::Error::from(std::io::ErrorKind::InvalidData);
                let next = u32_at(&bytes, at + DIR_NEXT).ok_or_else(bad)? as usize;
                let len = u32_at(&bytes, at + DIR_NAME_LEN).ok_or_else(bad)? as usize;
                let raw_name = bytes.get(at + DIR_NAME..at + DIR_NAME + len).ok_or_else(bad)?;
                let wide: Vec<u16> = raw_name.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]])).collect();
                if wide != [0x2E] && wide != [0x2E, 0x2E] {
                    names.push(<std::ffi::OsString as std::os::windows::ffi::OsStringExt>::from_wide(&wide));
                }
                if next == 0 {
                    break;
                }
                at += next;
            }
        }
    }

    /// The metadata of the entry `name` under `dir` itself (a reparse point
    /// there is described as a symlink, not followed), from a handle opened
    /// for its attributes only — so an entry the process may not read is
    /// described too.
    pub fn meta_at(dir: &Handle, name: &str) -> std::io::Result<super::FsScopedMeta> {
        let h = nt_open(dir, name, 0, FILE_OPEN, 0)?;
        let link = is_reparse(&h)?;
        let f = std::fs::File::from(h);
        let mut m = super::FsScopedMeta::from(f.metadata()?);
        if link {
            m.kind = 3;
        }
        m.identity = identity(&f)?;
        Ok(m)
    }

    /// A directory (or any entry) opened by its path, its own reparse points
    /// followed: a granted root or a rooted call's root, which is the
    /// program's choice. FILE_FLAG_BACKUP_SEMANTICS is what opens a
    /// directory.
    pub fn open_dir_path(p: &std::path::Path) -> std::io::Result<Handle> {
        let mut o = std::fs::OpenOptions::new();
        o.read(true);
        std::os::windows::fs::OpenOptionsExt::custom_flags(&mut o, FILE_FLAG_BACKUP_SEMANTICS);
        o.open(p).map(Handle::from)
    }

    /// An open entry's identity: its volume serial number and file index.
    pub fn identity(f: &std::fs::File) -> std::io::Result<(u64, u64)> {
        // BY_HANDLE_FILE_INFORMATION as 13 ULONGs: the volume serial at 7,
        // the file index's high and low halves at 11 and 12.
        let mut info = [0u32; 13];
        // SAFETY: `info` is a live, writable BY_HANDLE_FILE_INFORMATION-sized
        // buffer (52 bytes, 4-aligned); `f` is an open file.
        let ok = unsafe { GetFileInformationByHandle(raw(f), info.as_mut_ptr()) };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok((u64::from(info[7]), (u64::from(info[11]) << 32) | u64::from(info[12])))
    }

    fn err_loop() -> std::io::Error {
        std::io::Error::from_raw_os_error(ERROR_CANT_RESOLVE_FILENAME)
    }

    /// ERROR_ALREADY_EXISTS: an entry is already at the name.
    pub fn err_exists() -> std::io::Error {
        std::io::Error::from_raw_os_error(ERROR_ALREADY_EXISTS)
    }

    /// ERROR_DIRECTORY (STATUS_NOT_A_DIRECTORY): a segment is no directory.
    pub fn err_not_dir() -> std::io::Error {
        std::io::Error::from_raw_os_error(ERROR_DIRECTORY)
    }

    /// Whether a directory open failed because the entry is no directory.
    pub fn is_not_dir(e: &std::io::Error) -> bool {
        e.raw_os_error() == Some(ERROR_DIRECTORY)
    }

    /// Whether a directory open of a listed entry failed because there is no
    /// level to descend into: no directory, or a reparse point (never
    /// followed).
    pub fn is_not_a_level(e: &std::io::Error) -> bool {
        matches!(e.raw_os_error(), Some(ERROR_DIRECTORY | ERROR_CANT_RESOLVE_FILENAME))
    }

    /// Whether the entry is gone (ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND).
    pub fn is_gone(e: &std::io::Error) -> bool {
        matches!(e.raw_os_error(), Some(2 | 3))
    }
}
