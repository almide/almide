//! #3438: a string literal holding a Unicode bidi control must reach the v1
//! native render as a `\u{XXXX}` escape. rustc's deny-by-default
//! `text_direction_codepoint_in_literal` refuses the raw code point, so a raw
//! one in the rendered source is a build failure on native while wasm runs.

fn is_bidi(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

#[test]
fn a_bidi_control_in_a_literal_renders_escaped() {
    let src = "effect fn main() -> Unit = {\n  println(\"a\\u{202A}\\u{202E}\\u{2066}\\u{2069}b\")\n}\n";
    let rust = almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("the v1 native render walled: {}", e.reason()));
    assert!(!rust.chars().any(is_bidi), "a raw bidi control reached the rendered Rust:\n{rust}");
    assert!(rust.contains("a\\u{202A}\\u{202E}\\u{2066}\\u{2069}b"), "the escape is missing:\n{rust}");
}
