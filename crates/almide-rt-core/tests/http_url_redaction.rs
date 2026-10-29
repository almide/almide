//! C-328 (#4205): an HTTP client error never quotes the userinfo password —
//! split from http_client_audit.rs (the 800-line file budget).

use almide_rt_core::http_client_core as client;

#[test]
fn an_error_never_quotes_the_userinfo_password() {
    // C-328 (#4205): the password of the userinfo reads `***` wherever an
    // error names the URL — Go net/http's stripPassword — and every other
    // byte (user, host, port, path, query, fragment) is the program's.
    for (url, want) in [
        ("http://alice:s3cret@h:9/p?token=abc&x=1#frag", "http://alice:***@h:9/p?token=abc&x=1#frag"),
        ("http://alice:@h/p", "http://alice:***@h/p"),
        ("http://alice@h/p", "http://alice@h/p"),
        ("http://a:b:c@d@h/p", "http://a:***@h/p"),
        ("http://h/p?u=a:b@c#x:y@z", "http://h/p?u=a:b@c#x:y@z"),
        ("alice:s3cret@h/p", "alice:***@h/p"),
        ("x?u=http://a:b@c", "x?u=http://a:b@c"),
        ("ftp://u:s3cret@h/x", "ftp://u:***@h/x"),
        ("http://h", "http://h"),
        ("", ""),
    ] {
        assert_eq!(client::http_redact_url(url), want, "{url}");
    }
    let url = "http://alice:s3cret@127.0.0.1:9/p?token=abc";
    let shown = "\"http://alice:***@127.0.0.1:9/p?token=abc\"";
    for class in [
        client::HttpErrorClass::Dns,
        client::HttpErrorClass::Connect,
        client::HttpErrorClass::Timeout,
        client::HttpErrorClass::Tls,
        client::HttpErrorClass::Protocol,
    ] {
        let text = client::http_error_text(class, url);
        assert!(text.contains(shown), "{class:?}: {text}");
    }
    assert!(client::http_too_large_text(url, 7).contains(shown));
    // Every invalid-URL reason names the URL the same way.
    for bad in ["ftp://alice:s3cret@h/x", "HTTP:alice:s3cret@h", "alice:s3cret@h/x", "http://alice:s3cret@h:99999/", "http://alice:s3cret@h\\x/"] {
        let e = client::request_response("GET", bad, "", &[]).expect_err(bad);
        assert!(!e.contains("s3cret"), "{bad}: {e}");
        assert!(e.contains(":***@"), "{bad}: {e}");
    }
    // A live refusal, end to end.
    let e = client::request_response("GET", "http://alice:s3cret@127.0.0.1:9/p?token=abc", "", &[]).expect_err("port 9 refuses");
    assert_eq!(e, format!("could not connect to {shown} (connection refused or unreachable)"));
}
