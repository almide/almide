//! `string.push(s, x)` and `s = s + x` grow `s` in place on the native v1
//! render (#3639). Both reach the render as `d = __str_concat(s, x); drop s;
//! s = d`, which rendered as `let d = rt_str_concat(&s, &x); s = d.clone();`:
//! a fresh string and a full copy of `s` on every append, so a builder loop
//! was quadratic (400k pushes from a 20k start took 6.8 s on the shipped
//! binary). The pin reads what `almide build` ships — the v1 render of the
//! program — and asks that the loop body append with `push_str` and never
//! call the concat shim. The v1 differential corpus
//! (tests/native_v1_differential_test.rs, `string_push_alias`) runs the
//! aliasing shapes against the v0 pipeline.

/// The v1 native render of `src` (it must not wall).
fn render(src: &str) -> String {
    almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("the v1 native render walled: {e:?}\n{src}"))
}

/// The rendered `fn main` body.
fn main_body(rust: &str) -> &str {
    let at = rust.find("\nfn main()").expect("the render has a main");
    &rust[at..]
}

fn builder(step: &str) -> String {
    format!(
        "effect fn main() -> Unit = {{
  var s = string.repeat(\"a\", 20000)
  var i = 0
  while i < 400000 {{
    {step}
    i = i + 1
  }}
  println(int.to_string(string.len(s)))
}}
"
    )
}

#[test]
fn a_builder_loop_appends_in_place() {
    for step in ["string.push(s, \"b\")", "s = s + \"b\"", "string.push(s, int.to_string(i % 10))"] {
        let rust = render(&builder(step));
        let body = main_body(&rust);
        assert!(
            body.contains(".push_str(") && !body.contains("rt_str_concat("),
            "`{step}` must append in place, not concat a fresh copy per step:\n{body}"
        );
    }
}

#[test]
fn pushing_a_string_onto_itself_keeps_the_concat() {
    let rust = render(
        "effect fn main() -> Unit = {
  var w = \"ab\"
  string.push(w, w)
  println(w)
}
",
    );
    assert!(main_body(&rust).contains("rt_str_concat("), "`s.push_str(&s)` cannot borrow `s` twice:\n{rust}");
}
