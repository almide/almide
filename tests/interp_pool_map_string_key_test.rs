//! A self-hosted body the interp's pool tier runs builds a `Map[String, _]`
//! the way user code does — `map.insert(seen, k, v)` with a repeated key — and
//! must keep ONE entry per key, as both backends do. The body that first
//! exposed it was `http.query_params`'s old `__qp_fill`, which handed the third
//! oracle `["q": "a", "q": "c"]` for `?q=a&q=c`. That decoder is gone
//! (`http.query_params` is `url.parse_query`, C-404); the same pool-tier path
//! now runs in `value.object`'s fold of a repeated key (C-401, first position,
//! last value) and in `json.parse`'s duplicate-key check.

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
        "fn main() -> Unit = {\n\
         \x20 let v = value.object([(\"q\", value.str(\"a\")), (\"q\", value.str(\"c\"))])\n\
         \x20 println(\"${value.keys(v)} ${value.stringify(v)}\")\n\
         }\n",
        "[\"q\"] {\"q\":\"c\"}\n",
    );
}

#[test]
fn a_pool_body_keeps_distinct_string_keys_apart() {
    assert_agrees_or_abstains(
        "fn main() -> Unit = {\n\
         \x20 let w = value.object([(\"a\", value.int(1)), (\"b\", value.str(\"a b\")), (\"a\", value.int(3))])\n\
         \x20 println(\"${value.keys(w)} ${value.stringify(w)}\")\n\
         }\n",
        "[\"a\", \"b\"] {\"a\":3,\"b\":\"a b\"}\n",
    );
}

#[test]
fn query_params_keeps_every_pair_in_order() {
    assert_agrees_or_abstains(
        "import http\n\
         fn main() -> Unit = {\n\
         \x20 let ps = http.query_params(http.new_request(\"GET\", \"/s?a=1&b=a%20b&a=3\", \"\", [])) ?? []\n\
         \x20 println(\"${ps}\")\n\
         }\n",
        "[(\"a\", \"1\"), (\"b\", \"a b\"), (\"a\", \"3\")]\n",
    );
}
