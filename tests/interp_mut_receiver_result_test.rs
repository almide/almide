//! A stdlib mutator over a `mut` receiver answers its OWN result in the
//! interpreter, as both backends do, whether or not the move-mode rewrite
//! (almide-ir `mut_param`) reached the call site (#4400). `bytes.pop(b)` on a
//! `var` answered `(some(7), [2])` — the `(result, buffer)` pair only a
//! rewritten site takes apart — under the fuzz ladder's reference-oracle
//! pipeline, which lowers without the rewrite, where native and wasm print
//! `some(7)` (found by the map.upsert / map.merge inspections, seeds 1793524353
//! and 1793524523). Every case runs through BOTH pipelines: the driver's
//! `wasm_leg::lower_to_ir` (rewritten) and the oracle's
//! lower / optimize / mono / ir_link (not rewritten).

/// The driver's front: the move-mode rewrite runs.
fn run_rewritten(source: &str) -> almide_interp::RunOutcome {
    let ir = almide::wasm_leg::lower_to_ir("spec/wasm_cross/probe.almd", source).expect("front failed");
    almide_interp::Interpreter::new(&ir).run_main()
}

/// The fuzz ladder's reference-oracle front (tools/xtarget-fuzz/src/oracle/interp.rs):
/// no move-mode rewrite.
fn run_unrewritten(source: &str) -> almide_interp::RunOutcome {
    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens);
    let mut prog = parser.parse().expect("parse");
    assert!(parser.errors.is_empty(), "parse errors");
    let canon = almide::canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = almide::check::Checker::from_env(canon.env);
    let diags = checker.infer_program(&mut prog);
    assert!(!diags.iter().any(|d| d.level == almide::diagnostic::Level::Error), "check errors");
    let mut ir = almide::lower::lower_program(&prog, &checker.env, &checker.type_map);
    almide::optimize::optimize_program(&mut ir);
    almide::mono::monomorphize(&mut ir);
    almide::ir_link::ir_link(&mut ir);
    almide_interp::Interpreter::new(&ir).run_main()
}

/// The interp must ANSWER here (these are plain container ops it models), and
/// answer what both backends answer, on both fronts.
fn assert_answers(source: &str, expected: &str) {
    for (front, out) in [("rewritten", run_rewritten(source)), ("unrewritten", run_unrewritten(source))] {
        assert!(
            matches!(out.status, almide_interp::RunStatus::Ok),
            "{front}: the interp did not answer: {:?}\n{}",
            out.status,
            out.stdout
        );
        assert_eq!(out.stdout, expected, "{front}: the interp's vote differs from both backends");
    }
}

#[test]
fn bytes_pop_on_a_var_never_read_again_answers_the_byte() {
    assert_answers(
        "fn main() -> Unit = {\n  var s: Bytes = bytes.from_list([2, 7])\n  let r: Int? = bytes.pop(s)\n  println(\"r = ${r}\")\n}\n",
        "r = some(7)\n",
    );
}

#[test]
fn bytes_pop_on_a_var_read_again_answers_the_byte_and_shortens_it() {
    assert_answers(
        "fn main() -> Unit = {\n  var s: Bytes = bytes.from_list([2, 7])\n  let r: Int? = bytes.pop(s)\n  println(\"r = ${r} ${bytes.to_list(s)}\")\n}\n",
        "r = some(7) [2]\n",
    );
}

#[test]
fn bytes_pop_on_an_empty_var_answers_none() {
    assert_answers(
        "fn main() -> Unit = {\n  var s: Bytes = bytes.from_list([])\n  let r = bytes.pop(s)\n  println(\"r = ${r}\")\n}\n",
        "r = none\n",
    );
}

#[test]
fn list_pop_on_a_var_never_read_again_answers_the_element() {
    assert_answers(
        "fn main() -> Unit = {\n  var xs: List[Int] = [2, 7]\n  let r: Int? = list.pop(xs)\n  println(\"r = ${r}\")\n}\n",
        "r = some(7)\n",
    );
}

/// The map.upsert inspection's program (seed 1793524353, index 57), verbatim.
#[test]
fn the_upsert_campaign_program() {
    assert_answers(
        "fn main() -> Unit = {\n  var s7: Bytes = (if string.is_alpha(string.take(string.to_upper(\"\u{0391}\u{0392}\u{0393}\"), 2)) then bytes.from_list([2, 7])\n  else bytes.from_list([]))\n  let r8: Int? = bytes.pop(s7)\n  let r9: (String, Int) = (\"\", uint32.bit_length(uint32.from(1)))\n  println(\"r8 = ${r8}\")\n  println(\"r9 = ${r9}\")\n}\n",
        "r8 = some(7)\nr9 = (\"\", 1)\n",
    );
}

/// The map.merge inspection's program (seed 1793524523, index 184), verbatim.
#[test]
fn the_merge_campaign_program() {
    assert_answers(
        "fn main() -> Unit = {\n  var tmp2: Bytes = bytes.from_list([100, 100, 7, 2])\n  let r3: Int? = bytes.pop(tmp2)\n  let r5: String? = (if false then some(float.to_string(math.atanh(-0.0))) else some(\" nbsp \"))\n  println(\"r3 = ${r3}\")\n  println(\"r5 = ${r5}\")\n}\n",
        "r3 = some(2)\nr5 = some(\" nbsp \")\n",
    );
}

#[test]
fn bytes_put_on_a_var_never_read_again_leaves_the_program_running() {
    assert_answers(
        "fn main() -> Unit = {\n  var s: Bytes = bytes.zeros(2)\n  bytes.put_u8(s, 0, 9)\n  println(\"done\")\n}\n",
        "done\n",
    );
}

#[test]
fn bytes_put_on_a_var_read_again_writes_the_byte() {
    assert_answers(
        "fn main() -> Unit = {\n  var s: Bytes = bytes.zeros(2)\n  bytes.put_u8(s, 0, 9)\n  println(\"${bytes.to_list(s)}\")\n}\n",
        "[9, 0]\n",
    );
}
