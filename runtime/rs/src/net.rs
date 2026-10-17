// net — TCP networking runtime for Almide.
// TcpStream/TcpListener are opaque i64 handles backed by a thread-local registry.
//
// NOTE: No top-level `use` for std::io / std::net — avoids duplicate
// import errors when both `net` and `http` runtimes are linked.

// The tcp family's bodies live in the shared core (#3620), which the embedded
// wasm host links too — so a tcp call answers the same handle, bytes and err
// text on native and on `almide run --target wasm`. Native keeps one table per
// thread; the OS closes what is left at exit.
include!("../../../crates/almide-rt-core/src/net_core.rs");

thread_local! {
    static ALMIDE_NET: std::cell::RefCell<AlmideNetTables> = std::cell::RefCell::new(AlmideNetTables::default());
}

fn almide_net<R>(f: impl FnOnce(&mut AlmideNetTables) -> R) -> R {
    ALMIDE_NET.with(|t| f(&mut t.borrow_mut()))
}

pub fn almide_rt_net_tcp_connect(host: &str, port: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net(|t| t.tcp_connect(host, port)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_read(handle: i64, len: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_net(|t| t.tcp_read(handle, len)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_write(handle: i64, data: &[u8]) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net(|t| t.tcp_write(handle, data)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_read_exact(handle: i64, len: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_net(|t| t.tcp_read_exact(handle, len)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_close(handle: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net(|t| t.tcp_close(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_is_open(handle: i64) -> bool {
    almide_net(|t| t.tcp_is_open(handle))
}

pub fn almide_rt_net_tcp_read_timeout(handle: i64, len: i64, timeout_ms: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_net(|t| t.tcp_read_timeout(handle, len, timeout_ms)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_available(handle: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net(|t| t.tcp_available(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_listen(host: &str, port: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net(|t| t.tcp_listen(host, port)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_local_port(handle: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net(|t| t.tcp_local_port(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_accept(handle: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net(|t| t.tcp_accept(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_close_listener(handle: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net(|t| t.tcp_close_listener(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_tcp_set_timeout(handle: i64, timeout_ms: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net(|t| t.tcp_set_timeout(handle, timeout_ms)) })(); __almide_r.map_err(Into::into) }


// ══════════════════════════════════════════════════════════════════════════
// Unix-domain sockets and shared-memory files
// ══════════════════════════════════════════════════════════════════════════
//
// The local half of `net` (#3620): its bodies live in the shared core, which
// the embedded wasm host links too (interim ops 104..=116), so a call answers
// the same bytes, descriptors-received and err text on native and on
// `almide run --target wasm`. Descriptors are raw OS numbers; the table holds
// every one the program was given and refuses any other. Native keeps one
// table per thread; the OS closes what is left at exit.
include!("../../../crates/almide-rt-core/src/net_unix_core.rs");

thread_local! {
    static ALMIDE_NET_UNIX: std::cell::RefCell<AlmideUnixTables> = std::cell::RefCell::new(AlmideUnixTables::default());
}

fn almide_net_unix<R>(f: impl FnOnce(&mut AlmideUnixTables) -> R) -> R {
    ALMIDE_NET_UNIX.with(|t| f(&mut t.borrow_mut()))
}

pub fn almide_rt_net_unix_connect(path: &str) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net_unix(|t| t.unix_connect(path)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_listen(path: &str) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net_unix(|t| t.unix_listen(path)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_accept(listener: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net_unix(|t| t.unix_accept(listener)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_send(sock: i64, data: &[u8], fds: &[i64]) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net_unix(|t| t.unix_send(sock, data, fds)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_recv(sock: i64, max: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_net_unix(|t| t.unix_recv(sock, max)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_take_fds(sock: i64) -> Vec<i64> {
    almide_net_unix(|t| t.unix_take_fds(sock))
}

pub fn almide_rt_net_unix_poll(fd: i64, timeout_ms: i64) -> Result<bool, AlmideString> { let __almide_r: Result<bool, String> = (|| { almide_net_unix(|t| t.unix_poll(fd, timeout_ms)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_wait(fds: &[i64], timeout_ms: i64) -> Result<Vec<i64>, AlmideString> { let __almide_r: Result<Vec<i64>, String> = (|| { almide_net_unix(|t| t.unix_wait(fds, timeout_ms)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_unix_close(fd: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net_unix(|t| t.unix_close(fd)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_shm_create(size: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net_unix(|t| t.shm_create(size)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_shm_resize(fd: i64, size: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net_unix(|t| t.shm_resize(fd, size)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_shm_write(fd: i64, offset: i64, data: &[u8]) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net_unix(|t| t.shm_write(fd, offset, data)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_shm_read(fd: i64, offset: i64, len: i64) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { almide_net_unix(|t| t.shm_read(fd, offset, len)) })(); __almide_r.map_err(Into::into) }
