//! The OpenAI-compatible streaming helper on the wasm target (ADR-0023 step
//! 5): `openai_streaming_call_with_limits` is Almide over the call handle's
//! streaming client, so the embedded lane (`almide run --target wasm`)
//! answers what native answers — every delta in order, the accumulated
//! LLM-response JSON (content, model, usage, finish reason, tool calls built
//! from fragments), the end at `[DONE]`, a non-2xx status and a refused
//! connection. The plain `openai_streaming_call` is the same body over
//! `request_stream`; it is pinned on native here, since `request_stream`
//! has no wasm service yet.
//!
//! The server is a loopback listener that writes one canned SSE stream in
//! pieces cut mid-event and mid-character, so the event split and the
//! UTF-8 carry are both exercised. No network is needed.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Read one request head and its Content-Length body; answer the request
/// line and the `Authorization` value.
fn read_request(s: &mut TcpStream) -> (String, String) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
        let n = s.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break buf.len();
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let field = |name: &str| {
        head.lines()
            .find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()))
            .unwrap_or_default()
    };
    let len = field("content-length").parse::<usize>().unwrap_or(0);
    while buf.len() < head_end + len {
        let n = s.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    (head.lines().next().unwrap_or("").to_string(), format!("{}|{}", field("authorization"), field("accept")))
}

/// The canned stream: deltas with multibyte text, a tool call whose
/// arguments arrive in fragments, a call at index 2 (growing the list past
/// a gap), an event with no data, usage with a Float count, `[DONE]`, and
/// an event after it that must not be applied.
fn stream(auth: &str) -> String {
    [
        format!("data: {{\"model\":\"{auth}\",\"choices\":[{{\"delta\":{{\"content\":\"h\u{e9}llo\"}}}}]}}\n\n"),
        "data: {\"choices\":[{\"delta\":{\"content\":\" w\u{f6}rld \u{1f600}\",\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"get_w\",\"arguments\":\"{\\\"ci\"}}]}}]}\n\n".to_string(),
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"ty\\\":\\\"x\\\"}\"}},{\"index\":2,\"id\":\"c3\",\"function\":{\"name\":\"n3\",\"arguments\":\"\\u0001\\n\\\"q\\\"\"}}]}}]}\n\n".to_string(),
        "event: ping\n\n".to_string(),
        "data: {\"choices\":[{\"finish_reason\":\"tool_calls\",\"delta\":{}}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":7.9}}\n\n".to_string(),
        "data: [DONE]\n\n".to_string(),
        "data: {\"choices\":[{\"delta\":{\"content\":\"AFTER\"}}]}\n\n".to_string(),
    ]
    .concat()
}

fn answer(mut s: TcpStream) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let (line, auth) = read_request(&mut s);
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    if path != "/v1/chat/completions" {
        let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnope");
        return;
    }
    let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
    let body = stream(&auth).into_bytes();
    // Cut points: inside the first event's JSON, inside the two-byte `é`,
    // inside the four-byte emoji, and between the `\n\n` of an event.
    let e = body.windows(2).position(|w| w == "\u{e9}".as_bytes()).expect("é") + 1;
    let emoji = body.windows(4).position(|w| w == "\u{1f600}".as_bytes()).expect("emoji") + 2;
    let nl = body.windows(2).position(|w| w == b"\n\n").expect("blank line") + 1;
    let mut cuts = vec![10, e, nl, emoji, body.len() / 2, body.len()];
    cuts.sort_unstable();
    cuts.dedup();
    let mut at = 0;
    for cut in cuts {
        let _ = s.write_all(&body[at..cut]);
        let _ = s.flush();
        std::thread::sleep(Duration::from_millis(30));
        at = cut;
    }
}

/// Serve the stream on a loopback port until the test ends.
fn serve() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || answer(s));
        }
    });
    port
}

/// A port nothing listens on.
fn closed_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    l.local_addr().expect("addr").port()
}

/// The three exchanges through `call` — `limited` (the `_with_limits` twin)
/// or `plain`.
fn program(call: &str, port: u16, dead: u16) -> String {
    let (name, limits) = match call {
        "limited" => ("openai_streaming_call_with_limits", ", { total_ms: 20000, idle_ms: 0 }"),
        _ => ("openai_streaming_call", ""),
    };
    format!(
        r#"import http

effect fn show(tag: String, base: String) -> Unit = {{
  match http.{name}(base, "k-1", "{{\"stream\":true}}"{limits}, (d) => println("${{tag}} delta[${{d}}]")) {{
    ok(j) => println("${{tag}} ok ${{j}}"),
    err(e) => println("${{tag}} err ${{e}}"),
  }}
}}

effect fn main() -> Unit = {{
  show("stream", "http://127.0.0.1:{port}/v1//")!
  show("missing", "http://127.0.0.1:{port}/nope")!
  show("closed", "http://127.0.0.1:{dead}/v1")!
  println("done")
}}
"#
    )
}

fn observe(mut c: Command) -> (String, String, Option<i32>) {
    let out = c.stdin(Stdio::null()).output().expect("spawn");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned(), out.status.code())
}

fn run(file: &Path, wasm: bool) -> (String, String, Option<i32>) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    observe(c)
}

#[test]
fn the_openai_streaming_helper_reads_the_same_on_native_and_the_embedded_lane() {
    let port = serve();
    let dead = closed_port();
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("sse_limited.almd");
    std::fs::write(&file, program("limited", port, dead)).expect("write");
    let native = run(&file, false);
    assert_eq!(native.2, Some(0), "native failed: {}", native.1);
    // Pin native's own shape, so a change cannot pass by moving both legs.
    let expected_json = "stream ok {\"content\":\"h\u{e9}llo w\u{f6}rld \u{1f600}\",\"model\":\"Bearer k-1|text/event-stream\",\
\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":7,\"total_tokens\":12},\"finish_reason\":\"tool_calls\",\
\"tool_calls\":[{\"id\":\"call_1\",\"name\":\"get_w\",\"arguments\":\"{\\\"city\\\":\\\"x\\\"}\"},\
{\"id\":\"\",\"name\":\"\",\"arguments\":\"\"},{\"id\":\"c3\",\"name\":\"n3\",\"arguments\":\"\\u0001\\n\\\"q\\\"\"}]}\n";
    assert!(native.0.starts_with("stream delta[h\u{e9}llo]\nstream delta[ w\u{f6}rld \u{1f600}]\n"), "{}", native.0);
    assert!(native.0.contains(expected_json), "{}", native.0);
    assert!(!native.0.contains("AFTER"), "{}", native.0);
    assert!(native.0.contains("missing err HTTP 404: nope\n"), "{}", native.0);
    assert!(native.0.contains("closed err could not connect to "), "{}", native.0);
    let embedded = run(&file, true);
    assert_eq!((&embedded.0, embedded.2), (&native.0, native.2), "the embedded lane diverged: {}", embedded.1);
}

/// The plain helper is the same accumulation over `request_stream` (no
/// per-call limits): native answers exactly what the limited twin answers.
#[test]
fn the_plain_openai_helper_answers_what_its_limited_twin_answers() {
    let port = serve();
    let dead = closed_port();
    let dir = tempfile::tempdir().expect("tempdir");
    let limited = dir.path().join("sse_limited.almd");
    let plain = dir.path().join("sse_plain.almd");
    std::fs::write(&limited, program("limited", port, dead)).expect("write");
    std::fs::write(&plain, program("plain", port, dead)).expect("write");
    let a = run(&limited, false);
    let b = run(&plain, false);
    assert_eq!(a.2, Some(0), "limited failed: {}", a.1);
    assert_eq!((&b.0, b.2), (&a.0, a.2), "the plain helper diverged: {}", b.1);
}
