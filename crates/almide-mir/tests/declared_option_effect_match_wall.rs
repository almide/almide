//! #2308: a never-err effect fn DECLARED `-> T?` returns the raw Option on the
//! incumbent leg while its call is typed as the lifted `Result[Option[T], String]`.
//! `match f(x) { ok(v) => .., err(e) => .. }` is rewritten to a bind of the raw
//! Option and runs (spec/wasm_cross/fallible_hof_option_effect_callback.almd pins
//! it on every leg). A match the rewrite cannot take — a structured Ok pattern —
//! has no Result tag to dispatch on, so it must WALL like its lifted twin rather
//! than read the Option block as a Result: before the fix this shape printed
//! `none` for `some(3)` and then trapped in `rc_dec`.

fn render(src: &str) -> Result<String, almide_mir::lower::LowerError> {
    almide_mir::pipeline::try_render_wasm_source(src, &[], false)
}

const WALL: &str = "match over a never-err effect-fn call";

#[test]
fn structured_ok_pattern_over_a_declared_option_effect_call_walls() {
    let src = r#"effect fn third(x: Int) -> Int? = if x % 3 == 0 then some(x) else none

effect fn main() -> Unit = {
  match third(3) {
    ok(some(v)) => println("some=${v}"),
    ok(none) => println("none"),
    err(e) => println("err=${e}"),
  }
}
"#;
    let e = render(src).expect_err("the un-rewritable match must wall, not read the Option as a Result");
    assert!(e.reason().contains(WALL), "the wall must name the construct, got: {}", e.reason());
}

#[test]
fn stripped_and_bind_shaped_matches_keep_lowering() {
    // `third(3)!` is the Option itself (the wall is keyed on the Result type), and
    // `ok(v)` is the shape the rewrite turns into a bind.
    let src = r#"effect fn third(x: Int) -> Int? = if x % 3 == 0 then some(x) else none

effect fn main() -> Unit = {
  match third(3)! {
    some(v) => println("some=${v}"),
    none => println("none"),
  }
  match third(4) {
    ok(v) => println("ok=${v}"),
    err(e) => println("err=${e}"),
  }
}
"#;
    if let Err(e) = render(src) {
        panic!("both matches must lower on the incumbent, got: {}", e.reason());
    }
}
