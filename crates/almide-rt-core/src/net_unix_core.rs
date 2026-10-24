// net_unix_core (#3620): net's local half — Unix-domain stream sockets, the
// descriptors they carry beside their bytes (SCM_RIGHTS), and the
// shared-memory files such descriptors usually point at; the floor of
// Wayland, X11's shm extension, PipeWire and D-Bus fd passing. ONE source for
// two consumers, as net_core is for tcp:
//
// - runtime/rs/src/net.rs `include!`s this file verbatim and keeps one
//   `AlmideUnixTables` per thread, so the native `almide_rt_net_unix_*` and
//   `almide_rt_net_shm_*` intrinsics are thin wrappers over these methods;
// - almide-wasm-run links this crate and keeps one table per run for the
//   embedded host's interim ops 104..=116 (dropped with the run, which closes
//   every descriptor, as process exit does natively).
//
// Descriptors are the RAW OS numbers, on both legs: a descriptor is what
// crosses to the peer, and one received from it is a number the program
// passes on. The table holds every descriptor the program was given (opened
// here or received — at most MAX_FDS untaken per socket, the rest closed on
// receipt, #4124), each as an `OwnedFd`, and a call naming any other
// number — never issued, a standard stream, closed — is refused before any
// system call. That is what keeps a guest on the embedded host from reaching
// the host's own descriptors, and native answers the same by construction.
// Every descriptor is close-on-exec.
//
// The four syscalls std has no safe form of live in net_unix_sys.rs, the
// crate's one sanctioned `unsafe` module; this file is safe code.
//
// Splice discipline (lib.rs): no `crate::` paths, no `mod`, no `use` lines.

include!("net_unix_sys.rs");

/// The descriptors one run (embedded) or one thread (native) holds, and the
/// ones each socket received that the program has not taken yet.
#[derive(Default)]
pub struct AlmideUnixTables {
    #[cfg(unix)]
    held: std::collections::BTreeMap<i64, std::os::fd::OwnedFd>,
    #[cfg(unix)]
    received: std::collections::BTreeMap<i64, std::collections::VecDeque<i64>>,
}

#[cfg(unix)]
fn almide_unix_timeout(ms: i64) -> i32 {
    if ms < 0 { -1 } else { ms.min(i64::from(i32::MAX)) as i32 }
}

/// Retry `f` while the call was interrupted by a signal.
#[cfg(unix)]
fn almide_unix_retry<T>(mut f: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    loop {
        match f() {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            r => return r,
        }
    }
}

#[cfg(unix)]
impl AlmideUnixTables {
    /// How many descriptors the table holds.
    pub fn held(&self) -> usize {
        self.held.len()
    }

    fn hold(&mut self, fd: std::os::fd::OwnedFd) -> i64 {
        let n = i64::from(std::os::fd::AsRawFd::as_raw_fd(&fd));
        self.held.insert(n, fd);
        n
    }

    /// `fd` itself when the table holds it; else the err naming `call`.
    fn check(&self, call: &str, fd: i64) -> Result<i64, String> {
        if fd < 0 || fd > i64::from(i32::MAX) {
            return Err(format!("{call}: not a file descriptor: {fd}"));
        }
        if !self.held.contains_key(&fd) {
            return Err(format!("{call}: descriptor {fd} is not open in this program"));
        }
        Ok(fd)
    }

    fn borrow(&self, fd: i64) -> std::os::fd::BorrowedFd<'_> {
        std::os::fd::AsFd::as_fd(&self.held[&fd])
    }

    /// Run `f` on the held descriptor `fd` viewed as a `T` (a listener, a
    /// file), then hold it again.
    fn with_as<T, R>(&mut self, call: &str, fd: i64, f: impl FnOnce(&T) -> R) -> Result<R, String>
    where
        T: From<std::os::fd::OwnedFd>,
        std::os::fd::OwnedFd: From<T>,
    {
        let fd = self.check(call, fd)?;
        let owned = self.held.remove(&fd).ok_or_else(|| format!("{call}: descriptor {fd} is not open in this program"))?;
        let t = T::from(owned);
        let r = f(&t);
        self.held.insert(fd, std::os::fd::OwnedFd::from(t));
        Ok(r)
    }

    /// Connect a stream socket to the listener at `path`; its descriptor.
    pub fn unix_connect(&mut self, path: &str) -> Result<i64, String> {
        let s = std::os::unix::net::UnixStream::connect(path).map_err(|e| format!("unix_connect({}): {e}", almide_rt_errtext_escape(path)))?;
        Ok(self.hold(std::os::fd::OwnedFd::from(s)))
    }

    /// Listen for stream connections at `path` (which must not exist yet).
    pub fn unix_listen(&mut self, path: &str) -> Result<i64, String> {
        let l = std::os::unix::net::UnixListener::bind(path).map_err(|e| format!("unix_listen({}): {e}", almide_rt_errtext_escape(path)))?;
        Ok(self.hold(std::os::fd::OwnedFd::from(l)))
    }

    /// Wait for the next connection on `listener`; the connected descriptor.
    pub fn unix_accept(&mut self, listener: i64) -> Result<i64, String> {
        let r = self.with_as("unix_accept", listener, |l: &std::os::unix::net::UnixListener| l.accept())?;
        let (s, _) = r.map_err(|e| format!("unix_accept: {e}"))?;
        Ok(self.hold(std::os::fd::OwnedFd::from(s)))
    }

    /// Send all of `data`, the descriptors `fds` riding with its first byte.
    pub fn unix_send(&mut self, sock: i64, data: &[u8], fds: &[i64]) -> Result<(), String> {
        let sock = self.check("unix_send", sock)?;
        if fds.len() > almide_unix_sys::MAX_FDS {
            return Err(format!("unix_send: {} descriptors in one message (at most {})", fds.len(), almide_unix_sys::MAX_FDS));
        }
        if fds.is_empty() && data.is_empty() {
            return Ok(());
        }
        for &f in fds {
            self.check("unix_send", f)?;
        }
        let riders: Vec<_> = fds.iter().map(|f| self.borrow(*f)).collect();
        let s = self.borrow(sock);
        let mut sent = almide_unix_retry(|| almide_unix_sys::send(&s, data, &riders)).map_err(|e| format!("unix_send: {e}"))?;
        while sent < data.len() {
            sent += almide_unix_retry(|| almide_unix_sys::send(&s, &data[sent..], &[])).map_err(|e| format!("unix_send: {e}"))?;
        }
        Ok(())
    }

    /// Up to `max` bytes — at most 64 KiB per call, the buffer is never `max`
    /// itself (#4123) — blocking until some arrive; empty once the peer
    /// closed. Descriptors that came with them wait for `unix_take_fds`, at
    /// most `MAX_FDS` per socket (#4124): one that arrives while that many
    /// wait untaken is closed on receipt and never held, so a peer cannot
    /// fill the program's descriptor table through a socket it only reads.
    pub fn unix_recv(&mut self, sock: i64, max: i64) -> Result<Vec<u8>, String> {
        let sock = self.check("unix_recv", sock)?;
        let mut buf = vec![0u8; (max.max(0) as usize).min(ALMIDE_UNIX_RECV_CHUNK)];
        let s = self.borrow(sock);
        let (n, fds) = almide_unix_retry(|| almide_unix_sys::recv(&s, &mut buf)).map_err(|e| format!("unix_recv: {e}"))?;
        let waiting = self.received.get(&sock).map_or(0, |q| q.len());
        let room = almide_unix_sys::MAX_FDS.saturating_sub(waiting);
        // `fds` beyond the room drop here: their OwnedFd closes them.
        let got: Vec<i64> = fds.into_iter().take(room).map(|f| self.hold(f)).collect();
        if !got.is_empty() {
            self.received.entry(sock).or_default().extend(got);
        }
        buf.truncate(n);
        Ok(buf)
    }

    /// The descriptors received on `sock` so far and not yet taken, oldest
    /// first; they stay held, now the program's to use and close.
    pub fn unix_take_fds(&mut self, sock: i64) -> Vec<i64> {
        self.received.remove(&sock).map(Vec::from).unwrap_or_default()
    }

    /// Whether `fd` has something to read (data, a connection, or the peer's
    /// close) within `timeout_ms` milliseconds; 0 checks without waiting, a
    /// negative timeout waits as long as it takes.
    pub fn unix_poll(&mut self, fd: i64, timeout_ms: i64) -> Result<bool, String> {
        let fd = self.check("unix_poll", fd)?;
        let b = [self.borrow(fd)];
        let ready = almide_unix_retry(|| almide_unix_sys::readable(&b, almide_unix_timeout(timeout_ms)))
            .map_err(|e| format!("unix_poll: {e}"))?;
        Ok(ready[0])
    }

    /// The descriptors among `fds` that have something to read within
    /// `timeout_ms` milliseconds, in the order given; empty when none did.
    pub fn unix_wait(&mut self, fds: &[i64], timeout_ms: i64) -> Result<Vec<i64>, String> {
        for &f in fds {
            self.check("unix_wait", f)?;
        }
        let b: Vec<_> = fds.iter().map(|f| self.borrow(*f)).collect();
        let ready = almide_unix_retry(|| almide_unix_sys::readable(&b, almide_unix_timeout(timeout_ms)))
            .map_err(|e| format!("unix_wait: {e}"))?;
        Ok(fds.iter().zip(ready).filter(|(_, r)| *r).map(|(f, _)| *f).collect())
    }

    /// Close a descriptor the program holds. A socket's received descriptors
    /// that were never taken close with it.
    pub fn unix_close(&mut self, fd: i64) -> Result<(), String> {
        let fd = self.check("unix_close", fd)?;
        self.held.remove(&fd);
        for f in self.received.remove(&fd).unwrap_or_default() {
            self.held.remove(&f);
        }
        for q in self.received.values_mut() {
            q.retain(|f| *f != fd);
        }
        Ok(())
    }

    /// A new shared-memory file of `size` bytes, zero-filled, with no name
    /// left in any directory: its descriptor, to write into and pass on.
    pub fn shm_create(&mut self, size: i64) -> Result<i64, String> {
        if size < 0 {
            return Err(format!("shm_create: negative size {size}"));
        }
        let file = almide_shm_open().map_err(|e| format!("shm_create({size}): {e}"))?;
        file.set_len(size as u64).map_err(|e| format!("shm_create({size}): {e}"))?;
        Ok(self.hold(std::os::fd::OwnedFd::from(file)))
    }

    /// Grow or shrink the shared-memory file `fd` to `size` bytes.
    pub fn shm_resize(&mut self, fd: i64, size: i64) -> Result<(), String> {
        self.check("shm_resize", fd)?;
        if size < 0 {
            return Err(format!("shm_resize: negative size {size}"));
        }
        self.with_as("shm_resize", fd, |f: &std::fs::File| f.set_len(size as u64))?
            .map_err(|e| format!("shm_resize: {e}"))
    }

    /// Write `data` into the shared-memory file `fd` at byte `offset`.
    pub fn shm_write(&mut self, fd: i64, offset: i64, data: &[u8]) -> Result<(), String> {
        self.check("shm_write", fd)?;
        if offset < 0 {
            return Err(format!("shm_write: negative offset {offset}"));
        }
        self.with_as("shm_write", fd, |f: &std::fs::File| std::os::unix::fs::FileExt::write_all_at(f, data, offset as u64))?
            .map_err(|e| format!("shm_write: {e}"))
    }

    /// Up to `len` bytes of the file `fd` from byte `offset` (fewer at its end).
    pub fn shm_read(&mut self, fd: i64, offset: i64, len: i64) -> Result<Vec<u8>, String> {
        self.check("shm_read", fd)?;
        if offset < 0 || len < 0 {
            return Err(format!("shm_read: negative offset or length ({offset}, {len})"));
        }
        self.with_as("shm_read", fd, |f: &std::fs::File| almide_shm_read_at(f, offset as u64, len as usize))?
            .map_err(|e| format!("shm_read: {e}"))
    }
}

/// A fresh file in the session's runtime directory (a tmpfs on Linux), else
/// the system temp dir, unlinked at once: the name exists only between the
/// create and the unlink.
#[cfg(unix)]
fn almide_shm_open() -> std::io::Result<std::fs::File> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)
        .filter(|d| d.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    for attempt in 0..16u32 {
        let path = dir.join(format!("almide-shm-{}-{}-{}", std::process::id(), stamp, attempt));
        match std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path) {
            Ok(file) => {
                let _ = std::fs::remove_file(&path);
                return Ok(file);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no free name"))
}

/// The most one `unix_recv` or one `shm_read` step takes into memory (#4123).
#[cfg(unix)]
const ALMIDE_UNIX_RECV_CHUNK: usize = 64 * 1024;

/// Up to `len` bytes from `offset`, the answer growing with the bytes the file
/// has (#4123): allocating the program's `len` up front aborted the process
/// on a huge one.
#[cfg(unix)]
fn almide_shm_read_at(f: &std::fs::File, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    let mut chunk = vec![0u8; len.min(ALMIDE_UNIX_RECV_CHUNK)];
    let mut buf = Vec::new();
    while buf.len() < len {
        let want = (len - buf.len()).min(chunk.len());
        match almide_unix_retry(|| std::os::unix::fs::FileExt::read_at(f, &mut chunk[..want], offset + buf.len() as u64))? {
            0 => break,
            n => buf.extend_from_slice(&chunk[..n]),
        }
    }
    Ok(buf)
}

/// Off Unix there is nothing to hold: every call is the platform err.
#[cfg(not(unix))]
impl AlmideUnixTables {
    fn unsupported<T>(call: &str) -> Result<T, String> {
        Err(format!("{call}: Unix-domain sockets and shared-memory files need a Unix platform"))
    }
    pub fn held(&self) -> usize { 0 }
    pub fn unix_connect(&mut self, _: &str) -> Result<i64, String> { Self::unsupported("unix_connect") }
    pub fn unix_listen(&mut self, _: &str) -> Result<i64, String> { Self::unsupported("unix_listen") }
    pub fn unix_accept(&mut self, _: i64) -> Result<i64, String> { Self::unsupported("unix_accept") }
    pub fn unix_send(&mut self, _: i64, _: &[u8], _: &[i64]) -> Result<(), String> { Self::unsupported("unix_send") }
    pub fn unix_recv(&mut self, _: i64, _: i64) -> Result<Vec<u8>, String> { Self::unsupported("unix_recv") }
    pub fn unix_take_fds(&mut self, _: i64) -> Vec<i64> { Vec::new() }
    pub fn unix_poll(&mut self, _: i64, _: i64) -> Result<bool, String> { Self::unsupported("unix_poll") }
    pub fn unix_wait(&mut self, _: &[i64], _: i64) -> Result<Vec<i64>, String> { Self::unsupported("unix_wait") }
    pub fn unix_close(&mut self, _: i64) -> Result<(), String> { Self::unsupported("unix_close") }
    pub fn shm_create(&mut self, _: i64) -> Result<i64, String> { Self::unsupported("shm_create") }
    pub fn shm_resize(&mut self, _: i64, _: i64) -> Result<(), String> { Self::unsupported("shm_resize") }
    pub fn shm_write(&mut self, _: i64, _: i64, _: &[u8]) -> Result<(), String> { Self::unsupported("shm_write") }
    pub fn shm_read(&mut self, _: i64, _: i64, _: i64) -> Result<Vec<u8>, String> { Self::unsupported("shm_read") }
}
