// net — TCP and UDP networking and name lookup runtime for Almide.
// TcpStream/TcpListener/UdpSocket are opaque i64 handles backed by a
// thread-local registry; stdlib/net.almd wraps them in its `mod type`s.
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

pub fn almide_rt_net_udp_bind(host: &str, port: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net(|t| t.udp_bind(host, port)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_udp_local_port(handle: i64) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_net(|t| t.udp_local_port(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_udp_send_to(handle: i64, data: &[u8], host: &str, port: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net(|t| t.udp_send_to(handle, data, host, port)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_udp_recv_from(handle: i64, max: i64) -> Result<(Vec<u8>, AlmideString, i64), AlmideString> {
    let __almide_r: Result<(Vec<u8>, String, i64), String> = almide_net(|t| t.udp_recv_from(handle, max));
    __almide_r.map(|(d, h, p)| (d, h.into(), p)).map_err(Into::into)
}

pub fn almide_rt_net_udp_close(handle: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_net(|t| t.udp_close(handle)) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_net_lookup_host(host: &str) -> Result<Vec<AlmideString>, AlmideString> {
    let __almide_r: Result<Vec<String>, String> = almide_net_lookup_host(host);
    __almide_r.map(|v| v.into_iter().map(Into::into).collect()).map_err(Into::into)
}
