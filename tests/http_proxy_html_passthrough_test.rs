//! Security row 11e, owner ruling 2026-10-29 (C-275): a fetched HttpResponse
//! re-served unchanged (a proxy) stays allowed, a header added to it stays
//! allowed, and writing a String body into it is refused like any HTML
//! response's. Runs spec/native_only/http_proxy_html_passthrough.almd, which
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
fn a_fetched_html_response_passes_through_and_its_body_cannot_be_rewritten() {
    let file = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/native_only/http_proxy_html_passthrough.almd");
    let out = Command::new(almide()).arg("run").arg(file).output().expect("run almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stdout,
        "status 200\ncontent-type text/html; charset=utf-8\nvia 1.1 almide-proxy\nbody <p>upstream</p>\nrewriting the body\n",
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("PANIC: http.set_body: this response serves HTML or XML, so its body is SafeHtml"),
        "stderr:\n{stderr}"
    );
    assert_eq!(out.status.code(), Some(1), "stderr:\n{stderr}");
}
