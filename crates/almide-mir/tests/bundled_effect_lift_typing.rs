//! A bundled stdlib effect fn with an Almide body (`args.option`,
//! `env.secret`) is lifted to `Result[T, String]` by both backends, so its
//! call types as a Result. The single-file render front end checks with no
//! modules loaded, and it used to type the call as the raw `T` there, so the
//! CLI-accepted `args.option("out")! ?? "NONE"` walled as a type error on the
//! native render (args_surface, bundled_pure_modules, env_secret).

fn front_end_verdict(src: &str) -> Result<(), String> {
    almide_mir::pipeline::ownership_certificates(src)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

fn assert_no_type_error(src: &str) {
    if let Err(e) = front_end_verdict(src) {
        assert!(!e.contains("type errors"), "the render front end disagrees with the checker: {e}");
    }
}

#[test]
fn a_lifted_bundled_option_reader_types_as_a_result() {
    assert_no_type_error(
        r#"import args

effect fn main() -> Result[Unit, String] = {
  println("opt=" + (args.option("out")! ?? "NONE"))
  ok(())
}
"#,
    );
}

#[test]
fn a_lifted_bundled_secret_reader_types_as_a_result() {
    assert_no_type_error(
        r#"import env
import secret

effect fn main() -> Unit = {
  let tok = env.secret("ALMIDE_LIFT_TYPING_UNSET")!
  match tok {
    some(t) => println("${string.len(secret.reveal(t))}"),
    none => println("unset"),
  }
}
"#,
    );
}

#[test]
fn a_template_effect_fn_keeps_its_raw_return() {
    // `env.get` is `@intrinsic`: its runtime fn returns the Option itself,
    // so `?? ""` on the bare call is right and must stay accepted.
    assert_no_type_error(
        r#"import env

effect fn main() -> Unit = {
  println(env.get("ALMIDE_LIFT_TYPING_UNSET") ?? "none")
}
"#,
    );
}
