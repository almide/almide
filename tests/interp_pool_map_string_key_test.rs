//! A self-hosted body the interp's pool tier runs builds a `Map[String, _]`
//! the way user code does — `map.set(acc, k, v)` with a repeated key — and
//! must keep ONE entry per key, last value winning, as both backends do.
//! `http.query_params` (`__qp_fill` in stdlib/http_url_decode.almd) handed
//! the third oracle `["q": "a", "q": "c"]` for `?q=a&q=c` where native and
//! wasm print `["q": "c"]`: every fuzz program that printed a request's
//! query map read as "both targets disagree with the reference interpreter".

fn run(source: &str) -> almide_interp::RunOutcome {
    let ir = almide::wasm_leg::lower_to_ir("spec/wasm_cross/probe.almd", source).expect("front failed");
    almide_interp::Interpreter::new(&ir).run_main()
}

/// The interp either answers what both backends answer or abstains; a
/// wrong value is the one outcome it may not give.
fn assert_agrees_or_abstains(source: &str, expected: &str) {
    let out = run(source);
    match out.status {
        almide_interp::RunStatus::Ok => assert_eq!(out.stdout, expected, "the interp's vote differs from both backends"),
        _ => assert!(out.stdout.is_empty() || expected.starts_with(&out.stdout), "partial wrong output: <{}>", out.stdout),
    }
}

#[test]
fn a_pool_body_keeps_one_entry_per_repeated_string_key() {
    assert_agrees_or_abstains(
        "import http\n\
         fn main() -> Unit = {\n\
         \x20 let m = http.query_params(http.new_request(\"GET\", \"/s?q=a&q=c\", \"\", map.new()))\n\
         \x20 println(\"${map.len(m)} ${map.get(m, \"q\")} ${map.keys(m)}\")\n\
         }\n",
        "1 some(\"c\") [\"q\"]\n",
    );
}

#[test]
fn a_pool_body_keeps_distinct_string_keys_apart() {
    assert_agrees_or_abstains(
        "import http\n\
         fn main() -> Unit = {\n\
         \x20 println(\"${http.query_params(http.new_request(\"GET\", \"/s?a=1&b=a%20b&a=3\", \"\", map.new()))}\")\n\
         }\n",
        "[\"a\": \"3\", \"b\": \"a b\"]\n",
    );
}
