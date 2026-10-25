// net — TCP networking runtime for Almide.
// TcpStream/TcpListener are opaque i64 handles backed by one process-wide registry.
//
// NOTE: No top-level `use` for std::io / std::net — avoids duplicate
// import errors when both `net` and `http` runtimes are linked.

// The tcp family's bodies live in the shared core (#3620), which the embedded
// wasm host links too — so a tcp call answers the same handle, bytes and err
// text on native and on `almide run --target wasm`. The OS closes what is left
// at exit.
//
// ONE table for the whole process (#4199), as the embedded host keeps one per
// run: a `fan` arm runs on its own thread, and with a table per thread a
// handle opened in `main` was invalid in an arm, while a socket an arm opened
// came back as a number that named a different socket in `main` (and closed
// with the arm's thread). The lock is never held across a call that waits —
// accept, connect, read, write — so one arm blocked in `tcp_accept` cannot
// stall another arm's `tcp_connect` to it: such a call works on a clone of
// the socket (`try_clone`, the same OS socket) placed at the same handle in a
// scratch table, so it answers exactly the text the shared core does.
include!("../../../crates/almide-rt-core/src/net_core.rs");

static ALMIDE_NET: std::sync::Mutex<AlmideNetTables> =
    std::sync::Mutex::new(AlmideNetTables { streams: Vec::new(), listeners: Vec::new() });

fn almide_net<R>(f: impl FnOnce(&mut AlmideNetTables) -> R) -> R {
    f(&mut ALMIDE_NET.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
}

/// Run `f` on stream `h` without holding the table lock: a scratch table
/// holds a clone of the stream at index `h`. An invalid or closed handle is
/// refused from the shared table, with the core's own text.
fn almide_net_stream_io<R>(h: i64, f: impl FnOnce(&mut AlmideNetTables) -> Result<R, String>) -> Result<R, String> {
    let clone = almide_net(|t| t.stream(h)?.try_clone().map_err(|e| net_io_error_text("TcpStream", &e)))?;
    let mut scratch = AlmideNetTables::default();
    scratch.streams.resize_with(h as usize, || None);
    scratch.streams.push(Some(clone));
    f(&mut scratch)
}

fn almide_net_tcp_connect_unlocked(host: &str, port: i64) -> Result<i64, String> {
    let s = almide_net_each("tcp_connect", host, port, |a| std::net::TcpStream::connect(a))?;
    Ok(almide_net(|t| almide_net_slot(&mut t.streams, s)))
}

fn almide_net_tcp_accept_unlocked(h: i64) -> Result<i64, String> {
    let l = almide_net(|t| t.listener(h)?.try_clone().map_err(|e| net_io_error_text("tcp_accept", &e)))?;
    let (s, _) = l.accept().map_err(|e| net_io_error_text("tcp_accept", &e))?;
    Ok(almide_net(|t| almide_net_slot(&mut t.streams, s)))
}

pub fn almide_rt_net_tcp_connect(host: &str, port: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net_tcp_connect_unlocked(host, port) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_read(handle: i64, len: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_net_stream_io(handle, |t| t.tcp_read(handle, len)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_write(handle: i64, data: &[u8]) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net_stream_io(handle, |t| t.tcp_write(handle, data)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_read_exact(handle: i64, len: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_net_stream_io(handle, |t| t.tcp_read_exact(handle, len)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_close(handle: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net(|t| t.tcp_close(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_is_open(handle: i64) -> bool {
    almide_net(|t| t.tcp_is_open(handle))
}

pub fn almide_rt_net_tcp_read_timeout(handle: i64, len: i64, timeout_ms: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_net_stream_io(handle, |t| t.tcp_read_timeout(handle, len, timeout_ms)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_available(handle: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net_stream_io(handle, |t| t.tcp_available(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_listen(host: &str, port: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net(|t| t.tcp_listen(host, port)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_local_port(handle: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net(|t| t.tcp_local_port(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_accept(handle: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net_tcp_accept_unlocked(handle) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_close_listener(handle: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net(|t| t.tcp_close_listener(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_set_timeout(handle: i64, timeout_ms: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net_stream_io(handle, |t| t.tcp_set_timeout(handle, timeout_ms)) })(); __almide_r.map_err(Into::into) }


// ══════════════════════════════════════════════════════════════════════════
// Unix-domain sockets and shared-memory files
// ══════════════════════════════════════════════════════════════════════════
//
// The local half of `net` (#3620): its bodies live in the shared core, which
// the embedded wasm host links too (interim ops 104..=116), so a call answers
// the same bytes, descriptors-received and err text on native and on
// `almide run --target wasm`. Descriptors are raw OS numbers; the table holds
// every one the program was given and refuses any other. One table for the
// whole process (#4199), with the lock never held across a call that waits,
// as for tcp above; the OS closes what is left at exit.
include!("../../../crates/almide-rt-core/src/net_unix_core.rs");

static ALMIDE_NET_UNIX: std::sync::Mutex<Option<AlmideUnixTables>> = std::sync::Mutex::new(None);

fn almide_net_unix<R>(f: impl FnOnce(&mut AlmideUnixTables) -> R) -> R {
    let mut g = ALMIDE_NET_UNIX.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    f(g.get_or_insert_with(AlmideUnixTables::default))
}

/// Run `f` without holding the table lock, on a scratch table that holds a
/// clone of each descriptor in `fds` under its own number (a descriptor the
/// program does not hold is left out, so the core refuses it with its own
/// text). Whatever `f` opened or received is moved into the shared table
/// afterwards; the clones are closed.
#[cfg(unix)]
fn almide_net_unix_io<R>(fds: &[i64], f: impl FnOnce(&mut AlmideUnixTables) -> R) -> R {
    let mut scratch = AlmideUnixTables::default();
    almide_net_unix(|t| {
        for n in fds {
            if let Some(Ok(c)) = t.held.get(n).map(|fd| fd.try_clone()) {
                scratch.held.insert(*n, c);
            }
        }
    });
    let cloned: Vec<i64> = scratch.held.keys().copied().collect();
    let r = f(&mut scratch);
    almide_net_unix(|t| {
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
fn almide_net_unix_io<R>(_fds: &[i64], f: impl FnOnce(&mut AlmideUnixTables) -> R) -> R {
    almide_net_unix(f)
}

pub fn almide_rt_net_unix_connect(path: &str) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net_unix_io(&[], |t| t.unix_connect(path)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_listen(path: &str) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net_unix_io(&[], |t| t.unix_listen(path)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_accept(listener: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net_unix_io(&[listener], |t| t.unix_accept(listener)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_send(sock: i64, data: &[u8], fds: &[i64]) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net_unix_io(&[&[sock][..], fds].concat(), |t| t.unix_send(sock, data, fds)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_recv(sock: i64, max: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_net_unix_io(&[sock], |t| t.unix_recv(sock, max)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_take_fds(sock: i64) -> Vec<i64> {
    almide_net_unix(|t| t.unix_take_fds(sock))
}

pub fn almide_rt_net_unix_poll(fd: i64, timeout_ms: i64) -> Result<bool, AlmideString> { let __almide_r: Result<bool, String> = (|| { almide_net_unix_io(&[fd], |t| t.unix_poll(fd, timeout_ms)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_wait(fds: &[i64], timeout_ms: i64) -> Result<Vec<i64>, AlmideString> { let __almide_r: Result<Vec<i64>, String> = (|| { almide_net_unix_io(fds, |t| t.unix_wait(fds, timeout_ms)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_close(fd: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net_unix(|t| t.unix_close(fd)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_shm_create(size: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net_unix(|t| t.shm_create(size)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_shm_resize(fd: i64, size: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net_unix(|t| t.shm_resize(fd, size)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_shm_write(fd: i64, offset: i64, data: &[u8]) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net_unix(|t| t.shm_write(fd, offset, data)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_shm_read(fd: i64, offset: i64, len: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_net_unix(|t| t.shm_read(fd, offset, len)) })(); __almide_r.map_err(Into::into) }
