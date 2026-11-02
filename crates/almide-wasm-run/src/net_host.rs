//! The embedded host's net ops (#3620), served with the SAME cores native
//! runs, so handle and descriptor numbering, bytes and every err text agree
//! with native by construction:
//!
//! - ops 91..=103, the tcp family (the net/tcp design step 2), over
//!   `almide_rt_core::net_core` (runtime/rs/src/net.rs includes it verbatim);
//!   they retire when the embedded host runs the WASI 0.3 component (ADR-0023
//!   step 7), whose wasi:sockets serves tcp;
//! - ops 104..=116, Unix-domain sockets and shared-memory files, over
//!   `almide_rt_core::net_unix_core` — see net_unix_host.rs, which carries
//!   their retirement commitment (the same step 7);
//! - ops 140..=145, UDP and name lookup (#4306), over the same net core —
//!   bind, local port, send_to, recv_from, close and lookup_host; they
//!   retire with the tcp ops, when wasi:sockets/udp and ip-name-lookup serve
//!   them. A datagram wait runs on a clone of the socket outside the run's
//!   lock, as native's does, so one fan arm waiting never stalls another.
//!
//! The tables belong to ONE run (a field of the run's `Host`), not to the
//! process or the thread: `almide test` runs files on worker threads, and
//! one file's handles must not reach another. Dropping them with the run
//! closes every socket and descriptor, as process exit does natively.
//!
//! Tcp operands are stdlib/net_wasm.almd's decimal text: the handle (or the
//! host) in `a`; the port, length, milliseconds or `<len>\n<ms>` in `b`,
//! except `tcp_write`, whose `b` is the raw bytes. `udp_send_to`'s `a` is
//! `<sock>\n<port>\n<host>` and its `b` the datagram; `udp_recv_from` answers
//! `<port> <ip>\n` then the datagram's bytes; `lookup_host` one address per
//! line.

use std::sync::Mutex;

use almide_rt_core::net_core::AlmideNetTables;
use almide_rt_core::net_unix_core::AlmideUnixTables;

/// The first and last net op (tcp, then unix / shm).
/// Both families are declared in `almide_wasm::host_ops` (#4338).
pub(crate) const OP_FIRST: i32 = *almide_wasm::host_ops::TCP.start();
pub(crate) const OP_LAST: i32 = crate::net_unix_host::OP_LAST;
const TCP_LAST: i32 = *almide_wasm::host_ops::TCP.end();
const _: () = assert!(TCP_LAST + 1 == crate::net_unix_host::OP_FIRST);
/// The udp and name-lookup ops (#4306).
pub(crate) const UDP_FIRST: i32 = *almide_wasm::host_ops::UDP.start();
pub(crate) const UDP_LAST: i32 = *almide_wasm::host_ops::UDP.end();
const OP_UDP_SEND_TO: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::UDP, 2);
const OP_UDP_RECV_FROM: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::UDP, 3);

/// One run's net state.
#[derive(Default)]
pub(crate) struct NetRun {
    tcp: AlmideNetTables,
    unix: AlmideUnixTables,
}

impl NetRun {
    /// An `http.serve` handler call starts / returns / aborts (#4203): the
    /// shared cores record what it opens and close it on an abort.
    /// The records are per thread; the guest and its host calls run on one.
    pub(crate) fn request_begin(&mut self) {
        almide_rt_core::net_core::almide_net_request_begin();
        almide_rt_core::net_unix_core::almide_unix_request_begin();
    }

    pub(crate) fn request_end(&mut self) {
        let _ = almide_rt_core::net_core::almide_net_request_end();
        let _ = almide_rt_core::net_unix_core::almide_unix_request_end();
    }

    pub(crate) fn request_abort(&mut self) -> usize {
        let tcp = almide_rt_core::net_core::almide_net_request_end();
        let unix = almide_rt_core::net_unix_core::almide_unix_request_end();
        self.tcp.close_opened(&tcp) + self.unix.close_held(&unix)
    }
}

fn int(s: &str) -> Result<i64, String> {
    s.trim().parse().map_err(|_| format!("malformed net operand {}", almide_base::err_text::almide_rt_errtext_quote(s)))
}

fn text(r: Result<i64, String>) -> Result<Vec<u8>, String> {
    r.map(|n| n.to_string().into_bytes())
}

fn unit(r: Result<(), String>) -> Result<Vec<u8>, String> {
    r.map(|()| Vec::new())
}

fn call(t: &mut AlmideNetTables, op: i32, a: &str, b: &[u8]) -> Result<Vec<u8>, String> {
    let bs = || String::from_utf8_lossy(b).into_owned();
    match op {
        91 => text(t.tcp_listen(a, int(&bs())?)),
        92 => text(t.tcp_accept(int(a)?)),
        93 => text(t.tcp_connect(a, int(&bs())?)),
        94 => t.tcp_read(int(a)?, int(&bs())?),
        95 => t.tcp_read_exact(int(a)?, int(&bs())?),
        96 => {
            let f = bs();
            let (len, ms) = f.split_once('\n').ok_or("malformed net operand (missing timeout)")?;
            t.tcp_read_timeout(int(a)?, int(len)?, int(ms)?)
        }
        97 => unit(t.tcp_write(int(a)?, b)),
        98 => text(t.tcp_available(int(a)?)),
        99 => unit(t.tcp_set_timeout(int(a)?, int(&bs())?)),
        100 => unit(t.tcp_close(int(a)?)),
        101 => unit(t.tcp_close_listener(int(a)?)),
        102 => Ok(if t.tcp_is_open(int(a)?) { b"1".to_vec() } else { b"0".to_vec() }),
        103 => text(t.tcp_local_port(int(a)?)),
        140 => text(t.udp_bind(a, int(&bs())?)),
        141 => text(t.udp_local_port(int(a)?)),
        144 => unit(t.udp_close(int(a)?)),
        145 => almide_rt_core::net_core::almide_net_lookup_host(a).map(|v| v.join("\n").into_bytes()),
        _ => Err(format!("unknown net op {op}")),
    }
}

/// `<sock>\n<port>\n<host>`: the host last, so every byte of it survives.
fn sock_port_host(a: &str) -> Result<(i64, i64, &str), String> {
    let mut parts = a.splitn(3, '\n');
    let (Some(sock), Some(port), Some(host)) = (parts.next(), parts.next(), parts.next()) else {
        return Err("malformed net operand (missing udp destination)".to_string());
    };
    Ok((int(sock)?, int(port)?, host))
}

/// A datagram op: the socket is checked and cloned under the run's lock,
/// then the send or the wait runs on the clone with the lock released —
/// native's `almide_net_udp_io`, so the err order and texts match too.
fn datagram(t: &Mutex<NetRun>, op: i32, a: &str, b: &[u8]) -> Result<Vec<u8>, String> {
    let (h, rest) = if op == OP_UDP_SEND_TO {
        let (h, port, host) = sock_port_host(a)?;
        (h, Some((port, host)))
    } else {
        (int(a)?, None)
    };
    let mut scratch = t.lock().unwrap_or_else(|p| p.into_inner()).tcp.udp_scratch(h)?;
    match rest {
        Some((port, host)) => unit(scratch.udp_send_to(h, b, host, port)),
        None => {
            let max = int(&String::from_utf8_lossy(b))?;
            let (data, ip, port) = scratch.udp_recv_from(h, max)?;
            let mut out = format!("{port} {ip}\n").into_bytes();
            out.extend_from_slice(&data);
            Ok(out)
        }
    }
}

/// The `fs_call` form: status 0 = ok, 1 = err, the answer parked for
/// `host_read`.
pub(crate) fn dispatch(t: &Mutex<NetRun>, op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    let r = if op == OP_UDP_SEND_TO || op == OP_UDP_RECV_FROM {
        datagram(t, op, a, b)
    } else {
        let mut t = t.lock().unwrap_or_else(|p| p.into_inner());
        if (UDP_FIRST..=UDP_LAST).contains(&op) || op <= TCP_LAST {
            call(&mut t.tcp, op, a, b)
        } else {
            crate::net_unix_host::call(&mut t.unix, op, a, b)
        }
    };
    let (status, buf) = match r {
        Ok(v) => (0i64, v),
        Err(e) => (1i64, e.into_bytes()),
    };
    ((status << 32) | (buf.len() as i64 & 0xFFFF_FFFF), buf)
}
