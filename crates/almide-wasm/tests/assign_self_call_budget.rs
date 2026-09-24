//! #2616: `x = f(x, …)` must release the old value of `x` when `f` did not
//! spend it. This is the budget form of spec/wasm_cross/assign_self_call_release.almd.
//! The alloc ledger pins that fixture's watermark, but a ledger can be
//! re-recorded. This budget cannot: the program's live set is a few
//! hundred KB, and the leak put it at 57 MB (every old generation of the
//! borrowed, owned, map and string accumulators kept its credit). Under
//! 16 MiB the leak takes the defined C-197 out-of-memory exit, and
//! reclamation fits.
//!
//! The 256 MiB twin is the anti-vacuous half. The same program completes
//! there on either build, so a 16 MiB failure is the leak's doing, not the
//! program's.

mod harness;
use almide_wasm_run::run_wasm_capped;

const SRC: &str = include_str!("../../../spec/wasm_cross/assign_self_call_release.almd");

const EXPECTED: &str = "borrowed: 2000 1999000
owned: 2000 1999000
map: 1000 1998
string: 2000 012345678901
mut: 2000 9000
";

fn emit() -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("assign_self_call_release.almd", SRC).expect("front");
    almide_wasm::emit_program(&ir).expect("emit")
}

#[cfg_attr(debug_assertions, ignore = "budget sweep is release-only (CI: release-shape job)")]
#[test]
fn self_call_assign_fits_a_small_budget() {
    let r = run_wasm_capped(&emit(), 16 * 1024 * 1024).expect("engine");
    assert_eq!(
        r.exit, 0,
        "`x = f(x, …)` leaked its old values past 16 MiB (#2616): {}",
        r.stderr
    );
    assert_eq!(r.stdout, EXPECTED);
}

#[cfg_attr(debug_assertions, ignore = "budget sweep is release-only (CI: release-shape job)")]
#[test]
fn self_call_assign_completes_under_a_large_budget() {
    let r = run_wasm_capped(&emit(), 256 * 1024 * 1024).expect("engine");
    assert_eq!(r.exit, 0, "the large budget must complete: {}", r.stderr);
    assert_eq!(r.stdout, EXPECTED);
}
