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
//!             until a request arrives; the connection is held for 72.
//!             ok(no frames) once a shutdown signal arrived: the guest's
//!             serve loop returns (ADR-0020 §5.6, #2692)
//!   72 reply  a = `<len>\n<payload>` cells (status, body, k1, v1, …) —
//!             the http_framed cell format — written to the held
//!             connection, which then closes

use std::io::{BufWriter, Stdout, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, OnceLock};

use almide_rt_core::http_server_core::{HttpServerWatch, http_server_signal, http_server_watch};

pub(crate) const OP_SERVE_BIND: i32 = 70;
pub(crate) const OP_SERVE_NEXT: i32 = 71;
pub(crate) const OP_SERVE_REPLY: i32 = 72;

/// The run's listener and the connection whose response is pending.
#[derive(Default)]
pub(crate) struct ServeState {
    listener: Option<TcpListener>,
    pending: Option<TcpStream>,
    /// The shutdown watcher while the guest serves.
    watch: Option<HttpServerWatch>,
}

/// The product runner's stdout buffer, shared with the host's print ops.
pub(crate) type LiveOut = Arc<Mutex<BufWriter<Stdout>>>;

/// The run's stdout, which the forced stop flushes. Process-wide because
/// the signal handler is: one serving run per process (the product runner).
static FORCE_OUT: Mutex<Option<LiveOut>> = Mutex::new(None);

/// The forced stop (a second signal, or the drain outlasting the request
/// timeout): flush the run's stdout and exit 1. The buffer is behind a lock
/// the print ops share, so any thread can flush it — no handing back to the
/// guest's thread as native does. Once only: a racing second caller waits
/// for the first one's exit.
fn force_stop() {
    static STOPPING: OnceLock<()> = OnceLock::new();
    if STOPPING.set(()).is_err() {
        loop {
            std::thread::park();
        }
    }
    if let Some(out) = FORCE_OUT.lock().ok().and_then(|g| g.clone()) {
        let _ = out.lock().map(|mut w| w.flush());
    }
    std::process::exit(1);
}

/// Arm shutdown for a guest that bound a listener: the process's signal
/// handler (installed once; `ctrlc` has no uninstall) counts the signal, and
/// from the second on — during the drain or after `serve` returned — stops
/// the run by force. The first one is the watcher's to act on.
fn arm(listener: &TcpListener, out: LiveOut) -> HttpServerWatch {
    static INSTALLED: OnceLock<bool> = OnceLock::new();
    if let Ok(mut g) = FORCE_OUT.lock() {
        *g = Some(out);
    }
    INSTALLED.get_or_init(|| {
        ctrlc::set_handler(|| {
            if http_server_signal() >= 2 {
                force_stop();
            }
        })
        .is_ok()
    });
    http_server_watch(listener, Box::new(force_stop))
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
/// `parse_cells` its http_framed cell parser (both live in host.rs). `live`
/// is the product runner's stdout: only a live run handles shutdown signals
/// (the buffered test harness never serves a real client).
pub(crate) fn dispatch(
    state: &Mutex<ServeState>,
    live: Option<&LiveOut>,
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
                    st.watch = live.map(|out| arm(&l, out.clone()));
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
            let Some((stream, (method, path, body, headers))) =
                almide_rt_core::http_server_core::http_server_next(listener)
            else {
                // A shutdown signal: stop the watcher and close the listener
                // (connections still queued are refused); the guest's loop
                // ends on the empty answer, `http.serve` returns, and the run
                // flushes stdout when main ends.
                st.watch = None;
                st.listener = None;
                return (pack(0, 0), Vec::new());
            };
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
