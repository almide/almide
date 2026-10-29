//! Security row 11e, ADR-0032 (C-275): a fetched HttpResponse keeps its own
//! body type. Re-served unchanged (a proxy) it stays text/html, a header added
//! to it stays allowed, a String written into it makes it text/plain, and
//! writing Content-Type by name aborts. Runs spec/native_only/http_proxy_html_passthrough.almd, which
//! fetches a canned text/html response over loopback; the full-response
//! client family is native-only, so this is the one leg that runs it.
#![cfg(unix)]
use std::process::Command;

/// `ALMIDE_BIN` runs it against another build (A/B); the default is this
/// workspace's binary.
fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

#[test]
fn a_fetched_html_response_keeps_its_type_until_its_body_is_replaced() {
    let file = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/native_only/http_proxy_html_passthrough.almd");
    let out = Command::new(almide()).arg("run").arg(file).output().expect("run almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stdout,
        "status 200\ncontent-type text/html; charset=utf-8\nvia 1.1 almide-proxy\nbody <p>upstream</p>\nrewritten text/plain; charset=utf-8\nrelabelling\n",
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("PANIC: http.set_header: `Content-Type` is written from the body's type, never by name"),
        "stderr:\n{stderr}"
    );
    assert_eq!(out.status.code(), Some(1), "stderr:\n{stderr}");
}
