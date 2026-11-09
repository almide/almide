//! #4614 — the ownership witness's path walk grows linearly with the objects
//! of one frame.
//!
//! A module-level `List` of N records lowers to one initializer frame with
//! N records (and their strings) in a straight line. The walk rendered each
//! object's line by walking the whole frame, so the wasm build grew with N²
//! (1600 rows: 4.4 s, against 0.14 s before the witness). This counts the
//! walk's work (`witness::path_walk_steps`: one per node a path is stepped
//! through, one per loop activation queued) — not wall time — at N and 4N:
//! linear work grows about 4x, the per-object full walk 16x. The bound is 6x.

mod harness;
use harness::run_wasm;

fn rows(n: usize) -> String {
    let mut o = vec![
        "type Row = { id: String, kind: String, pattern: String, ja: String, note: String }".to_string(),
        String::new(),
        "let ROWS: List[Row] = [".to_string(),
    ];
    for i in 0..n {
        o.push(format!(
            "  Row {{ id: \"R-{i}\", kind: \"k{}\", pattern: \"pkg.*.op{i}\", ja: \"label {i}\", note: \"\" }},",
            i % 7
        ));
    }
    o.push("]".into());
    o.push(String::new());
    o.push(
        "fn find(id: String) -> String = {\n  var out = \"\"\n  for r in ROWS {\n    if r.id == id then {\n      out = r.ja\n    } else ()\n  }\n  out\n}\n"
            .into(),
    );
    o.push("effect fn main() -> Unit = {\n  println(find(\"R-1\"))\n}".into());
    o.join("\n")
}

/// (walk steps, stdout) of one build of the N-row program, the witness armed.
fn build(n: usize) -> (u64, String) {
    let ir = almide_spine::s5::lower_to_ir("witness_paths_linear.almd", &rows(n)).expect("front");
    almide_wasm::witness::start_collecting();
    let before = almide_wasm::witness::path_walk_steps();
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let steps = almide_wasm::witness::path_walk_steps() - before;
    let certs = almide_wasm::witness::take();
    assert!(!certs.is_empty(), "the armed build recorded no witness");
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (steps, out.stdout)
}

#[test]
fn the_witness_path_walk_is_linear_in_a_frames_objects() {
    let (small, out_small) = build(200);
    let (large, out_large) = build(800);
    assert_eq!(out_small, "label 1\n");
    assert_eq!(out_large, "label 1\n");
    assert!(small > 0, "the walk did no work: the witness was not rendered");
    eprintln!("witness path walk: N=200 {small} steps, N=800 {large} steps ({:.2}x)", large as f64 / small as f64);
    assert!(
        large < 6 * small,
        "4x the rows took {:.1}x the witness path walk ({small} -> {large}): the walk is superlinear in the objects of one frame",
        large as f64 / small as f64
    );
}
