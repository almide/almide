//! The #1865 wall: `fan.map(xs, <inline callback whose body propagates>)!` on
//! the incumbent leg.
//!
//! The `!`-consumed form used to be stripped by `rewrite_fan_map_pure` into a
//! `list.map` whose String-returning closure still carried a `!` — the lifted
//! body printed nondeterministic garbage bytes per element under wasmtime
//! (`map bang fs: �,^X`). The brick now REFUSES exactly that shape, with the
//! callback's span and a reason naming the construct; the UN-consumed forms
//! (`?? fb`, a match over the Result, an effect-fn VALUE callback) keep
//! lowering — they ride the self-host `fan.map` route with the callback's own
//! Result channel intact, byte-identical to native
//! (spec/wasm_cross/fan_prefetch_fs.almd). The cross-target evidence for every
//! spelling is spec/embedded_cross/fan_mapper_effect_callback_forms.almd.

const WALL: &str = "fan.map consumed by `!` with a propagating (!) callback body";

fn render(src: &str) -> Result<String, almide_mir::lower::LowerError> {
    almide_mir::pipeline::try_render_wasm_source(src, &[], false)
}

#[test]
fn consumed_fan_map_with_propagating_lambda_walls_with_its_span() {
    let src = r#"import fs

effect fn helper(p: String) -> Result[String, String] = fs.read_text(p)

effect fn main() -> Unit = {
  let xs = fan.map(["a.txt", "b.txt"], (p) => fs.read_text(p)!)!
  println(list.join(xs, ","))
}
"#;
    let e = render(src).expect_err("the `!`-consumed propagating callback must wall");
    let reason = e.reason().to_string();
    assert!(reason.contains(WALL), "the wall must name the construct, got: {reason}");
    let span = e.span().expect("the wall must carry the callback's span");
    assert_eq!(span.line, 6, "the span must point at the callback, got line {}", span.line);
}

#[test]
fn helper_call_and_compound_spellings_wall_alike() {
    for body in ["(p) => helper(p)!", "(p) => {\n    let t = string.trim(p)\n    fs.read_text(t)!\n  }"] {
        let src = format!(
            r#"import fs

effect fn helper(p: String) -> Result[String, String] = fs.read_text(p)

effect fn main() -> Unit = {{
  let xs = fan.map(["a.txt"], {body})!
  println(list.join(xs, ","))
}}
"#
        );
        let e = render(&src).expect_err("every propagating spelling must wall under `!`");
        assert!(e.reason().contains(WALL), "got: {}", e.reason());
        assert!(e.span().is_some(), "spanless wall for the {body} spelling");
    }
}

#[test]
fn unconsumed_and_fn_value_forms_keep_lowering() {
    let src = r#"import fs

effect fn helper(p: String) -> Result[String, String] = fs.read_text(p)

effect fn main() -> Unit = {
  println(list.join(fan.map(["a.txt"], (p) => fs.read_text(p)!) ?? ["fb"], ","))
  println(list.join(fan.map(["a.txt"], helper)!, ","))
  match fan.map(["b.txt"], (p) => helper(p)!) {
    ok(vs) => println(list.join(vs, ",")),
    err(e) => println(e),
  }
}
"#;
    if let Err(e) = render(src) {
        panic!("the un-consumed / fn-value forms must stay on the incumbent, got: {}", e.reason());
    }
}
