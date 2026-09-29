// `almide test --run <pattern>` must select the SAME tests on both legs (#2085).
//
// The wasm leg has no argv: it selects at runner-synthesis time, so the check is
// "which tests did the synthesized runner get built out of". The native leg hands
// the pattern to the Rust test harness, which substring-matches the EMITTED fn
// name — so `almide_base::names` owns the predicate and both legs read it.
//
// The bug this pins was invisible to every other gate: `cmd_test_wasm` took the
// filter as `_run_filter` and dropped it at the signature, so the wasm leg ran
// every test whatever the caller asked. A green suite proved nothing, because
// running MORE tests than asked still passes when they all pass.
//
// Selection is asserted by what the runner PRINTS: filtering a two-test file
// down to one test must print exactly what a file containing only that test
// prints. (The module bytes differ by the assertion's source line, so they
// are not compared.)

use almide_base::names::{rust_safe_fn_name, test_name_matches_filter};

const BOTH: &str = "fn main() -> Unit = println(\"x\")\n\
\n\
test \"alpha passes\" {\n\
  assert_eq(1, 1)\n\
}\n\
\n\
test \"beta fails\" {\n\
  assert_eq(1, 2)\n\
}\n";

const ONLY_ALPHA: &str = "fn main() -> Unit = println(\"x\")\n\
\n\
test \"alpha passes\" {\n\
  assert_eq(1, 1)\n\
}\n";

const ONLY_BETA: &str = "fn main() -> Unit = println(\"x\")\n\
\n\
test \"beta fails\" {\n\
  assert_eq(1, 2)\n\
}\n";

/// What the wasm leg's synthesized runner prints for `source` under `filter`
/// (the lane's own lowering, `almide::wasm_leg::lower_to_ir_tests_with_deps`,
/// run on the embedded host): one `test: <label> ... ` line per selected test.
/// A failing test ends the run, so stdout up to it is the observable.
fn render(source: &str, filter: Option<&str>) -> String {
    let ir = almide::wasm_leg::lower_to_ir_tests_with_deps("t.almd", source, &[], filter)
        .expect("the fixture lowers on the wasm leg");
    let (bytes, _) = almide_wasm::emit_program_with_ops(&ir).expect("the fixture emits on the wasm leg");
    almide_wasm_run::run_wasm(&bytes).expect("the embedded host runs the module").stdout
}

/// How many tests the runner was built out of: the `test: ` lines.
fn tests_built_in(source: &str, filter: Option<&str>) -> usize {
    render(source, filter).matches("test: ").count()
}

#[test]
fn no_filter_builds_every_test() {
    assert_eq!(tests_built_in(BOTH, None), tests_built_in(ONLY_ALPHA, None) * 2);
}

#[test]
fn filtering_to_one_test_renders_that_test_s_module_exactly() {
    assert_eq!(render(BOTH, Some("alpha")), render(ONLY_ALPHA, None));
    assert_eq!(render(BOTH, Some("beta")), render(ONLY_BETA, None));
    // …and the two are genuinely different modules, so the equalities above are
    // not both satisfied by some degenerate empty render.
    assert_ne!(render(ONLY_ALPHA, None), render(ONLY_BETA, None));
}

/// Zero selected is a legal render, not a wall: the runner is simply empty, the
/// module exits 0, and the harness reports a pass over zero tests. Erroring here
/// would turn a mistyped `--run` into a compile failure.
#[test]
fn a_filter_matching_nothing_still_renders_an_empty_runner() {
    assert_eq!(tests_built_in(BOTH, Some("zzz-no-such-test")), 0);
}

/// The predicate is the native leg's, not a lookalike: the pattern is compared
/// against the emitted fn name. `beta fails` (with the space) selecting nothing
/// is native's existing behaviour — surprising, but the point of #2085 is that
/// both legs are surprising in the SAME way.
#[test]
fn the_predicate_matches_the_emitted_name_on_both_legs() {
    let ir_name = "__test_almd_beta fails";
    assert_eq!(rust_safe_fn_name(ir_name), "__test_almd_beta_fails");
    assert!(test_name_matches_filter(ir_name, "beta"));
    assert!(test_name_matches_filter(ir_name, "beta_fails"));
    assert!(!test_name_matches_filter(ir_name, "beta fails"));
    assert!(!test_name_matches_filter(ir_name, "BETA"));

    // The wasm leg agrees with the predicate on the same spellings.
    assert_eq!(tests_built_in(BOTH, Some("beta fails")), 0);
    assert_eq!(render(BOTH, Some("beta_fails")), render(ONLY_BETA, None));
}
