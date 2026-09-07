//! Dev probe (not a gate): `ALMIDE_PROBE_SRC=<file> cargo test --release -p almide-wasm --test heap_probe -- --nocapture`
//! prints the heap high-water mark of a program on the structural leg, so a
//! leak can be bisected by editing the program.
mod harness;
use harness::run_wasm;

#[test]
fn probe() {
    let Ok(path) = std::env::var("ALMIDE_PROBE_SRC") else { return };
    let src = std::fs::read_to_string(&path).expect("read");
    let ir = almide_spine::s5::lower_to_ir("probe.almd", &src).expect("front");
    if let Ok(dump) = std::env::var("ALMIDE_PROBE_IR") {
        std::fs::write(dump, format!("{ir:#?}")).expect("dump ir");
    }
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    if let Ok(dump) = std::env::var("ALMIDE_PROBE_DUMP") {
        std::fs::write(dump, &bytes).expect("dump");
    }
    let out = run_wasm(&bytes).expect("run");
    println!("PROBE exit={} heap={:?} stdout={:?}", out.exit, out.heap_end, out.stdout.trim());
}
