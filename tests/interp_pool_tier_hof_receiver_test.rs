//! The interp's pool tier keeps heap values as ADDRESSES while a self-hosted
//! body is on the stack, so a VALUE-level body (`matrix_softmax_rows` was
//! `m |> list.map(..)`) reached from inside another body with a block a
//! HEAP-level body built (matrix `*`'s) used to iterate an Int and abort
//! with `internal: HOF receiver not iterable` — a wrong third vote (exit 1
//! where both backends print), found by the `matrix_fused` compositions
//! (#1423 stage 4). A HOF only reads its receiver, so `eval_hof` now reads a
//! block-address receiver back under its static type, the same read-back the
//! pool boundary performs. The positive reproducer (`matrix.attention_weights`
//! against its direct spelling) left with those functions at dialect epoch 14,
//! when no stdlib composition of that shape remained; the negative half below
//! still pins that the read-back leaves a plain top-level receiver alone.

fn run(source: &str) -> almide_interp::RunOutcome {
    let ir = almide::wasm_leg::lower_to_ir("spec/wasm_cross/probe.almd", source)
        .expect("front failed");
    almide_interp::Interpreter::new(&ir).run_main()
}

/// The read-back must not reach a receiver that is a plain integer by
/// coincidence: a HOF over a list of Ints at the top level, whose elements
/// happen to equal live block addresses, is untouched (the tier is not on the
/// stack, and the receiver is already a value).
#[test]
fn a_top_level_hof_receiver_is_left_alone() {
    let out = run(
        "fn main() -> Unit = {\n\
         \x20 let m = matrix.from_lists([[1.0]])\n\
         \x20 let xs = [0, 1, 2, 3, 4, 5, 6, 7, 8]\n\
         \x20 println(\"${list.map(xs, (x) => x * 2)} ${matrix.rows(m)}\")\n\
         }\n",
    );
    assert_eq!(out.status, almide_interp::RunStatus::Ok, "stderr=<{}>", out.stderr);
    assert_eq!(out.stdout, "[0, 2, 4, 6, 8, 10, 12, 14, 16] 1\n");
}
