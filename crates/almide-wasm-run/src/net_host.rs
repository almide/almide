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
//!   their retirement commitment (the same step 7).
//!
//! The tables belong to ONE run (a field of the run's `Host`), not to the
//! process or the thread: `almide test` runs files on worker threads, and
//! one file's handles must not reach another. Dropping them with the run
//! closes every socket and descriptor, as process exit does natively.
//!
//! Tcp operands are stdlib/net_wasm.almd's decimal text: the handle (or the
//! host) in `a`; the port, length, milliseconds or `<len>\n<ms>` in `b`,
//! except `tcp_write`, whose `b` is the raw bytes.

use std::sync::Mutex;

use almide_rt_core::net_core::AlmideNetTables;
use almide_rt_core::net_unix_core::AlmideUnixTables;

/// The first and last net op (tcp, then unix / shm).
pub(crate) const OP_FIRST: i32 = 91;
pub(crate) const OP_LAST: i32 = crate::net_unix_host::OP_LAST;
const TCP_LAST: i32 = 103;
const _: () = assert!(TCP_LAST + 1 == crate::net_unix_host::OP_FIRST);

/// One run's net state.
#[derive(Default)]
pub(crate) struct NetRun {
    tcp: AlmideNetTables,
    unix: AlmideUnixTables,
}

impl NetRun {
    /// An `http.serve` handler call starts / returns / aborts (#4203): the
    /// shared cores record what it opens and close it on an abort.
    pub(crate) fn request_begin(&mut self) {
        self.tcp.request_begin();
        self.unix.request_begin();
    }

    pub(crate) fn request_end(&mut self) {
        self.tcp.request_end();
        self.unix.request_end();
    }

    pub(crate) fn request_abort(&mut self) -> usize {
        self.tcp.request_abort() + self.unix.request_abort()
    }
}

fn int(s: &str) -> Result<i64, String> {
    s.trim().parse().map_err(|_| format!("malformed net operand {s:?}"))
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
        _ => Err(format!("unknown net op {op}")),
    }
}

/// The `fs_call` form: status 0 = ok, 1 = err, the answer parked for
/// `host_read`.
pub(crate) fn dispatch(t: &Mutex<NetRun>, op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    let mut t = t.lock().unwrap_or_else(|p| p.into_inner());
    let r = if op > TCP_LAST { crate::net_unix_host::call(&mut t.unix, op, a, b) } else { call(&mut t.tcp, op, a, b) };
    let (status, buf) = match r {
        Ok(v) => (0i64, v),
        Err(e) => (1i64, e.into_bytes()),
    };
    ((status << 32) | (buf.len() as i64 & 0xFFFF_FFFF), buf)
}
