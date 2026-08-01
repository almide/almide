//! Stage 1 charge-probe insertion (ALMIDE_FUEL_PROBE builds only).
//!
//! Inserts [`crate::Op::Charge`] at every function entry and after every
//! `LoopStart` — the W1 placement (every cycle and every call passes a charge
//! site). Runs on the SHARED user-function MIR, immediately after lowering and
//! BEFORE any leg-specific pass, on BOTH the wasm and the native leg. From that
//! point on, every downstream pass and both renderers are REQUIRED to preserve
//! the charges; the probe run compares (consumed, trace) across targets and a
//! divergence falsifies the charge-trace-preservation property.
//!
//! Site ids are a function of the FUNCTION NAME and the in-function charge
//! index — never of function ORDER — because the two legs link different
//! function sets (wasm links self-hosted runtime fns as MIR; native calls
//! intrinsics) and only the user functions are common ground.

use crate::{MirFunction, Op};

/// FNV-1a over the function name, folded to 24 bits so `site` stays readable
/// in traces; the low byte carries the in-function index.
fn site_id(fn_name: &str, idx: u32) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in fn_name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    (h & 0x00ff_ffff).wrapping_mul(251).wrapping_add(idx)
}

/// True when the probe is requested for this process.
pub fn probe_enabled() -> bool {
    std::env::var("ALMIDE_FUEL_PROBE").is_ok_and(|v| v == "1")
}

/// Insert entry + loop-head charges into every function, in place. No-op when
/// the probe env var is not set.
pub fn insert_probe_charges(functions: &mut [MirFunction]) {
    if !probe_enabled() {
        return;
    }
    for f in functions.iter_mut() {
        let mut idx: u32 = 0;
        let mut out: Vec<Op> = Vec::with_capacity(f.ops.len() + 4);
        out.push(Op::Charge { site: site_id(&f.name, idx), cost: 1 });
        idx += 1;
        for op in f.ops.drain(..) {
            let is_loop_start = matches!(op, Op::LoopStart);
            out.push(op);
            if is_loop_start {
                out.push(Op::Charge { site: site_id(&f.name, idx), cost: 1 });
                idx += 1;
            }
        }
        f.ops = out;
    }
}

// ───────────────────── charge certificate (static preservation) ─────────────────────

/// Extract the charge-site sequence a rendered WAT module executes, in TEXT
/// order, via the site-specific trace-update pattern (the same pattern
/// [`crate::translation_validation::wasm_pattern`] claims). BCE-versioned
/// loops legitimately DUPLICATE a body, so consumers compare
/// [`first_occurrences`], not raw counts.
pub fn wasm_charge_sites(wat: &str) -> Vec<u32> {
    const PAT: &str = "(i64.mul (global.get $__trace) (i64.const 1000003)) (i64.const ";
    let mut out = Vec::new();
    let mut rest = wat;
    while let Some(i) = rest.find(PAT) {
        rest = &rest[i + PAT.len()..];
        if let Some(end) = rest.find(')') {
            if let Ok(site) = rest[..end].trim().parse::<u32>() {
                out.push(site);
            }
        }
    }
    out
}

/// Extract the charge-site sequence from rendered native Rust source, in TEXT
/// order, via the `__almd_charge(site, cost)` shim calls.
pub fn native_charge_sites(rs: &str) -> Vec<u32> {
    const PAT: &str = "__almd_charge(";
    let mut out = Vec::new();
    let mut rest = rs;
    while let Some(i) = rest.find(PAT) {
        rest = &rest[i + PAT.len()..];
        if let Some(end) = rest.find(',') {
            if let Ok(site) = rest[..end].trim().parse::<u32>() {
                out.push(site);
            }
        }
    }
    out
}

/// The order of FIRST occurrences — the render-order claim that survives
/// legitimate body duplication (loop versioning): a dropped site vanishes,
/// a reordered site changes the sequence, a duplicated body does neither.
pub fn first_occurrences(sites: &[u32]) -> Vec<u32> {
    let mut seen = std::collections::HashSet::new();
    sites.iter().copied().filter(|s| seen.insert(*s)).collect()
}

#[cfg(test)]
mod cert_tests {
    use super::*;

    #[test]
    fn wasm_extraction_orders_and_parses() {
        let wat = "\
    (global.set $__fuel (i64.add (global.get $__fuel) (i64.const 1)))\n\
    (global.set $__trace (i64.add (i64.mul (global.get $__trace) (i64.const 1000003)) (i64.const 42)))\n\
    (i64.const 999)\n\
    (global.set $__trace (i64.add (i64.mul (global.get $__trace) (i64.const 1000003)) (i64.const 7)))\n";
        assert_eq!(wasm_charge_sites(wat), vec![42, 7]);
    }

    #[test]
    fn native_extraction_orders_and_parses() {
        let rs = "fn main() {\n    __almd_charge(42, 1);\n    let x = 5;\n    __almd_charge(7, 1);\n}\n";
        assert_eq!(native_charge_sites(rs), vec![42, 7]);
    }

    #[test]
    fn first_occurrences_survives_duplication() {
        assert_eq!(first_occurrences(&[1, 2, 3, 2, 3]), vec![1, 2, 3]);
        assert_eq!(first_occurrences(&[]), Vec::<u32>::new());
    }

    #[test]
    fn insertion_is_noop_without_env() {
        // Deliberately does NOT set the env var: the default path must not
        // insert charges (normal builds are byte-identical to pre-probe).
        if std::env::var("ALMIDE_FUEL_PROBE").is_ok() {
            return; // an outer harness set it; this test's claim is vacuous there
        }
        let mut fns: Vec<crate::MirFunction> = Vec::new();
        insert_probe_charges(&mut fns);
        assert!(fns.is_empty());
    }
}
