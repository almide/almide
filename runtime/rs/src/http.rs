// http extern — Rust native HTTP client/server (platform layer)
// Uses std::net::TcpStream for client and TcpListener for server.
// HTTPS via rustls (pure-Rust TLS).
// SSE streaming: almide_rt_sse_anthropic_messages (in sse.rs); the OpenAI-compatible helper is Almide (stdlib/http.almd)

// HashMap already imported by prelude
// Read/Write/TcpStream come from the inlined client core (#1715); the
// server core (#2650) writes its std paths fully qualified, so this file
// imports nothing of its own.

// ── HTTP response/request types ──
// The user-facing `HttpRequest` / `HttpResponse` nominals are RUNTIME-BACKED
// (stdlib_info::RUNTIME_BACKED_TYPES): the emitter spells them under the
// reserved `Almide*` names below (#1821), so a user's own `type HttpRequest`
// never meets a runtime item of the same spelling in the flat-spliced module.
// No bare alias may exist here — it collided (E0428) with exactly that user
// type whenever http.rs was inlined.

#[derive(Clone, Debug, PartialEq)]
pub struct AlmideHttpResponse {
    pub status: i64,
    pub body: String,
    pub headers: Vec<(String, String)>,
}

// A record may hold any of the http types, and a record's repr calls each field's
// (#2647). The body is shown; the headers are counted, not listed.
impl AlmideRepr for AlmideHttpResponse {
    fn almide_repr(&self) -> String {
        format!("HttpResponse {{ status: {}, body: {}, headers: {} }}", self.status.almide_repr(), self.body.almide_repr(), self.headers.len())
    }
}

impl AlmideHttpResponse {
    pub fn new(status: i64, body: String) -> Self {
        Self { status, body, headers: vec![("Content-Type".into(), "text/plain".into())] }
    }
    pub fn json(status: i64, body: String) -> Self {
        Self { status, body, headers: vec![("Content-Type".into(), "application/json".into())] }
    }
    /// EXACTLY the caller's headers — no seeded `Content-Type` (#1352). Entries
    /// are upserted in map order, so the list carries at most one entry per
    /// case-insensitive field name. Delegates to the same helper the
    /// `almide_rt_http_with_headers` intrinsic uses, so the struct API and the
    /// intrinsic cannot drift apart again — they used to disagree, this one
    /// REPLACING the header list while the intrinsic seeded-then-upserted.
    pub fn with_headers(status: i64, body: String, headers: AlmideMap<AlmideString, AlmideString>) -> Self {
        Self::from_headers(status, body, &headers)
    }
    /// `upsert_header` per entry, through an index of the lowercased names
    /// already stored: the per-entry scan made building n headers O(n²).
    fn from_headers(status: i64, body: String, headers: &AlmideMap<AlmideString, AlmideString>) -> Self {
        let mut list: Vec<(String, String)> = Vec::with_capacity(headers.len());
        let mut at: std::collections::HashMap<String, usize> = std::collections::HashMap::with_capacity(headers.len());
        for (k, v) in headers.iter() {
            match at.entry(k.to_ascii_lowercase()) {
                std::collections::hash_map::Entry::Occupied(e) => list[*e.get()].1 = v.to_string(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(list.len());
                    list.push((k.to_string(), v.to_string()));
                }
            }
        }
        Self { status, body, headers: list }
    }
}

/// Case-insensitive header upsert — RFC 9110 §5.1: field names are
/// case-insensitive, and ASCII-only (a field name is a `token`), which is
/// exactly `eq_ignore_ascii_case`. Replaces the first matching field's VALUE
/// IN PLACE (the name keeps the spelling it was first stored under, and the
/// entry does not move), and appends when the field is absent. Shared by
/// `set_header` and `with_headers` so a response never carries two entries for
/// one field name and set-then-get always round-trips: before #1352
/// `set_header` matched case-SENSITIVELY, so setting `content-type` on a
/// response holding `Content-Type` appended a SECOND entry that the
/// case-insensitive `get_header` then shadowed — the write vanished.
fn upsert_header(headers: &mut Vec<(String, String)>, key: &str, value: &str) {
    for slot in headers.iter_mut() {
        if slot.0.eq_ignore_ascii_case(key) {
            slot.1 = value.to_string();
            return;
        }
    }
    headers.push((key.to_string(), value.to_string()));
}

// ── Response builders ──

pub fn almide_http_redirect(url: &str, code: i64) -> AlmideHttpResponse {
    AlmideHttpResponse { status: code, body: String::new(), headers: vec![("Location".into(), url.to_string())] }
}

pub fn almide_rt_http_not_found(body: &str) -> AlmideHttpResponse {
    AlmideHttpResponse::new(404, body.to_string())
}

pub fn almide_rt_http_redirect(url: &str) -> AlmideHttpResponse {
    almide_http_redirect(url, 302)
}

pub fn almide_rt_http_response(status: i64, body: &str) -> AlmideHttpResponse {
    AlmideHttpResponse::new(status, body.to_string())
}

pub fn almide_rt_http_json(status: i64, body: &str) -> AlmideHttpResponse {
    AlmideHttpResponse::json(status, body.to_string())
}

pub fn almide_rt_http_with_headers(status: i64, body: &str, headers: &AlmideMap<AlmideString, AlmideString>) -> AlmideHttpResponse {
    AlmideHttpResponse::from_headers(status, body.to_string(), headers)
}

pub fn almide_http_set_status(mut resp: AlmideHttpResponse, code: i64) -> AlmideHttpResponse {
    resp.status = code; resp
}

pub fn almide_http_get_body(resp: &AlmideHttpResponse) -> AlmideString { let __almide_v: String = (|| {
    resp.body.clone()
})(); __almide_v.into_almide() }

pub fn almide_http_set_header(mut resp: AlmideHttpResponse, key: &str, value: &str) -> AlmideHttpResponse {
    upsert_header(&mut resp.headers, key, value);
    resp
}

pub fn almide_http_get_header(resp: &AlmideHttpResponse, key: &str) -> Option<AlmideString> { let __almide_v: Option<String> = (|| {
    resp.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone())
})(); __almide_v.into_almide() }

/// The status code of a response — the read twin of `almide_http_set_status`
/// (#1791; the Almide surface had no status GETTER before the response family).
pub fn almide_http_status_code(resp: &AlmideHttpResponse) -> i64 {
    resp.status
}

/// EVERY value of one field, in wire order — the accessor that keeps a
/// repeated field (`Set-Cookie`) whole where `get_header` answers the first.
pub fn almide_http_header_values(resp: &AlmideHttpResponse, key: &str) -> Vec<AlmideString> { let __almide_v: Vec<String> = (|| {
    resp.headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone()).collect()
})(); __almide_v.into_almide() }

/// All headers as a map keyed by the LOWERCASED field name (RFC 9110 §5.1 —
/// field names are case-insensitive ASCII tokens). A repeated field keeps its
/// FIRST value — the same rule `get_header` / `req_header` apply — so a
/// `Set-Cookie` list is read through `header_values`, never through this map.
pub fn almide_http_headers(resp: &AlmideHttpResponse) -> AlmideMap<AlmideString, AlmideString> { let __almide_v: AlmideMap<String, String> = (|| {
    let mut out = AlmideMap::new();
    for (k, v) in resp.headers.iter() {
        let name = k.to_ascii_lowercase();
        if !out.contains_key(&name) {
            out.insert(name, v.clone());
        }
    }
    out
})(); __almide_v.into_almide() }

/// Router plumbing: HEAD answered by the GET route keeps the status and the
/// headers and drops the body.
pub fn almide_http_set_body(mut resp: AlmideHttpResponse, body: &str) -> AlmideHttpResponse {
    resp.body = body.to_string();
    resp
}

pub fn almide_http_set_cookie(mut resp: AlmideHttpResponse, name: &str, value: &str) -> AlmideHttpResponse {
    resp.headers.push(("Set-Cookie".into(), format!("{}={}", name, value)));
    resp
}

// ── Request accessors ──

// `PartialEq` (#3755): a request is a plain value — `==`, a Set element and
// a Map key compare it by content, as the wasm leg's representation does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlmideHttpRequest {
    pub method: String,
    pub path: String,
    pub body: String,
    pub headers: Vec<(String, String)>,
    /// Path parameters bound by `http.router` (#2588): `{id}` in the route
    /// pattern → `("id", "42")`. Empty for a request the listener parsed.
    pub params: Vec<(String, String)>,
}

/// `http.new_request` (#2588): a request built in code, so a handler or a
/// router can be exercised without a socket. `target` is the request-target
/// as it appears on the wire — path plus any `?query`.
pub fn almide_rt_http_new_request(method: &str, target: &str, body: &str, headers: &AlmideMap<AlmideString, AlmideString>) -> AlmideHttpRequest {
    AlmideHttpRequest {
        method: method.to_string(),
        path: target.to_string(),
        body: body.to_string(),
        headers: header_pairs(headers),
        params: Vec::new(),
    }
}

pub fn almide_http_param(req: &AlmideHttpRequest, name: &str) -> Option<AlmideString> { let __almide_v: Option<String> = (|| {
    req.params.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
})(); __almide_v.into_almide() }

/// Router plumbing: bind path parameters, given flat `[k1, v1, k2, v2, …]`
/// pairs (the shape the wasm rep stores them in). New bindings go IN FRONT of
/// the ones an enclosing `mount` bound, so `param` finds the innermost first
/// and `{org}` of `mount("/orgs/{org}", sub)` stays visible inside `sub`.
pub fn almide_http_req_with_params(mut req: AlmideHttpRequest, flat: &[AlmideString]) -> AlmideHttpRequest {
    let mut params: Vec<(String, String)> =
        flat.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].to_string(), c[1].to_string())).collect();
    params.append(&mut req.params);
    req.params = params;
    req
}

/// Router plumbing for `http.mount`: the sub-app sees the target with the
/// mount prefix stripped.
pub fn almide_http_req_with_path(mut req: AlmideHttpRequest, target: &str) -> AlmideHttpRequest {
    req.path = target.to_string();
    req
}

// ADR-0013 stage 2: the request and response keep their own `String`s and
// cross the runtime boundary unchanged.
almide_into_str_leaf!(AlmideHttpResponse, AlmideHttpRequest);

impl AlmideRepr for AlmideHttpRequest {
    fn almide_repr(&self) -> String {
        format!("HttpRequest {{ method: {}, path: {}, body: {}, headers: {} }}", self.method.almide_repr(), self.path.almide_repr(), self.body.almide_repr(), self.headers.len())
    }
}

// `http.__http_request_repr` / `__http_response_repr` (#4204): the declared
// reprs are these impls; the wasm legs' self-hosted twins print the same text.
pub fn almide_http_req_repr(req: &AlmideHttpRequest) -> AlmideString { req.almide_repr().into_almide() }
pub fn almide_http_resp_repr(resp: &AlmideHttpResponse) -> AlmideString { resp.almide_repr().into_almide() }

pub fn almide_http_req_method(req: &AlmideHttpRequest) -> AlmideString { let __almide_v: String = (|| { req.method.clone() })(); __almide_v.into_almide() }
pub fn almide_http_req_path(req: &AlmideHttpRequest) -> AlmideString { let __almide_v: String = (|| { req.path.clone() })(); __almide_v.into_almide() }
pub fn almide_http_req_body(req: &AlmideHttpRequest) -> AlmideString { let __almide_v: String = (|| { req.body.clone() })(); __almide_v.into_almide() }

pub fn almide_http_req_header(req: &AlmideHttpRequest, key: &str) -> Option<AlmideString> { let __almide_v: Option<String> = (|| {
    req.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone())
})(); __almide_v.into_almide() }

/// Percent-decode a URL component for manual use (stdlib `http.url_decode`),
/// leniently: `+` → space, `%XX` → raw byte. Not the query decoder —
/// `http.query_params` is `url.parse_query` (C-404).
pub fn almide_http_url_decode(s: &str) -> AlmideString { let __almide_v: String = (|| {
    percent_decode(s)
})(); __almide_v.into_almide() }


pub fn almide_http_get(url: &str) -> Result<AlmideString, AlmideString> {
    almide_http_request("GET", url, "", &AlmideMap::new())
}

pub fn almide_http_post(url: &str, body: &str) -> Result<AlmideString, AlmideString> {
    almide_http_request("POST", url, body, &AlmideMap::new())
}

pub fn almide_http_put(url: &str, body: &str) -> Result<AlmideString, AlmideString> {
    almide_http_request("PUT", url, body, &AlmideMap::new())
}

pub fn almide_http_patch(url: &str, body: &str) -> Result<AlmideString, AlmideString> {
    almide_http_request("PATCH", url, body, &AlmideMap::new())
}

pub fn almide_http_delete(url: &str) -> Result<AlmideString, AlmideString> {
    almide_http_request("DELETE", url, "", &AlmideMap::new())
}

pub fn almide_http_get_with_headers(url: &str, headers: &AlmideMap<AlmideString, AlmideString>) -> Result<AlmideString, AlmideString> {
    almide_http_request("GET", url, "", headers)
}

// ── HTTP Client ──
//
// The client core (parse/timeout/framing/TLS/exchange, all three result
// shapes) lives in crates/almide-rt-core/src/http_client_core.rs and is
// inlined here at embed time (#1715) — the SAME text the embedded host
// links, so the C-328 equality holds by shared code. The wrappers below
// only adapt the AlmideMap header surface to the core's pair slice.
include!("../../../crates/almide-rt-core/src/http_client_core.rs");

/// Headers cross into the shared core as a plain pair slice.
fn header_pairs(headers: &AlmideMap<AlmideString, AlmideString>) -> Vec<(String, String)> {
    headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

pub fn almide_http_request(method: &str, url: &str, body: &str, headers: &AlmideMap<AlmideString, AlmideString>) -> Result<AlmideString, AlmideString> { let __almide_v: Result<String, String> = (|| {
    request(method, url, body, &header_pairs(headers))
})(); __almide_v.into_almide() }

// ── Status-preserving client ──
//
// Mirrors the String client but returns `(status_code, body)` for ANY
// complete response — a 404 is `Ok((404, body))`, not an `Err`. `Err` is
// reserved for transport failures (connection / TLS / timeout). The String
// client above collapses the status into the body-or-error distinction, so a
// caller that needs to branch on the numeric status (e.g. 404 vs 200) uses
// these instead.

pub fn almide_http_get_status(url: &str) -> Result<(i64, AlmideString), AlmideString> {
    almide_http_request_status("GET", url, "", &AlmideMap::new())
}

pub fn almide_http_request_status(method: &str, url: &str, body: &str, headers: &AlmideMap<AlmideString, AlmideString>) -> Result<(i64, AlmideString), AlmideString> { let __almide_v: Result<(i64, String), String> = (|| {
    request_status(method, url, body, &header_pairs(headers))
})(); __almide_v.into_almide() }

// ── Full-response client (#1791) ──
//
// The twin of every verb-shaped String client, answering the WHOLE response
// as an `AlmideHttpResponse` — status, every header line in wire order
// (repeats kept), body — for ANY complete response; `Err` is transport-only.
// Redirects are never followed, so a 3xx arrives with its `Location`. The
// String and status clients above are projections of `request_response` in
// the shared core, so the three shapes cannot drift apart.

pub fn almide_http_get_response(url: &str) -> Result<AlmideHttpResponse, AlmideString> {
    almide_http_request_response("GET", url, "", &AlmideMap::new())
}

pub fn almide_http_post_response(url: &str, body: &str) -> Result<AlmideHttpResponse, AlmideString> {
    almide_http_request_response("POST", url, body, &AlmideMap::new())
}

pub fn almide_http_put_response(url: &str, body: &str) -> Result<AlmideHttpResponse, AlmideString> {
    almide_http_request_response("PUT", url, body, &AlmideMap::new())
}

pub fn almide_http_patch_response(url: &str, body: &str) -> Result<AlmideHttpResponse, AlmideString> {
    almide_http_request_response("PATCH", url, body, &AlmideMap::new())
}

pub fn almide_http_delete_response(url: &str) -> Result<AlmideHttpResponse, AlmideString> {
    almide_http_request_response("DELETE", url, "", &AlmideMap::new())
}

pub fn almide_http_request_response(method: &str, url: &str, body: &str, headers: &AlmideMap<AlmideString, AlmideString>) -> Result<AlmideHttpResponse, AlmideString> { let __almide_r: Result<AlmideHttpResponse, String> = (|| {
    request_response(method, url, body, &header_pairs(headers))
        .map(|(status, headers, body)| AlmideHttpResponse { status, body, headers })
})(); __almide_r.map_err(Into::into) }

// ── Binary client ──
//
// Mirrors the String client but returns the raw response body as `Vec<u8>`
// (Almide `Bytes`) with NO UTF-8 conversion, so binary payloads (images,
// audio — e.g. TTS mp3/wav) survive byte-identical.

pub fn almide_http_get_bytes(url: &str) -> Result<Vec<u8>, AlmideString> {
    almide_http_request_bytes("GET", url, "", &AlmideMap::new())
}

pub fn almide_http_request_bytes(method: &str, url: &str, body: &str, headers: &AlmideMap<AlmideString, AlmideString>) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| {
    request_bytes(method, url, body, &header_pairs(headers))
})(); __almide_r.map_err(Into::into) }



// ── Streaming request ──
//
// Like almide_http_request but delivers the response body to a callback
// in chunks as they arrive on the wire. Designed for Server-Sent Events
// (text/event-stream) where a single HTTP response carries many small
// "data: ..." records over time. Handles both `Transfer-Encoding: chunked`
// (the common SSE shape) and plain bodies.
//
// The callback receives raw UTF-8 substrings of the body — it is the
// caller's job to do SSE line splitting / parsing / event assembly. A
// multibyte character split across two reads or two chunks is carried to
// the next delivery whole (#2536), so the concatenation of every delivered
// piece equals the lossy decoding of the whole body.

pub fn almide_http_request_stream(
    method: &str,
    url: &str,
    body: &str,
    headers: &AlmideMap<AlmideString, AlmideString>,
    on_chunk: std::rc::Rc<dyn Fn(AlmideString)>,
) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| {
    // Almide function values use Rc<dyn Fn>; Rc itself does not implement
    // FnMut. Adapt at the boundary while native SSE callers retain FnMut.
    almide_http_request_stream_impl(method, url, body, headers, |chunk| on_chunk(AlmideString::from(chunk)))
})(); __almide_r.map_err(Into::into) }

pub fn almide_http_request_stream_impl(
    method: &str,
    url: &str,
    body: &str,
    headers: &AlmideMap<AlmideString, AlmideString>,
    on_chunk: impl FnMut(String),
) -> Result<(), String> {
    // A CLASSIC call on the shared call core (http_call_core.rs): the same
    // worker, framing, UTF-8 carry and status refusal the embedded lane runs
    // for this function, bounded only by the client read timeout.
    let shared = http_call_spawn_classic(method, url, body, header_pairs(headers))?;
    let call = AlmideHttpCall { inner: std::sync::Arc::new(AlmideHttpCallOwner { shared }) };
    almide_http_stream_pump(&call, on_chunk)
}

// ── Call handle (#2631) ──
//
// The in-flight call itself — the worker thread, the limits, cancel, the
// UTF-8-safe `read_new` split and the streaming step — is the shared core
// crates/almide-rt-core/src/http_call_core.rs (inlined above with the client
// core), the SAME text the embedded wasm host serves `--target wasm` with
// (#2633). What stays here is the native handle's ownership: copies of an
// `HttpCall` share one call, and dropping the last copy cancels it.

/// The owner of a call: its drop (the last copy of the handle going away)
/// cancels the call.
pub struct AlmideHttpCallOwner {
    shared: std::sync::Arc<AlmideHttpCallShared>,
}

impl Drop for AlmideHttpCallOwner {
    fn drop(&mut self) {
        self.shared.cancel();
    }
}

/// The Almide `HttpCall`. Copies share one call.
#[derive(Clone)]
pub struct AlmideHttpCall {
    inner: std::sync::Arc<AlmideHttpCallOwner>,
}

impl std::fmt::Debug for AlmideHttpCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpCall")
    }
}

// A call in flight has no value to show: it is the handle, as Debug says.
impl AlmideRepr for AlmideHttpCall {
    fn almide_repr(&self) -> String { "HttpCall".to_string() }
}

// `http.__http_call_repr` (#4204): a call in flight shows only its type name.
pub fn almide_http_call_repr(c: &AlmideHttpCall) -> AlmideString { c.almide_repr().into_almide() }

impl PartialEq for AlmideHttpCall {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl AlmideHttpCall {
    fn shared(&self) -> &AlmideHttpCallShared {
        &self.inner.shared
    }
}

fn http_call_response(r: Result<HttpTextResponse, String>) -> Result<AlmideHttpResponse, String> {
    r.map(|(status, headers, body)| AlmideHttpResponse { status, body, headers })
}

pub fn almide_http_call_start(
    method: &str,
    url: &str,
    body: &str,
    headers: &AlmideMap<AlmideString, AlmideString>,
    total_ms: i64,
    idle_ms: i64,
) -> Result<AlmideHttpCall, String> {
    let shared = http_call_spawn(method, url, body, header_pairs(headers), total_ms, idle_ms)?;
    Ok(AlmideHttpCall { inner: std::sync::Arc::new(AlmideHttpCallOwner { shared }) })
}

/// `http.start` — the private `__call_start` its Almide wrapper calls with
/// the limits record spread into two Ints (the sibling-call mangling names it).
pub fn almide_rt_http___call_start(
    method: &str,
    url: &str,
    body: &str,
    headers: &AlmideMap<AlmideString, AlmideString>,
    total_ms: i64,
    idle_ms: i64,
) -> Result<AlmideHttpCall, AlmideString> { let __almide_r: Result<AlmideHttpCall, String> = (|| { almide_http_call_start(method, url, body, headers, total_ms, idle_ms) })(); __almide_r.map_err(Into::into) }

/// Never blocks: `None` while the call runs, the result once it ended.
pub fn almide_http_call_poll(c: &AlmideHttpCall) -> Option<Result<AlmideHttpResponse, AlmideString>> { let __almide_v: Option<Result<AlmideHttpResponse, String>> = (|| {
    http_call_poll(c.shared()).map(http_call_response)
})(); __almide_v.into_almide() }

/// Block until the call ends (bounded by its limits) and answer it: ANY
/// complete response is `Ok`, as with `request_response`.
pub fn almide_http_call_wait(c: &AlmideHttpCall) -> Result<AlmideHttpResponse, AlmideString> { let __almide_r: Result<AlmideHttpResponse, String> = (|| {
    http_call_response(http_call_wait(c.shared()))
})(); __almide_r.map_err(Into::into) }

/// The body text received since the previous `read_new`; never blocks. A
/// multibyte character split across reads is held back until it is whole.
pub fn almide_http_call_read_new(c: &AlmideHttpCall) -> AlmideString { let __almide_v: String = (|| {
    http_call_read_new(c.shared())
})(); __almide_v.into_almide() }

pub fn almide_http_call_cancel(c: &AlmideHttpCall) {
    c.shared().cancel();
}

/// The streaming client on the handle (`request_stream_with_limits` and the
/// SSE twins): deliver each arrival to `on_chunk`, answer a non-2xx status
/// the way `request_stream` does, and end with the call's own error — a
/// fired limit names itself.
pub fn almide_http_stream_limited_impl(
    method: &str,
    url: &str,
    body: &str,
    headers: &AlmideMap<AlmideString, AlmideString>,
    (total_ms, idle_ms): (i64, i64),
    on_chunk: impl FnMut(String),
) -> Result<(), String> {
    let call = almide_http_call_start(method, url, body, headers, total_ms, idle_ms)?;
    almide_http_stream_pump(&call, on_chunk)
}

/// Deliver each arrival of `call` to `on_chunk` until it ends, and answer how.
fn almide_http_stream_pump(call: &AlmideHttpCall, mut on_chunk: impl FnMut(String)) -> Result<(), String> {
    loop {
        let (piece, ended) = http_call_stream_step(call.shared());
        if !piece.is_empty() {
            on_chunk(piece);
        }
        if let Some(outcome) = ended {
            return outcome;
        }
    }
}

pub fn almide_rt_http___request_stream_limited(
    method: &str,
    url: &str,
    body: &str,
    headers: &AlmideMap<AlmideString, AlmideString>,
    total_ms: i64,
    idle_ms: i64,
    on_chunk: std::rc::Rc<dyn Fn(AlmideString)>,
) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_http_stream_limited_impl(method, url, body, headers, (total_ms, idle_ms), |chunk| on_chunk(AlmideString::from(chunk))) })(); __almide_r.map_err(Into::into) }

/// The one entry the SSE helpers stream through: no limits = the classic
/// `request_stream` path (ALMIDE_HTTP_TIMEOUT_SECS / 120 s), limits = the
/// handle.
pub fn almide_http_stream_dispatch(
    method: &str,
    url: &str,
    body: &str,
    headers: &AlmideMap<AlmideString, AlmideString>,
    limits: Option<(i64, i64)>,
    on_chunk: impl FnMut(String),
) -> Result<(), String> {
    match limits {
        None => almide_http_request_stream_impl(method, url, body, headers, on_chunk),
        Some(limits) => almide_http_stream_limited_impl(method, url, body, headers, limits, on_chunk),
    }
}


// ── HTTP Server ──
//
// The server core (bind / accept + parse / the response bytes) lives in
// crates/almide-rt-core/src/http_server_core.rs and is inlined here at embed
// time (#2650) — the SAME text the embedded wasm host links for the guest's
// serve ops, so C-367's byte-identity holds by shared code.
include!("../../../crates/almide-rt-core/src/http_server_core.rs");

pub fn almide_http_serve(port: i64, handler: std::rc::Rc<dyn Fn(AlmideHttpRequest) -> Result<AlmideHttpResponse, AlmideString>>) -> Result<(), String> {
    // `http.serve` is typed never-err (`-> Unit`), so a caller's `!` is a
    // no-op (#1049) and a returned Err only surfaced when the call happened
    // to be a fn's tail — elsewhere the server silently never started. A
    // bind failure ABORTS instead, the same line and exit code in every
    // position and on the embedded wasm lane (C-367).
    let listener = match http_server_bind(port) {
        Ok(l) => l,
        Err(m) => {
            almide_abort(m);
        }
    };
    // Shutdown (ADR-0020 §5.6, #2692): a signal ends the loop below; the
    // request in flight is answered first.
    let signals = almide_http_signals_arm();
    let watch = http_server_watch(&listener, Box::new(almide_http_force_from_watcher));
    // Connections are accepted, read and written by the pool's workers (64 in
    // flight, a 10 s request-head deadline, #4021), so a slow client holds one
    // worker and never this thread. The handler runs HERE, one request at a
    // time in the order requests arrive whole: the app is an `Rc` built on
    // this thread and stdout is this thread's buffer (ADR-0020 §5.5, #2665).
    let pool = http_server_pool(listener, HTTP_SERVER_DEFAULT_LIMITS);
    // A handler err is a fixed 500 body naming the request's id, and the error
    // itself goes to stderr under it (C-409, #4150).
    // A runtime abort inside the handler is contained to its request (#4203):
    // ALMIDE_CONTAIN makes the abort unwind instead of exiting, and the same
    // fixed 500 answers it, so one request reaching an out-of-bounds index or
    // a division by zero no longer ends the server.
    while let Some((conn, (method, path, body, headers))) = pool.next() {
        let id = http_server_request_id(&method, &path, &body);
        let req = AlmideHttpRequest { method, path, body, headers, params: Vec::new() };
        ALMIDE_CONTAIN.with(|c| c.set(true));
        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(req)));
        ALMIDE_CONTAIN.with(|c| c.set(false));
        let resp = match ran {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                eprintln!("{}", http_server_handler_error_line(&id, &e));
                AlmideHttpResponse::new(500, http_server_handler_error_body(&id))
            }
            Err(payload) => {
                let text = payload
                    .downcast_ref::<AlmideContained>()
                    .map(|c| c.0.clone())
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "panic".to_string());
                eprintln!("{}", http_server_handler_abort_line(&id, &text));
                AlmideHttpResponse::new(500, http_server_handler_error_body(&id))
            }
        };
        let _ = http_server_write(conn, resp.status, &resp.headers, &resp.body);
    }
    // The responses handed to workers are written before `serve` returns
    // (the listener closed when the accept thread stopped).
    drop(pool);
    drop(watch);
    almide_http_signals_disarm(signals);
    // A second signal that raced the last response: the forced stop.
    if http_server_forced() {
        almide_stdout_flush();
        std::process::exit(1);
    }
    // The drained stop: stdout reaches its file/pipe now, and `http.serve`
    // returns, so main's statements after it run and main's code is the
    // exit code.
    almide_stdout_flush();
    Ok(())
}

// ── Shutdown signals (native; the shared part is in http_server_core) ──
//
// stdout is today ONE thread-local 64 KiB buffer (ALMIDE_STDOUT_BUF), and the
// serve loop runs on the thread that called `http.serve` — the only thread
// that ever writes it while serving, because the loop is sequential. So
// flushing THAT thread's buffer is enough, and the forced path makes the
// serving thread do it: the watcher re-raises the signal at that thread, and
// the handler, running there, flushes and `_exit(1)`s. It cannot flush while
// the interrupted code holds the buffer (a print in progress), so it then
// returns and the watcher raises again on its next tick. ADR-0020 §5.5 moves
// stdout into one process-global buffer behind a lock (the worker pool's
// threads all write it); the forced path then flushes that buffer from the
// watcher itself and the re-raise goes away.
//
// Windows has no SIGTERM: Ctrl-C and Ctrl-Break start the drain, and the
// console handler runs on a thread of its own. A forced stop there exits 1
// from the watcher WITHOUT flushing, since no other thread can reach the
// serving thread's buffer — until §5.5's shared buffer.

// The C calls (signal / pthread_self / pthread_kill / _exit, and
// SetConsoleCtrlHandler on Windows) are sys.rs's `almide_rt_sys_ffi`.

/// The thread serving (pthread_t), where the forced stop must run.
#[cfg(unix)]
static ALMIDE_HTTP_SERVE_THREAD: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A signal is counted on the serving thread: one delivered to another
/// thread (the watcher, a `fan` worker) is passed on to it, so the forced
/// stop always runs where the buffer is.
#[cfg(unix)]
extern "C" fn almide_http_on_signal(sig: i32) {
    use almide_rt_sys_ffi as ffi;
    let serving = ALMIDE_HTTP_SERVE_THREAD.load(std::sync::atomic::Ordering::SeqCst);
    if ffi::thread_self() != serving {
        ffi::thread_kill(serving, sig);
        return;
    }
    if http_server_signal() >= 2 {
        almide_http_force_here();
    }
}

/// The forced stop, from inside the signal handler on the serving thread.
/// It cannot flush while the interrupted code holds the buffer (a print in
/// progress): then it returns, and the watcher raises again.
#[cfg(unix)]
fn almide_http_force_here() {
    let flushed = ALMIDE_STDOUT_BUF
        .try_with(|buf| match buf.try_borrow_mut() {
            Ok(mut w) => {
                let _ = std::io::Write::flush(&mut *w);
                true
            }
            Err(_) => false,
        })
        .unwrap_or(true);
    if flushed {
        almide_rt_sys_ffi::exit_now(1)
    }
}

#[cfg(unix)]
fn almide_http_force_from_watcher() {
    let serving = ALMIDE_HTTP_SERVE_THREAD.load(std::sync::atomic::Ordering::SeqCst);
    almide_rt_sys_ffi::thread_kill(serving, almide_rt_sys_ffi::SIGTERM);
}

/// Install the handler for SIGTERM and SIGINT; answers the dispositions to
/// restore. A signal the process was started with IGNORED (SIGINT under a
/// non-interactive shell's `&`, a `nohup`) stays ignored.
#[cfg(unix)]
fn almide_http_signals_arm() -> [(i32, almide_rt_sys_ffi::Disposition); 2] {
    use almide_rt_sys_ffi as ffi;
    // Touch the buffer here, so the handler never runs its lazy init.
    ALMIDE_STDOUT_BUF.with(|_| ());
    ALMIDE_HTTP_SERVE_THREAD.store(ffi::thread_self(), std::sync::atomic::Ordering::SeqCst);
    let handler = ffi::Disposition::handler(almide_http_on_signal);
    [ffi::SIGTERM, ffi::SIGINT].map(|sig| {
        let prev = ffi::set_signal(sig, handler);
        if prev == ffi::Disposition::IGNORE {
            ffi::set_signal(sig, ffi::Disposition::IGNORE);
        }
        (sig, prev)
    })
}

/// After `http.serve` returned, a signal still ends the run as a forced
/// stop (flush, exit 1) while main goes on — the handler stays. A serve that
/// ran on another thread (a `fan` arm) cannot keep it, since that thread
/// ends and the handler passes signals to it: there the signals do again
/// what they did before `serve`.
#[cfg(unix)]
fn almide_http_signals_disarm(prev: [(i32, almide_rt_sys_ffi::Disposition); 2]) {
    if std::thread::current().name() == Some("main") {
        return;
    }
    for (sig, disposition) in prev {
        almide_rt_sys_ffi::set_signal(sig, disposition);
    }
}

// CTRL_C_EVENT = 0, CTRL_BREAK_EVENT = 1.
#[cfg(windows)]
extern "system" fn almide_http_on_ctrl(event: u32) -> i32 {
    match event {
        0 | 1 => {
            http_server_signal();
            1
        }
        _ => 0,
    }
}

#[cfg(windows)]
fn almide_http_force_from_watcher() {
    std::process::exit(1);
}

#[cfg(windows)]
fn almide_http_signals_arm() {
    almide_rt_sys_ffi::set_console_ctrl_handler(almide_http_on_ctrl, true);
}

#[cfg(windows)]
fn almide_http_signals_disarm(_: ()) {
    almide_rt_sys_ffi::set_console_ctrl_handler(almide_http_on_ctrl, false);
}

#[cfg(not(any(unix, windows)))]
fn almide_http_force_from_watcher() {}

#[cfg(not(any(unix, windows)))]
fn almide_http_signals_arm() {}

#[cfg(not(any(unix, windows)))]
fn almide_http_signals_disarm(_: ()) {}

// Handler-as-closure wrapper for `@intrinsic` migration of `http.serve`.
// The Almide side passes a `(Request) -> Response` closure; this wrapper
// composes it with `Ok(...)` so the inner `almide_http_serve` keeps its
// `Result<Response, String>` contract (future error-in-handler support).
pub fn almide_rt_http_serve(
    port: i64,
    handler: std::rc::Rc<dyn Fn(AlmideHttpRequest) -> Result<AlmideHttpResponse, AlmideString>>,
) -> Result<(), AlmideString> {
    let __almide_r: Result<(), String> = (|| {
        // #1055: the handler slot is effect-typed, so the checker already hands
        // us the carrier shape — a handler `Err` becomes the listener loop's 500.
        almide_http_serve(port, handler)
    })();
    __almide_r.map_err(Into::into)
}

// ── Helpers ──





/// Percent-decode an `application/x-www-form-urlencoded` component: `+` → space,
/// `%XX` → the raw byte 0xXX. Bytes are gathered first then interpreted as UTF-8
/// (lossy) so multi-byte sequences like `%E7%8C%AB` (猫) round-trip. Malformed or
/// truncated `%` escapes pass through verbatim.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => { out.push(b' '); i += 1; }
            c => { out.push(c); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
