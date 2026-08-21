//! Perf probe (not a gate yet): emits each kernel once and times five
//! wasmtime runs, best-of. First measured 2026-08-21 against the a877
//! oracle's wasm leg — the numbers and the three fixes they forced
//! (merge sort, single-alloc filter, regionless meter elision) are in
//! PORTLOG stage 52. Ratio gating follows once a second checkpoint
//! exists.

mod harness;
use harness::run_wasm;
use std::time::Instant;

#[test]
#[ignore = "perf probe — run explicitly: cargo test --release -p almide-wasm --test perf_probe -- --ignored --nocapture"]
fn bench() {
    let dir = std::env::var("ALMIDE_BENCH_DIR").unwrap_or_else(|_| {
        format!("{}/tests/perf", env!("CARGO_MANIFEST_DIR"))
    });
    // MEASURED baseline, never a guessed one (the first comparison round
    // mis-called two verdicts by subtracting an estimate): an empty
    // program through the same emit+run path.
    let empty_ir = almide_spine::s5::lower_to_ir("e.almd", "fn main() -> Unit = println(\"\")\n")
        .expect("front");
    let empty = almide_wasm::emit_program(&empty_ir).expect("emit");
    let mut base = u128::MAX;
    for _ in 0..5 {
        let t = Instant::now();
        run_wasm(&empty).expect("run");
        base = base.min(t.elapsed().as_millis());
    }
    println!("BENCH baseline_ms={base}");
    for b in ["int_loop", "float_math", "str_build", "list_sort", "recursion", "list_pipeline"] {
        let src = std::fs::read_to_string(format!("{dir}/{b}.almd")).expect("bench file");
        let t0 = Instant::now();
        let ir = almide_spine::s5::lower_to_ir("b.almd", &src).expect("front");
        let bytes = almide_wasm::emit_program(&ir).expect("emit");
        let emit_ms = t0.elapsed().as_millis();
        let mut best = u128::MAX;
        let mut out = String::new();
        for _ in 0..5 {
            let t = Instant::now();
            let r = run_wasm(&bytes).expect("run");
            let ms = t.elapsed().as_millis();
            if ms < best {
                best = ms;
            }
            out = r.stdout.clone();
            assert_eq!(r.exit, 0, "{b} exited nonzero");
        }
        println!(
            "BENCH {b} emit_ms={emit_ms} run_best_ms={best} work_ms={} out={}",
            best.saturating_sub(base),
            out.trim()
        );
    }
}
