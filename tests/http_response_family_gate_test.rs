//! R-http-01 family gate: the one-shot http client is a 2×2 matrix, machine-
//! checked so it cannot grow point-wise (CLAUDE.md: extended by matrix, never
//! point-wise). The rows say how the request is named — by its URL alone (a
//! GET) or in full — and the columns what is answered: the body text or the
//! whole response. The raw body bytes are `http.body_bytes` of the response
//! (R-http-02), not a third column.
//!
//! |             | text      | response       |
//! |-------------|-----------|----------------|
//! | URL (GET)   | `get`     | `get_response` |
//! | any request | `request` | `send`         |
//!
//! `request` takes `(method, url, body, headers)`; `send` takes a built
//! `HttpRequest` (`new_request`), so a request with a bytes body is sent
//! through it. The verb shortcuts (`post`, `put`, …) and the
//! `*_response` / `*_status` twins are gone, and so are the callback streaming
//! clients — the call handle (`start`) reads a body as it arrives.

use almide_frontend::bundled_sigs;

const TEXT: &str = "Result[String, String]";
const RESPONSE: &str = "Result[HttpResponse, String]";

/// The matrix: (fn, return).
const CELLS: &[(&str, &str)] = &[
    ("get", TEXT),
    ("get_response", RESPONSE),
    ("request", TEXT),
    ("send", RESPONSE),
];

fn params(name: &str) -> Vec<(String, String)> {
    let s = bundled_sigs::lookup("http", name).unwrap_or_else(|| panic!("family cell missing: http.{name}"));
    s.params.iter().map(|(n, t)| (n.to_string(), t.display())).collect()
}

#[test]
fn every_cell_of_the_client_matrix_has_its_shape() {
    for (name, ret) in CELLS {
        let s = bundled_sigs::lookup("http", name).unwrap_or_else(|| panic!("family cell missing: http.{name}"));
        assert_eq!(s.ret.display(), *ret, "http.{name} return");
        assert!(s.is_effect, "http.{name} is a network call — effect fn");
    }
    let url = vec![("url".to_string(), "String".to_string())];
    for name in ["get", "get_response"] {
        assert_eq!(params(name), url, "http.{name} takes the URL alone");
    }
    let full = params("request");
    let names: Vec<&str> = full.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["method", "url", "body", "headers"], "http.request's parameters");
    // The header list is new_request's: one header shape across the module.
    assert_eq!(full[3].1, params("new_request")[3].1, "request's headers are new_request's");
    assert_eq!(params("send"), vec![("req".to_string(), "HttpRequest".to_string())], "http.send takes a built request");
}

#[test]
fn no_client_grows_outside_the_matrix() {
    let effect_clients: Vec<&str> = bundled_sigs::module_fn_names("http")
        .into_iter()
        .filter(|f| !f.starts_with("__"))
        .filter(|f| {
            let s = bundled_sigs::lookup("http", f).expect("listed");
            s.is_effect && [TEXT, RESPONSE, "Result[Bytes, String]"].contains(&s.ret.display().as_str()) && !s.params.iter().any(|(_, t)| t.display() == "HttpCall")
        })
        .collect();
    let mut want: Vec<&str> = CELLS.iter().map(|(n, _)| *n).collect();
    want.sort_unstable();
    let mut got = effect_clients.clone();
    got.sort_unstable();
    assert_eq!(got, want, "a one-shot client outside the 2×2 matrix — update the family rule (stdlib/http.almd comment + this gate) in the same PR");
    for gone in [
        "post", "put", "patch", "delete", "get_status", "request_status", "post_response", "put_response",
        "patch_response", "delete_response", "request_response", "request_stream", "request_stream_with_limits",
        "get_bytes", "request_bytes",
    ] {
        assert!(bundled_sigs::lookup("http", gone).is_none(), "http.{gone} is outside the family rule");
    }
}
