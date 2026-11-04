//! The certified native render links a bundled stdlib body that no other
//! route serves (selfhost-link-v2 on the native leg, #4006): the body is
//! lowered and certified like a user fn, under its `std_` name.
//!
//! The link takes the bundled modules from the mir harness resolver
//! (`pipeline::bundled_self_modules`, whole modules parsed from the bundled
//! sources), not from the CLI resolver's per-program output. A mono instance
//! can reach a fn of a module the source never names — `int.from_checked` at
//! Float reaches `float.__int_exact` through its protocol pivot — and the
//! render loads that module and links the fn. These pins fail if a change to
//! how bundled modules are handed over (a resolver that prunes the fns a
//! program does not name) leaves such a body out.

fn render(src: &str) -> String {
    almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("the certified native render walled: {e:?}\n{src}"))
}

#[test]
fn a_monomorphic_bundled_body_links_under_its_std_name() {
    let rust = render("effect fn main() -> Unit = println(int.to_string(list.len(list.range_step(0, 10, 3))))\n");
    assert!(rust.contains("fn almd_std_list_range_step("), "list.range_step's body is not linked:\n{rust}");
}

#[test]
fn a_mono_instance_links_the_pivot_of_a_module_the_source_never_names() {
    let rust = render(
        "fn opt(o: Int?) -> String = match o {\n  some(v) => \"some ${v}\",\n  none => \"none\",\n}\n\n\
         effect fn main() -> Unit = println(opt(int.from_checked(12.0)))\n",
    );
    assert!(
        rust.contains("from_checked__Float(") && rust.contains("fn almd_std_float___int_exact("),
        "the mono instance or the pivot it reaches is not linked:\n{rust}"
    );
}

/// A fan arm that reaches an `@ordered` stdlib fn (ADR-0024 D3) runs its
/// arms in arm order on the standard codegen; this render runs fan arms on
/// threads, so it must decline. `env.set` reaches the render only through
/// the bundled-body link: before the link the program walled on the call,
/// after it the arms ran concurrently and the reader saw the writer's value.
#[test]
fn a_fan_arm_reaching_an_ordered_fn_declines() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/spec/wasm_cross/fan_ordered_arms.almd"))
        .expect("the fixture");
    let e = almide_mir::pipeline::try_render_rust_source(&src).expect_err("an ordered fan must not render on threads");
    assert!(format!("{e:?}").contains("@ordered"), "declined for another reason: {e:?}");
}
