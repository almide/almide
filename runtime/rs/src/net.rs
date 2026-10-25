// net — TCP and UDP networking and name lookup runtime for Almide.
// TcpStream/TcpListener/UdpSocket are opaque i64 handles backed by one
// process-wide registry; stdlib/net.almd wraps them in its `mod type`s.
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
    std::sync::Mutex::new(AlmideNetTables { streams: Vec::new(), listeners: Vec::new(), udps: Vec::new() });

fn almide_net<R>(f: impl FnOnce(&mut AlmideNetTables) -> R) -> R {
    almide_net_request_sync();
    f(&mut ALMIDE_NET.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
}

// #4203: a handler that aborts closes the handles it opened. The hook is
// registered once per process, the first time any thread uses `net`; the
// serve loop then begins, ends or aborts its own thread's record around each
// handler. A table first used inside a handler — after the loop's begin ran
// with no hook yet — starts this thread's record here: `ALMIDE_CONTAIN` is
// set only on the serve loop's thread while a handler runs, so a `fan` arm's
// thread never starts one.
static ALMIDE_NET_HOOK: std::sync::Once = std::sync::Once::new();

fn almide_net_request_sync() {
    ALMIDE_NET_HOOK.call_once(|| almide_request_hook(almide_net_request));
    if ALMIDE_CONTAIN.with(|c| c.get()) && !almide_net_request_active() {
        almide_net_request_begin();
    }
}

fn almide_net_request(stage: u8) {
    match stage {
        0 => almide_net_request_begin(),
        1 => {
            let _ = almide_net_request_end();
        }
        _ => {
            let opened = almide_net_request_end();
            ALMIDE_NET.lock().unwrap_or_else(std::sync::PoisonError::into_inner).close_opened(&opened);
        }
    }
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

/// Run `f` on UDP socket `h` without holding the table lock, as for a
/// stream: a datagram wait in one `fan` arm must not stall another arm.
fn almide_net_udp_io<R>(h: i64, f: impl FnOnce(&mut AlmideNetTables) -> Result<R, String>) -> Result<R, String> {
    let clone = almide_net(|t| t.udp(h)?.try_clone().map_err(|e| net_io_error_text("UdpSocket", &e)))?;
    let mut scratch = AlmideNetTables::default();
    scratch.udps.resize_with(h as usize, || None);
    scratch.udps.push(Some(clone));
    f(&mut scratch)
}

pub fn almide_rt_net_udp_bind(host: &str, port: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net(|t| t.udp_bind(host, port)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_udp_local_port(handle: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net(|t| t.udp_local_port(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_udp_send_to(handle: i64, data: &[u8], host: &str, port: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net_udp_io(handle, |t| t.udp_send_to(handle, data, host, port)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_udp_recv_from(handle: i64, max: i64) -> Result<(Vec<u8>, AlmideString, i64), AlmideString> {
    let __almide_r: Result<(Vec<u8>, String, i64), String> = almide_net_udp_io(handle, |t| t.udp_recv_from(handle, max));
    __almide_r.map(|(d, h, p)| (d, h.into(), p)).map_err(Into::into)
}

pub fn almide_rt_net_udp_close(handle: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net(|t| t.udp_close(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_lookup_host(host: &str) -> Result<Vec<AlmideString>, AlmideString> {
    let __almide_r: Result<Vec<String>, String> = almide_net_lookup_host(host);
    __almide_r.map(|v| v.into_iter().map(Into::into).collect()).map_err(Into::into)
}
