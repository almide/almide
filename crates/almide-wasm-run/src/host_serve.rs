//! The embedded host's `http.serve` ops (#2650, C-367). The GUEST owns the
//! serve loop (stdlib/http_serve.almd): it binds once, then pulls one
//! request at a time and hands back one response, all inside the ONE
//! instance main runs in — so handler-visible state (values main computed
//! and the handler captured, the heap, the allocator) is the native
//! process's, request after request, never a fresh instance per request.
//! The host only moves bytes, through the SAME server core the native
//! runtime inlines (almide-rt-core's http_server_core), so the request the
//! handler reads and the response bytes the client receives are native's
//! by shared code.
//!
//! Ops (fs_call, a/b buffers, the usual `status<<32 | len` answer):
//!   70 bind   a = the port in decimal → ok / err("bind failed: …")
//!   71 next   → ok(frames [method, target, body, k1, v1, …]), blocking
//!             until a request arrives; the connection is held for 72
//!   72 reply  a = `<len>\n<payload>` cells (status, body, k1, v1, …) —
//!             the http_framed cell format — written to the held
//!             connection, which then closes

use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;

pub(crate) const OP_SERVE_BIND: i32 = 70;
pub(crate) const OP_SERVE_NEXT: i32 = 71;
pub(crate) const OP_SERVE_REPLY: i32 = 72;

/// The run's listener and the connection whose response is pending.
#[derive(Default)]
pub(crate) struct ServeState {
    listener: Option<TcpListener>,
    pending: Option<TcpStream>,
}

fn pack(status: i64, len: usize) -> i64 {
    (status << 32) | (len as i64 & 0xFFFF_FFFF)
}

fn err_s(m: String) -> (i64, Vec<u8>) {
    (pack(1, m.len()), m.into_bytes())
}

/// The host's http_framed cell parser: (first cell, second cell, pairs).
type ParseCells = fn(&str) -> Result<(String, String, Vec<(String, String)>), String>;

/// Serve one of the ops 70..=72; `frames` is the host's list encoding and
/// `parse_cells` its http_framed cell parser (both live in host.rs).
pub(crate) fn dispatch(
    state: &Mutex<ServeState>,
    op: i32,
    a: &str,
    frames: fn(&[String]) -> Vec<u8>,
    parse_cells: ParseCells,
) -> (i64, Vec<u8>) {
    let mut st = state.lock().expect("serve state");
    match op {
        OP_SERVE_BIND => {
            let port: i64 = a.trim().parse().unwrap_or(0);
            match almide_rt_core::http_server_core::http_server_bind(port) {
                Ok(l) => {
                    st.listener = Some(l);
                    (pack(0, 0), Vec::new())
                }
                Err(m) => err_s(m),
            }
        }
        OP_SERVE_NEXT => {
            let Some(listener) = st.listener.as_ref() else {
                return err_s("http.serve: no listener bound".to_string());
            };
            let (stream, (method, path, body, headers)) =
                almide_rt_core::http_server_core::http_server_next(listener);
            st.pending = Some(stream);
            let mut cells = vec![method, path, body];
            for (k, v) in headers {
                cells.push(k);
                cells.push(v);
            }
            let buf = frames(&cells);
            (pack(0, buf.len()), buf)
        }
        _ => {
            let Some(stream) = st.pending.take() else {
                return err_s("http.serve: no request pending".to_string());
            };
            let (status, body, headers) = match parse_cells(a) {
                Ok(p) => p,
                Err(m) => return err_s(m),
            };
            let status: i64 = status.parse().unwrap_or(0);
            // A client that hung up before the response is native's
            // `let _ = write_response(..)`: ignored, the loop goes on.
            let _ = almide_rt_core::http_server_core::http_server_write(stream, status, &headers, &body);
            (pack(0, 0), Vec::new())
        }
    }
}
