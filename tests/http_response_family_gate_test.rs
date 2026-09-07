//! #1791 family gate: the `*_response` twins exist for EXACTLY the
//! verb-shaped http String clients. Every verb (`get` / `post` / `put` /
//! `patch` / `delete` / `request`) has one `<verb>_response` twin with the
//! SAME parameter list returning `Result[HttpResponse, String]`, and no
//! twin grows outside the verb set — so the family's completeness rule
//! (stdlib/http.almd comment) stays machine-enforced (CLAUDE.md: extended
//! by matrix, never point-wise). The intentional omissions are pinned too:
//! `get_status` / `get_bytes` / `request_bytes` are result-SHAPE variants,
//! not verbs, and `request_stream` carries no response record.

use almide_frontend::bundled_sigs;

const VERBS: &[&str] = &["get", "post", "put", "patch", "delete", "request"];

#[test]
fn every_http_verb_has_its_response_twin_with_the_same_params() {
    for verb in VERBS {
        let body = bundled_sigs::lookup("http", verb)
            .unwrap_or_else(|| panic!("http.{verb} is a verb cell of the String client family"));
        assert_eq!(
            body.ret.display(),
            "Result[String, String]",
            "http.{verb} must stay the body-only String client"
        );
        let twin_name = format!("{verb}_response");
        let twin = bundled_sigs::lookup("http", &twin_name)
            .unwrap_or_else(|| panic!("family cell missing: http.{twin_name} (the twin of http.{verb})"));
        assert_eq!(
            twin.ret.display(),
            "Result[HttpResponse, String]",
            "http.{twin_name} must return the response record"
        );
        assert_eq!(
            body.params, twin.params,
            "http.{twin_name} must take exactly http.{verb}'s parameters"
        );
        assert!(twin.is_effect, "http.{twin_name} is a network call — effect fn");
    }
}

#[test]
fn no_response_twin_grows_outside_the_verb_set() {
    let twins: Vec<&str> = bundled_sigs::module_fn_names("http")
        .into_iter()
        .filter(|f| f.ends_with("_response"))
        .collect();
    assert_eq!(
        twins.len(),
        VERBS.len(),
        "the *_response family changed size ({twins:?}) — update the family rule \
         (stdlib/http.almd comment + this gate) in the same PR"
    );
    for twin in twins {
        let verb = twin.strip_suffix("_response").unwrap();
        assert!(VERBS.contains(&verb), "http.{twin} has no verb cell http.{verb}");
    }
    // The intentional omissions stay omitted: result-shape variants and the
    // streaming client are not verbs.
    for not_a_verb in ["get_status_response", "get_bytes_response", "request_bytes_response", "request_stream_response"] {
        assert!(bundled_sigs::lookup("http", not_a_verb).is_none(), "http.{not_a_verb} is outside the family rule");
    }
}
