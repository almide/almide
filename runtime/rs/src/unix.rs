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
// every one the program was given and refuses any other. One table for the
// whole process (#4199), with the lock never held across a call that waits,
// as for tcp above; the OS closes what is left at exit.
include!("../../../crates/almide-rt-core/src/net_unix_core.rs");

static ALMIDE_UNIX: std::sync::Mutex<Option<AlmideUnixTables>> = std::sync::Mutex::new(None);

// #4203: a handler that aborts closes the descriptors it came to hold — the
// unix half of net's hook, registered once per process on first use.
static ALMIDE_UNIX_HOOK: std::sync::Once = std::sync::Once::new();

fn almide_unix_request_sync() {
    ALMIDE_UNIX_HOOK.call_once(|| almide_request_hook(almide_unix_request));
    if ALMIDE_CONTAIN.with(|c| c.get()) && !almide_unix_request_active() {
        almide_unix_request_begin();
    }
}

fn almide_unix_request(stage: u8) {
    match stage {
        0 => almide_unix_request_begin(),
        1 => {
            let _ = almide_unix_request_end();
        }
        _ => {
            let held = almide_unix_request_end();
            if let Some(t) = ALMIDE_UNIX.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut() {
                t.close_held(&held);
            }
        }
    }
}

fn almide_unix<R>(f: impl FnOnce(&mut AlmideUnixTables) -> R) -> R {
    almide_unix_request_sync();
    let mut g = ALMIDE_UNIX.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    f(g.get_or_insert_with(AlmideUnixTables::default))
}

/// Run `f` without holding the table lock, on a scratch table that holds a
/// clone of each descriptor in `fds` under its own number (a descriptor the
/// program does not hold is left out, so the core refuses it with its own
/// text). Whatever `f` opened or received is moved into the shared table
/// afterwards; the clones are closed.
#[cfg(unix)]
fn almide_unix_io<R>(fds: &[i64], f: impl FnOnce(&mut AlmideUnixTables) -> R) -> R {
    let mut scratch = AlmideUnixTables::default();
    almide_unix(|t| {
        for n in fds {
            if let Some(Ok(c)) = t.held.get(n).map(|fd| fd.try_clone()) {
                scratch.held.insert(*n, c);
            }
        }
    });
    let cloned: Vec<i64> = scratch.held.keys().copied().collect();
    let r = f(&mut scratch);
    almide_unix(|t| {
        for (n, fd) in std::mem::take(&mut scratch.held) {
            if !cloned.contains(&n) {
                t.held.insert(n, fd);
            }
        }
        for (n, q) in std::mem::take(&mut scratch.received) {
            t.absorb_received(n, q);
        }
    });
    r
}

#[cfg(not(unix))]
fn almide_unix_io<R>(_fds: &[i64], f: impl FnOnce(&mut AlmideUnixTables) -> R) -> R {
    almide_unix(f)
}

pub fn almide_rt_unix_connect(path: &str) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_unix_io(&[], |t| t.unix_connect(path)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix_listen(path: &str) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_unix_io(&[], |t| t.unix_listen(path)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix_accept(listener: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_unix_io(&[listener], |t| t.unix_accept(listener)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix_send(sock: i64, data: &[u8], fds: &[i64]) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_unix_io(&[&[sock][..], fds].concat(), |t| t.unix_send(sock, data, fds)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix_recv(sock: i64, max: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_unix_io(&[sock], |t| t.unix_recv(sock, max)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix_take_fds(sock: i64) -> Vec<i64> {
    almide_unix(|t| t.unix_take_fds(sock))
}

pub fn almide_rt_unix_wait(fds: &[i64], timeout_ms: i64) -> Result<Vec<i64>, AlmideString> { let __almide_r: Result<Vec<i64>, String> = (|| { almide_unix_io(fds, |t| t.unix_wait(fds, timeout_ms)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix_close(fd: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_unix(|t| t.unix_close(fd)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix_shm_create(size: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_unix(|t| t.shm_create(size)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix_shm_resize(fd: i64, size: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_unix(|t| t.shm_resize(fd, size)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix_shm_write(fd: i64, offset: i64, data: &[u8]) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_unix(|t| t.shm_write(fd, offset, data)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_unix_shm_read(fd: i64, offset: i64, len: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_unix(|t| t.shm_read(fd, offset, len)) })(); __almide_r.map_err(Into::into) }
