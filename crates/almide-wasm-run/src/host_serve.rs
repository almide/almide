//! The embedded host's `http.serve` ops (#2650 / #2659, C-367). The GUEST
//! owns the serve loop (stdlib/http_serve.almd): it binds once, then takes
//! one connection at a time, reads the request, runs the handler and writes
//! the response, all inside the ONE instance main runs in — so
//! handler-visible state (values main computed and the handler captured, the
//! heap, the allocator) is the native process's, request after request,
//! never a fresh instance per request.
//!
//! The host only moves bytes. The request parse and the response bytes are
//! the guest's (http_serve.almd), the same transcription the stock artifact's
//! WASI 0.3 component runs (wasi_p3_serve.rs moves its bytes over
//! wasi:sockets), so both wasm legs answer with one code path; the replay in
//! tests/http_serve_cross_test.rs holds it byte-identical to native. The bind
//! is the native runtime's own (almide-rt-core's `http_server_bind`), so the
//! `bind failed: <os message>` line is native's.
//!
//! Ops (fs_call, a/b buffers, the usual `status<<32 | len` answer):
//!   70 bind    a = the port in decimal → ok / err("bind failed: …")
//!   73 accept  blocks until a connection arrives; it becomes the current
//!              one (a failed accept is retried, as natively)
//!   74 recv    → ok(the connection's next bytes), empty once the peer
//!              closed; err on a read error
//!   75 send    a = the response bytes, written to the current connection,
//!              which then closes (a write error is ignored, as natively)

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;

pub(crate) const OP_SERVE_BIND: i32 = 70;
pub(crate) const OP_SERVE_ACCEPT: i32 = 73;
pub(crate) const OP_SERVE_RECV: i32 = 74;
pub(crate) const OP_SERVE_SEND: i32 = 75;

/// Is `op` one of the serve ops this module answers.
pub(crate) fn is_serve_op(op: i32) -> bool {
    matches!(op, OP_SERVE_BIND | OP_SERVE_ACCEPT | OP_SERVE_RECV | OP_SERVE_SEND)
}

/// The run's listener and the connection being served.
#[derive(Default)]
pub(crate) struct ServeState {
    listener: Option<TcpListener>,
    conn: Option<TcpStream>,
}

fn pack(status: i64, len: usize) -> i64 {
    (status << 32) | (len as i64 & 0xFFFF_FFFF)
}

fn err_s(m: String) -> (i64, Vec<u8>) {
    (pack(1, m.len()), m.into_bytes())
}

fn ok_empty() -> (i64, Vec<u8>) {
    (pack(0, 0), Vec::new())
}

/// Serve one of the ops 70 / 73 / 74 / 75.
pub(crate) fn dispatch(state: &Mutex<ServeState>, op: i32, a: &[u8]) -> (i64, Vec<u8>) {
    let mut st = state.lock().expect("serve state");
    match op {
        OP_SERVE_BIND => {
            let port: i64 = String::from_utf8_lossy(a).trim().parse().unwrap_or(0);
            match almide_rt_core::http_server_core::http_server_bind(port) {
                Ok(l) => {
                    st.listener = Some(l);
                    ok_empty()
                }
                Err(m) => err_s(m),
            }
        }
        OP_SERVE_ACCEPT => {
            let Some(listener) = st.listener.as_ref() else {
                return err_s("http.serve: no listener bound".to_string());
            };
            let stream = loop {
                if let Ok((s, _)) = listener.accept() {
                    break s;
                }
            };
            st.conn = Some(stream);
            ok_empty()
        }
        OP_SERVE_RECV => {
            let Some(conn) = st.conn.as_mut() else {
                return err_s("http.serve: no connection".to_string());
            };
            let mut buf = vec![0u8; 8192];
            match conn.read(&mut buf) {
                Ok(n) => {
                    buf.truncate(n);
                    (pack(0, n), buf)
                }
                Err(e) => err_s(e.to_string()),
            }
        }
        _ => {
            let Some(mut conn) = st.conn.take() else {
                return err_s("http.serve: no connection".to_string());
            };
            // A client that hung up before the response is native's
            // `let _ = write_response(..)`: ignored, the loop goes on. The
            // connection closes as `conn` drops here.
            let _ = conn.write_all(a);
            ok_empty()
        }
    }
}
