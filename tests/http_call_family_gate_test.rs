//! #2631 family gate: the http call handle and the `_with_limits` streaming
//! twins, machine-checked so the surface cannot grow point-wise (CLAUDE.md:
//! API families are extended by matrix). The rule, stated in
//! stdlib/http.almd:
//!
//! 1. The handle has EXACTLY five verbs — `start` / `poll` / `read_new` /
//!    `wait` / `cancel` — every one an effect fn, and every http fn that takes
//!    an `HttpCall` is one of the four after `start`.
//! 2. `start` takes `request_response`'s parameters plus `limits: HttpLimits`,
//!    and `wait` answers exactly what `request_response` answers.
//! 3. Every streaming client has exactly one `<name>_with_limits` twin: the
//!    same parameters with `limits: HttpLimits` inserted before the callback,
//!    the same return type. No `_with_limits` fn exists outside that set.
//! 4. Intentional omission: the one-shot clients get no twin — `start` +
//!    `wait` is their limited form.

use almide_frontend::bundled_sigs;
use almide_lang::types::FnSig;

const STREAMING: &[&str] = &["request_stream", "openai_streaming_call", "anthropic_streaming_call"];
const VERBS: &[&str] = &["start", "poll", "read_new", "wait", "cancel"];

fn sig(name: &str) -> FnSig {
    bundled_sigs::lookup("http", name).unwrap_or_else(|| panic!("family cell missing: http.{name}"))
}

fn params(s: &FnSig) -> Vec<(String, String)> {
    s.params.iter().map(|(n, t)| (n.to_string(), t.display())).collect()
}

/// A verb's expected shape: name, (param, type) list, return type.
type VerbShape = (&'static str, &'static [(&'static str, &'static str)], &'static str);

fn limits() -> (String, String) {
    ("limits".to_string(), "HttpLimits".to_string())
}

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
fn start_is_request_response_plus_limits_and_wait_answers_like_it() {
    let rr = sig("request_response");
    let mut want = params(&rr);
    want.push(limits());
    assert_eq!(params(&sig("start")), want, "http.start = request_response's parameters + limits");
    assert_eq!(sig("wait").ret, rr.ret, "http.wait answers the request_response record");
}

#[test]
fn every_streaming_client_has_one_with_limits_twin() {
    for base in STREAMING {
        let b = sig(base);
        let twin = sig(&format!("{base}_with_limits"));
        let mut want = params(&b);
        let callback = want.pop().expect("a streaming client ends with its callback");
        assert!(callback.1.starts_with("fn("), "http.{base}'s last parameter is the callback, got {callback:?}");
        want.push(limits());
        want.push(callback);
        assert_eq!(params(&twin), want, "http.{base}_with_limits = {base}'s parameters with limits before the callback");
        assert_eq!(twin.ret, b.ret, "http.{base}_with_limits answers what {base} answers");
        assert!(twin.is_effect);
    }
    let twins: Vec<&str> = bundled_sigs::module_fn_names("http")
        .into_iter()
        .filter(|f| f.ends_with("_with_limits"))
        .collect();
    assert_eq!(twins.len(), STREAMING.len(), "the _with_limits family changed size ({twins:?}) — update the family rule and this gate together");
    // The one-shot clients stay without a twin: start + wait is their form.
    for one_shot in ["get", "request", "request_response", "request_status", "request_bytes", "get_response"] {
        assert!(sig(one_shot).params.iter().all(|(n, _)| n.as_str() != "limits"));
        assert!(bundled_sigs::lookup("http", &format!("{one_shot}_with_limits")).is_none(), "http.{one_shot}_with_limits is outside the family rule");
    }
}
