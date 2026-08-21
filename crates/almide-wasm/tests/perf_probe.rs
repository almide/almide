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
        println!("BENCH {b} emit_ms={emit_ms} run_best_ms={best} out={}", out.trim());
    }
}
