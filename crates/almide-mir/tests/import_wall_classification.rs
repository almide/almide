//! The #1052 import-wall taxonomy gate.
//!
//! A program the real checker accepts must never wall under "type errors" —
//! that bucket is the one the walled-real ledgers audit as empty-by-
//! construction. The native rung passes no resolved siblings, so an
//! `import self.*` (or dependency) program used to fail its front-end check
//! with a cascade of "undefined function" messages misfiled as type errors.
//! It must instead wall as the FEATURE gap it is: one line, before inference,
//! naming the import. (Adjacent but distinct from #943, the linking-stage
//! wall: that one HAS the module and fails to link it.)

#[test]
fn unresolved_package_sibling_walls_as_feature_not_type_errors() {
    let src = r#"import self.otel

effect fn main() -> Unit = {
  let t = otel.tracer("demo")
  println(t)
}
"#;
    let e = almide_mir::pipeline::try_render_rust_source(src)
        .expect_err("the rung has no sibling resolver — this must wall");
    let reason = e.reason().to_string();
    assert!(
        reason.contains("import self.otel") && reason.contains("feature wall"),
        "the wall must name the import as a feature gap, got: {reason}"
    );
    assert!(
        !reason.contains("type errors"),
        "an unresolved import must never be misfiled as type errors, got: {reason}"
    );
}

#[test]
fn unresolved_dependency_import_walls_as_feature_not_type_errors() {
    let src = r#"import somepkg

effect fn main() -> Unit = {
  println(somepkg.greet("x"))
}
"#;
    let e = almide_mir::pipeline::try_render_rust_source(src)
        .expect_err("the rung has no dependency resolver — this must wall");
    let reason = e.reason().to_string();
    assert!(
        reason.contains("import somepkg") && reason.contains("feature wall"),
        "the wall must name the import as a feature gap, got: {reason}"
    );
    assert!(
        !reason.contains("type errors"),
        "an unresolved import must never be misfiled as type errors, got: {reason}"
    );
}
