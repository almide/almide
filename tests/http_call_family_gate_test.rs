//! #2631 family gate: the http call handle, machine-checked so the surface
//! cannot grow point-wise (CLAUDE.md: API families are extended by matrix).
//! The rule, stated in stdlib/http.almd:
//!
//! 1. The handle has EXACTLY five verbs — `start` / `poll` / `read_new` /
//!    `wait` / `cancel` — every one an effect fn, and every http fn that takes
//!    an `HttpCall` is one of the four after `start`.
//! 2. `start` takes what `send` takes plus `limits: HttpLimits` (R-http-14:
//!    one way to build a request), and `wait` answers exactly what `send`
//!    answers.
//! 3. There are no callback streaming clients (R-http-01): a streamed body is
//!    read with `read_new`, so no `_with_limits` twin exists, and the one-shot
//!    clients get none either — `start` + `wait` is their limited form.

use almide_frontend::bundled_sigs;
use almide_lang::types::FnSig;

const VERBS: &[&str] = &["start", "poll", "read_new", "wait", "cancel"];
const ONE_SHOT: &[&str] = &["get", "request", "get_response", "send", "get_bytes", "request_bytes"];

fn sig(name: &str) -> FnSig {
    bundled_sigs::lookup("http", name).unwrap_or_else(|| panic!("family cell missing: http.{name}"))
}

fn params(s: &FnSig) -> Vec<(String, String)> {
    s.params.iter().map(|(n, t)| (n.to_string(), t.display())).collect()
}

/// A verb's expected shape: name, (param, type) list, return type.
type VerbShape = (&'static str, &'static [(&'static str, &'static str)], &'static str);

#[test]
fn the_handle_has_exactly_its_five_verbs() {
    let expected: &[VerbShape] = &[
        ("poll", &[("c", "HttpCall")], "Option[Result[HttpResponse, String]]"),
        ("read_new", &[("c", "HttpCall")], "String"),
        ("wait", &[("c", "HttpCall")], "Result[HttpResponse, String]"),
        ("cancel", &[("c", "HttpCall")], "Unit"),
    ];
    for (name, ps, ret) in expected {
        let s = sig(name);
        let want: Vec<(String, String)> = ps.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect();
        assert_eq!(params(&s), want, "http.{name} parameters");
        assert_eq!(s.ret.display(), *ret, "http.{name} return");
        assert!(s.is_effect, "http.{name} touches a live connection — effect fn");
    }
    let start = sig("start");
    assert_eq!(start.ret.display(), "Result[HttpCall, String]");
    assert!(start.is_effect);

    // Every public http fn that takes a handle is a verb.
    let takers: Vec<&str> = bundled_sigs::module_fn_names("http")
        .into_iter()
        .filter(|f| !f.starts_with("__"))
        .filter(|f| sig(f).params.iter().any(|(_, t)| t.display() == "HttpCall"))
        .collect();
    let mut want: Vec<&str> = VERBS.iter().copied().filter(|v| *v != "start").collect();
    want.sort_unstable();
    assert_eq!(takers, want, "an HttpCall-taking fn outside the five verbs — update the family rule and this gate together");
}

#[test]
fn start_is_send_plus_limits_and_wait_answers_like_it() {
    let send = sig("send");
    let mut want = params(&send);
    want.push(("limits".to_string(), "HttpLimits".to_string()));
    assert_eq!(params(&sig("start")), want, "http.start = send's parameters + limits");
    assert_eq!(sig("wait").ret, send.ret, "http.wait answers the send record");
}

#[test]
fn no_client_has_a_with_limits_twin() {
    let twins: Vec<&str> = bundled_sigs::module_fn_names("http")
        .into_iter()
        .filter(|f| f.ends_with("_with_limits"))
        .collect();
    assert!(twins.is_empty(), "a _with_limits fn ({twins:?}) is outside the family rule — start + wait is the limited form");
    for one_shot in ONE_SHOT {
        assert!(sig(one_shot).params.iter().all(|(n, _)| n.as_str() != "limits"), "http.{one_shot} takes no limits");
    }
    // The callback clients are gone (R-http-01).
    for gone in ["request_stream", "request_stream_with_limits"] {
        assert!(bundled_sigs::lookup("http", gone).is_none(), "http.{gone} is outside the family rule");
    }
}
