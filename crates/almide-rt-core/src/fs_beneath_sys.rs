// fs_beneath_sys (#4084): the descriptor syscalls the rooted fs family and
// the `[permissions] fs` scope need and std has no form of — openat (relative
// to a directory descriptor, with O_NOFOLLOW), readlinkat (is this entry a
// symlink, without following it), unlinkat, mkdirat, renameat, fdopendir /
// readdir (list the directory a descriptor holds), and on Linux openat2
// (RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS). The walk built on
// them (fs_beneath_core.rs, safe code) is what keeps a rooted call inside its
// root when the tree is rewritten between the check and the use.
//
// The second sanctioned `unsafe` module of almide-rt-core, beside
// net_unix_sys.rs, for the same reason: native and the embedded wasm host run
// this one module, so they refuse the same trees. Every descriptor passed in
// is borrowed from an `OwnedFd` the caller holds for the whole call; every
// descriptor returned is owned (close-on-exec).
//
// Platforms: Linux and macOS on 64-bit targets, whose flag values and
// `struct dirent` layouts are spelled below. Elsewhere `FD_BENEATH` is false
// and the core falls back to its path-based check (racy, as before #4084).
//
// Splice discipline (lib.rs): no `crate::` paths, no `use` lines; the one
// inline `mod` carries the lint allowance.

#[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
#[allow(unsafe_code)]
mod almide_fs_beneath_sys {
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
        /// `struct dirent` (64-bit inode): ino u64, seekoff u64, reclen u16, namlen u16, type u8, name.
        pub const DIRENT_NAME: usize = 21;
    }
    #[cfg(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "powerpc64")))]
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
        pub const AT_REMOVEDIR: i32 = 0x200;
        /// `struct dirent` on 64-bit glibc and musl: ino u64, off i64, reclen u16, type u8, name.
        pub const DIRENT_NAME: usize = 19;
    }
    #[cfg(all(target_os = "linux", not(any(target_arch = "aarch64", target_arch = "powerpc64"))))]
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
        pub const AT_REMOVEDIR: i32 = 0x200;
        /// `struct dirent` on 64-bit glibc and musl: ino u64, off i64, reclen u16, type u8, name.
        pub const DIRENT_NAME: usize = 19;
    }

    #[repr(C)]
    struct Dir {
        _opaque: [u8; 0],
    }

    // SAFETY: the platform C library's own declarations of these functions
    // (POSIX.1-2008), with C `int` as i32 and `size_t` / `ssize_t` as the
    // pointer-width integers; on macOS x86_64 the 64-bit-inode variants.
    unsafe extern "C" {
        fn openat(dirfd: CInt, path: *const std::ffi::c_char, flags: CInt, ...) -> CInt;
        fn readlinkat(dirfd: CInt, path: *const std::ffi::c_char, buf: *mut u8, len: usize) -> isize;
        fn unlinkat(dirfd: CInt, path: *const std::ffi::c_char, flags: CInt) -> CInt;
        fn mkdirat(dirfd: CInt, path: *const std::ffi::c_char, mode: ModeT) -> CInt;
        fn renameat(olddirfd: CInt, old: *const std::ffi::c_char, newdirfd: CInt, new: *const std::ffi::c_char) -> CInt;
        #[cfg_attr(all(target_os = "macos", target_arch = "x86_64"), link_name = "fdopendir$INODE64")]
        fn fdopendir(fd: CInt) -> *mut Dir;
        #[cfg_attr(all(target_os = "macos", target_arch = "x86_64"), link_name = "readdir$INODE64")]
        fn readdir(dir: *mut Dir) -> *const u8;
        fn closedir(dir: *mut Dir) -> CInt;
        #[cfg(target_os = "linux")]
        fn syscall(num: std::ffi::c_long, ...) -> std::ffi::c_long;
        #[cfg(target_os = "linux")]
        fn __errno_location() -> *mut CInt;
        #[cfg(target_os = "macos")]
        fn __error() -> *mut CInt;
    }

    fn cname(name: &str) -> std::io::Result<std::ffi::CString> {
        std::ffi::CString::new(name).map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
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
    #[derive(Clone, Copy)]
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
        let base = flags::O_NOFOLLOW | flags::O_CLOEXEC;
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
    pub fn open_at(dir: std::os::fd::BorrowedFd<'_>, name: &str, how: Open) -> std::io::Result<std::os::fd::OwnedFd> {
        let c = cname(name)?;
        let mode: CInt = if matches!(how, Open::CreateNew) { 0o600 } else { 0o666 };
        // SAFETY: `c` is a NUL-terminated string alive for the call and `dir`
        // is borrowed from a held OwnedFd; the mode is read only with O_CREAT.
        let fd = unsafe { openat(std::os::fd::AsRawFd::as_raw_fd(&dir), c.as_ptr(), open_flags(how), mode) };
        owned(fd)
    }

    /// Whether `name` under `dir` is itself a symlink: `Ok(true)` when it is,
    /// `Ok(false)` when it exists and is not, the error (ENOENT, ENOTDIR, ...)
    /// otherwise. readlinkat answers EINVAL for an entry that is no link.
    pub fn symlink_at(dir: std::os::fd::BorrowedFd<'_>, name: &str) -> std::io::Result<bool> {
        let c = cname(name)?;
        let mut buf = [0u8; 1];
        // SAFETY: `buf` is a live, writable buffer of the length passed; `c`
        // and `dir` as in `open_at`.
        let n = unsafe { readlinkat(std::os::fd::AsRawFd::as_raw_fd(&dir), c.as_ptr(), buf.as_mut_ptr(), buf.len()) };
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
    pub fn unlink_at(dir: std::os::fd::BorrowedFd<'_>, name: &str, is_dir: bool) -> std::io::Result<()> {
        let c = cname(name)?;
        let how = if is_dir { flags::AT_REMOVEDIR } else { 0 };
        // SAFETY: as in `open_at`.
        let r = unsafe { unlinkat(std::os::fd::AsRawFd::as_raw_fd(&dir), c.as_ptr(), how) };
        if r < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
    }

    /// Create the directory `name` under `dir` with `mode` (before the
    /// umask); a
    /// symlink at `name` is not followed (EEXIST).
    pub fn mkdir_at(dir: std::os::fd::BorrowedFd<'_>, name: &str, mode: u16) -> std::io::Result<()> {
        let c = cname(name)?;
        // SAFETY: as in `open_at`.
        let r = unsafe { mkdirat(std::os::fd::AsRawFd::as_raw_fd(&dir), c.as_ptr(), ModeT::from(mode)) };
        if r < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
    }

    /// Rename the entry `from` under `from_dir` to `to` under `to_dir`;
    /// neither name is followed when it is a symlink.
    pub fn rename_at(from_dir: std::os::fd::BorrowedFd<'_>, from: &str, to_dir: std::os::fd::BorrowedFd<'_>, to: &str) -> std::io::Result<()> {
        let (a, b) = (cname(from)?, cname(to)?);
        // SAFETY: two NUL-terminated strings alive for the call and two
        // directory descriptors borrowed from held OwnedFds.
        let r = unsafe { renameat(std::os::fd::AsRawFd::as_raw_fd(&from_dir), a.as_ptr(), std::os::fd::AsRawFd::as_raw_fd(&to_dir), b.as_ptr()) };
        if r < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
    }

    /// The entry names (`.` and `..` left out) of the directory `fd` holds,
    /// in the order the system answers them. The descriptor is consumed.
    pub fn list_fd(fd: std::os::fd::OwnedFd) -> std::io::Result<Vec<Vec<u8>>> {
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
                names.push(bytes.to_vec());
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

    /// Linux openat2(2): `rel` (segments joined by `/`) under `dir` as a
    /// directory, with RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS — the kernel
    /// refuses any symlink and any step out of `dir` (ELOOP / EXDEV). `None`
    /// when the kernel has no openat2 (ENOSYS, before 5.6) or a sandbox
    /// refuses it (EPERM, E2BIG): the caller walks segment by segment.
    #[cfg(target_os = "linux")]
    pub fn openat2_dir(dir: std::os::fd::BorrowedFd<'_>, rel: &str) -> Option<std::io::Result<std::os::fd::OwnedFd>> {
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
            syscall(SYS_OPENAT2, std::os::fd::AsRawFd::as_raw_fd(&dir) as std::ffi::c_long, c.as_ptr(), &how as *const OpenHow, std::mem::size_of::<OpenHow>())
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
}
