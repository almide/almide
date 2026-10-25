// unix — Unix-domain sockets and shared-memory files runtime for Almide.
//
// NOTE: No top-level `use` lines — the emitter flattens every runtime module
// into one, so a `use` here could collide with net's or http's.

// ══════════════════════════════════════════════════════════════════════════
// Unix-domain sockets and shared-memory files
// ══════════════════════════════════════════════════════════════════════════
//
// The `unix` module (R-unix-01; the local half of `net` until #3853): its
// bodies live in the shared core, which
// the embedded wasm host links too (interim ops 104..=116), so a call answers
// the same bytes, descriptors-received and err text on native and on
// `almide run --target wasm`. Descriptors are raw OS numbers; the table holds
// every one the program was given and refuses any other. Native keeps one
// table per thread; the OS closes what is left at exit.
include!("../../../crates/almide-rt-core/src/net_unix_core.rs");

thread_local! {
    static ALMIDE_UNIX: std::cell::RefCell<AlmideUnixTables> = std::cell::RefCell::new(AlmideUnixTables::default());
}

fn almide_unix<R>(f: impl FnOnce(&mut AlmideUnixTables) -> R) -> R {
    ALMIDE_UNIX.with(|t| f(&mut t.borrow_mut()))
}

pub fn almide_rt_unix___os_connect(path: &str) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_unix(|t| t.unix_connect(path)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix___os_listen(path: &str) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_unix(|t| t.unix_listen(path)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix___os_accept(listener: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_unix(|t| t.unix_accept(listener)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix___os_send(sock: i64, data: &[u8], fds: &[i64]) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_unix(|t| t.unix_send(sock, data, fds)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix___os_recv(sock: i64, max: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_unix(|t| t.unix_recv(sock, max)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix___os_take_fds(sock: i64) -> Vec<i64> {
    almide_unix(|t| t.unix_take_fds(sock))
}

pub fn almide_rt_unix___os_wait(fds: &[i64], timeout_ms: i64) -> Result<Vec<i64>, AlmideString> { let __almide_r: Result<Vec<i64>, String> = (|| { almide_unix(|t| t.unix_wait(fds, timeout_ms)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix___os_close(fd: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_unix(|t| t.unix_close(fd)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix___os_shm_create(size: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_unix(|t| t.shm_create(size)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix___os_shm_resize(fd: i64, size: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_unix(|t| t.shm_resize(fd, size)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix___os_shm_write(fd: i64, offset: i64, data: &[u8]) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_unix(|t| t.shm_write(fd, offset, data)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix___os_shm_read(fd: i64, offset: i64, len: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_unix(|t| t.shm_read(fd, offset, len)) })(); __almide_r.map_err(Into::into) }
