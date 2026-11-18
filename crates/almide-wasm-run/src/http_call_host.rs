//! The embedded host's http call table (#2633): the wasm lane of the call
//! handle native serves since #2631. Every call runs the SHARED core
//! (`almide_rt_core::http_client_core`, whose `http_call_core.rs` is the text
//! the native splice inlines) — the worker thread, the total_ms / idle_ms
//! clock, cancel, the UTF-8-safe read_new split — so the C-366 observables
//! agree with native by construction. The guest holds only the call id
//! (stdlib/http_call.almd, crates/almide-wasm/src/http_call.rs).
//!
//! Ops (the id rides a_len with a null a_ptr, except open):
//!   53 open    url in a, the start frame in b → the id in the len half, or
//!              err(message); limit cells `classic` open plain
//!              `request_stream`'s classic call
//!   54 state   "1" once the call ended (the wall clock checked), else "0"
//!   55 wait    blocks; the response as frames `[status, body, k1, v1, …]`
//!              (wire order, repeats kept), or err(message)
//!   56 read    the body text since the last read; never blocks
//!   57 cancel  a running call ends as `request cancelled`, the connection shut
//!   58 step    one streaming turn: `<tag><cell piece>[<error>]`, tag `c` =
//!              more to come, `o` = ended ok, `x` = ended with the error
//!   59 drop    the guest released its last copy: cancel and forget
//! The table belongs to one run; when the run ends every call still live is
//! cancelled, as native's process exit closes its sockets.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use almide_rt_core::http_client_core as core;

#[derive(Default)]
pub(crate) struct HttpCalls {
    next: u32,
    live: HashMap<u32, Arc<core::AlmideHttpCallShared>>,
    /// While an `http.serve` handler runs (#4203): the first id it can open.
    request: Option<u32>,
}

impl HttpCalls {
    /// A handler call starts (#4203).
    pub(crate) fn request_begin(&mut self) {
        self.request = Some(self.next + 1);
    }

    /// The handler returned.
    pub(crate) fn request_end(&mut self) {
        self.request = None;
    }

    /// The handler aborted (#4203): cancel and forget every call it started
    /// that is still live — native's `Drop` cancels them as the abort
    /// unwinds; here the guest's drop glue (op 59) never runs.
    pub(crate) fn request_abort(&mut self) -> usize {
        let Some(first) = self.request.take() else { return 0 };
        let ids: Vec<u32> = self.live.keys().copied().filter(|id| *id >= first).collect();
        for id in &ids {
            if let Some(sh) = self.live.remove(id) {
                sh.cancel();
            }
        }
        ids.len()
    }
}

impl Drop for HttpCalls {
    fn drop(&mut self) {
        for sh in self.live.values() {
            sh.cancel();
        }
    }
}

fn pack(status: i64, len: usize) -> i64 {
    (status << 32) | (len as i64 & 0xFFFF_FFFF)
}

fn ok_text(t: String) -> (i64, Vec<u8>) {
    (pack(0, t.len()), t.into_bytes())
}

fn err_text(m: String) -> (i64, Vec<u8>) {
    (pack(1, m.len()), m.into_bytes())
}

/// A `<decimal CHAR count>\n<payload>` cell — stdlib/http_framed.almd's and
/// stdlib/http_call.almd's `string.len` arithmetic.
fn cell(s: &str) -> String {
    format!("{}\n{}", s.chars().count(), s)
}

/// The start frame: method, body, total_ms, idle_ms, then header key/value
/// cells until the frame ends. Both limit cells `classic` is plain
/// `request_stream`'s call (the core's classic mode, no limits named);
/// `None` here. The body is BYTES (#4744): the guest frames the request's
/// exact bytes, never a lossy decoding of them.
type StartFrame = (String, Vec<u8>, Option<(i64, i64)>, Vec<(String, String)>);

/// One `<count>\n<payload>` cell over the frame's bytes, walked as the p3
/// shim walks it (`http_frame_cell`): each counted unit is one byte and the
/// UTF-8 continuation bytes after it. On valid text that is `string.len`'s
/// char count; on a raw body cell it is the count the guest wrote for it.
fn take_cell(rest: &[u8]) -> Result<(&[u8], &[u8]), String> {
    let nl = rest.iter().position(|&b| b == b'\n').ok_or_else(|| "malformed http call frame (missing length)".to_string())?;
    let n: usize = std::str::from_utf8(&rest[..nl])
        .ok()
        .and_then(|d| d.parse().ok())
        .ok_or_else(|| "malformed http call frame (bad length)".to_string())?;
    let tail = &rest[nl + 1..];
    let mut end = 0;
    for _ in 0..n {
        if end >= tail.len() {
            return Err("malformed http call frame (short cell)".to_string());
        }
        end += 1;
        while end < tail.len() && tail[end] & 0xC0 == 0x80 {
            end += 1;
        }
    }
    Ok((&tail[..end], &tail[end..]))
}

fn parse_start_frame(frame: &[u8]) -> Result<StartFrame, String> {
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let int = |s: String| s.parse::<i64>().map_err(|_| "malformed http call frame (bad limit)".to_string());
    let (method, rest) = take_cell(frame)?;
    let (body, rest) = take_cell(rest)?;
    let (total, rest) = take_cell(rest)?;
    let (idle, mut rest) = take_cell(rest)?;
    let mut headers = Vec::new();
    while !rest.is_empty() {
        let (k, r1) = take_cell(rest)?;
        let (v, r2) = take_cell(r1)?;
        headers.push((text(k), text(v)));
        rest = r2;
    }
    let (total, idle) = (text(total), text(idle));
    let limits = if total == "classic" && idle == "classic" { None } else { Some((int(total)?, int(idle)?)) };
    Ok((text(method), body.to_vec(), limits, headers))
}

/// Length-prefixed frames (u32 LE + bytes), the list-of-strings encoding.
fn frames<S: AsRef<[u8]>>(items: &[S]) -> Vec<u8> {
    let mut b = Vec::new();
    for s in items {
        let s = s.as_ref();
        b.extend_from_slice(&(s.len() as u32).to_le_bytes());
        b.extend_from_slice(s);
    }
    b
}

/// Op 53: validate and start the call; its id, or the start error.
pub(crate) fn open(calls: &Mutex<HttpCalls>, url: &str, frame: &[u8]) -> (i64, Vec<u8>) {
    let (method, body, limits, headers) = match parse_start_frame(frame) {
        Ok(f) => f,
        Err(m) => return err_text(m),
    };
    let spawned = match limits {
        Some((total_ms, idle_ms)) => core::http_call_spawn(&method, url, &body, headers, total_ms, idle_ms),
        None => core::http_call_spawn_classic(&method, url, &body, headers),
    };
    match spawned {
        Ok(sh) => {
            let mut t = calls.lock().unwrap_or_else(|p| p.into_inner());
            t.next += 1;
            let id = t.next;
            t.live.insert(id, sh);
            (pack(0, id as usize), Vec::new())
        }
        Err(m) => err_text(m),
    }
}

/// Ops 54..=59 on the call `id`. The table lock is never held across a
/// blocking wait.
pub(crate) fn by_id(calls: &Mutex<HttpCalls>, op: i32, id: u32) -> (i64, Vec<u8>) {
    if op == 59 {
        let gone = calls.lock().unwrap_or_else(|p| p.into_inner()).live.remove(&id);
        if let Some(sh) = gone {
            sh.cancel();
        }
        return (0, Vec::new());
    }
    let Some(sh) = calls.lock().unwrap_or_else(|p| p.into_inner()).live.get(&id).cloned() else {
        return err_text(format!("unknown http call {id}"));
    };
    match op {
        54 => ok_text(if core::http_call_poll(&sh).is_some() { "1" } else { "0" }.to_string()),
        55 => match core::http_call_wait(&sh) {
            // The body cell carries the bytes as the server sent them (#4746):
            // the guest's `body_bytes` copies the cell, `body` decodes it.
            Ok((status, headers, body)) => {
                let mut items = vec![status.to_string().into_bytes(), body];
                for (k, v) in headers {
                    items.push(k.into_bytes());
                    items.push(v.into_bytes());
                }
                let b = frames(&items);
                (pack(0, b.len()), b)
            }
            Err(m) => err_text(m),
        },
        56 => ok_text(core::http_call_read_new(&sh)),
        57 => {
            sh.cancel();
            ok_text(String::new())
        }
        58 => {
            let (piece, ended) = core::http_call_stream_step(&sh);
            ok_text(match ended {
                None => format!("c{}", cell(&piece)),
                Some(Ok(())) => format!("o{}", cell(&piece)),
                Some(Err(e)) => format!("x{}{}", cell(&piece), e),
            })
        }
        _ => err_text(format!("unknown http call op {op}")),
    }
}
