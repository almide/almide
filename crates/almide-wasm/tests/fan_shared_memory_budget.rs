//! #4135: a run's fan workers share the run's ONE linear-memory budget.
//!
//! The instance-parallel fan (`crates/almide-wasm-run/src/host_fan.rs`) runs
//! a chunk map on separate instances of the module. Each worker store used to
//! get the whole cap, so `main` holding half the cap and N workers each
//! allocating another half completed — the run held (N + 1) / 2 × its cap.
//! Now every store meters against one budget: the workers' growth is refused,
//! the offer falls back to the sequential run (the fan's own rule), and the
//! sequential run, which needs more than the cap, ends with C-197's defined
//! `Error: out of memory`, exit 1. The large-budget twin shows the program
//! itself is fine: with room for every worker it completes, in parallel.

use almide_wasm_run::run_wasm_capped;

const MIB: usize = 1024 * 1024;
const CAP: usize = 64 * MIB;

/// `main` keeps a CAP/2 string alive across a fan whose every element
/// builds a CAP/2 + 1 byte string more (`string` is a pure module, so the
/// arm qualifies for the instance-parallel offer; `bytes` is not).
const SRC: &str = r#"fn grab(i: Int) -> Int = string.len(string.repeat("x", 33554433 + i))

effect fn main() -> Unit = {
  let held = string.repeat("y", 33554432)
  let xs = list.range(0, 4)
  let sizes = fan {
    list.map(xs, (i) => grab(i))
  }
  println(int.to_string(list.len(sizes)) + " " + int.to_string(string.len(held)))
}
"#;

fn emit() -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("fan_shared_memory_budget.almd", SRC).expect("front");
    almide_wasm::emit_program(&ir).expect("emit")
}

fn parallel() -> bool {
    std::thread::available_parallelism().is_ok_and(|p| p.get() >= 2)
}

#[cfg_attr(debug_assertions, ignore = "budget sweep is release-only (CI: release-shape job)")]
#[test]
fn fan_workers_cannot_exceed_the_runs_one_budget() {
    if !parallel() {
        eprintln!("one CPU: the fan never runs workers here; nothing to bound");
        return;
    }
    let r = run_wasm_capped(&emit(), CAP).expect("engine");
    assert_eq!(
        (r.exit, r.stderr.as_str()),
        (1, "Error: out of memory\n"),
        "the workers were granted memory beyond the run's one budget (#4135); stdout: {}",
        r.stdout
    );
}

#[cfg_attr(debug_assertions, ignore = "budget sweep is release-only (CI: release-shape job)")]
#[test]
fn the_same_fan_completes_when_the_budget_holds_every_worker() {
    let r = run_wasm_capped(&emit(), 64 * CAP).expect("engine");
    assert_eq!(r.exit, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "4 33554432\n");
}
