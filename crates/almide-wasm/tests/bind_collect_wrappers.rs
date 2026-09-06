//! The local-bind collector must walk EVERY expression wrapper: a `match`
//! with a pattern bind beneath `r?` (ToOption) or `o?.field`
//! (OptionalChain) surfaced as `bind:unmapped` on the structural leg — and,
//! the incumbent walling the same draws for its own reasons, on BOTH legs
//! (composition fuzz family, seed 7 draws 11 / 21 / 30 / 57 / 88 / 98 / 103).

mod harness;
use harness::run_wasm;

const TO_OPTION_OVER_MATCH: &str = r#"fn w_some_tup(x: (Int, String)) -> (Int, String)? = some(x)

fn w_ok_tup(x: (Int, String)) -> Result[(Int, String), String] = ok(x)

fn probe_c() -> (Int, String) = w_ok_tup(match w_some_tup((7, "t")) {
  some(v) => v,
  none => (8, "t"),
})? ?? (9, "t")

fn probe_d() -> Int = w_ok_tup(match w_some_tup((5, "u")) {
  some(v) => (v.0 + 1, v.1),
  none => (0, "u"),
})? ?? (9, "t") |> ((t) => t.0)

effect fn main() -> Unit = println("c=${probe_c().0} d=${probe_d()}")
"#;

const OPTIONAL_CHAIN_OVER_MATCH: &str = r#"type P = { n: Int, s: String }

fn w_some_rec(p: P) -> P? = some(p)

fn probe_e() -> Int = (match w_some_rec(P { n: 3, s: "r" }) {
  some(v) => some(v),
  none => none,
})?.n ?? 9

effect fn main() -> Unit = println("e=${probe_e()}")
"#;

fn run(name: &str, src: &str) -> String {
    let ir = almide_spine::s5::lower_to_ir(name, src).expect("front end");
    let bytes = almide_wasm::emit_program(&ir)
        .unwrap_or_else(|e| panic!("the structural leg must lower {name}: {e:?}"));
    let out = run_wasm(&bytes).expect("wasmtime run");
    assert_eq!(out.exit, 0, "{name} stderr: {}", out.stderr);
    out.stdout
}

#[test]
fn a_pattern_bind_beneath_to_option_is_mapped() {
    assert_eq!(run("to_option.almd", TO_OPTION_OVER_MATCH), "c=7 d=6\n");
}

#[test]
fn a_pattern_bind_beneath_optional_chain_is_mapped() {
    assert_eq!(run("optional_chain.almd", OPTIONAL_CHAIN_OVER_MATCH), "e=3\n");
}
