//! A declared floor call returning a scalar Option (`list.first` over an
//! Int list, `bytes.get`, `string.to_codepoint`) binds the native
//! `Option<i64>` carrier, so its `?? d` window renders through the carrier
//! reads (#4006 family B). The rewrite used to seed Options from user
//! declarations only, so these windows reached the render as raw
//! `Prim Handle` over an `Opt` value and walled.

fn render(src: &str) -> String {
    almide_mir::pipeline::try_render_rust_source(src).unwrap_or_else(|e| panic!("walled: {e:?}"))
}

#[test]
fn an_element_option_from_the_floor_renders_through_the_carrier() {
    let rust = render(
        r#"fn first_or(xs: List[Int], d: Int) -> Int = list.first(xs) ?? d

effect fn main() -> Unit = {
  println(int.to_string(first_or([4, 5], 9) + first_or([], 7)))
}
"#,
    );
    assert!(rust.contains(".is_some() as i64"), "no carrier tag read:\n{rust}");
}

#[test]
fn a_scalar_option_from_the_floor_renders_through_the_carrier() {
    render(
        r#"fn cp(s: String) -> Int = string.to_codepoint(s) ?? 0

effect fn main() -> Unit = {
  println(int.to_string(cp("A") + cp("")))
}
"#,
    );
}
