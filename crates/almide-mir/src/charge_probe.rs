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

/// Insert deterministic charges. Two modes:
///  - PROBE (`ALMIDE_FUEL_PROBE`): every function is metered — the probe
///    measures the whole program, unchanged.
///  - BUDGET-ONLY: metered-clone specialization (T1-2) — only the outlined
///    region fns and `__fuel` CLONES of their transitive callees carry
///    charges, so a program's non-region paths pay ZERO metering cost.
///    Region spends are unchanged (every in-region callee IS a metered
///    clone), and out-of-region spend is unobservable (verdicts read only
///    the enter/exit delta), so no fixture flip point moves.
pub fn insert_probe_charges(functions: &mut Vec<MirFunction>) {
    if probe_enabled() {
        for f in functions.iter_mut() {
            charge_fn(f);
        }
        return;
    }
    if budget_used() {
        specialize_metered_clones(functions);
    }
}

/// Entry + loop-head charges for one fn, in place (the W1 placement).
fn charge_fn(f: &mut MirFunction) {
    let mut idx: u32 = 0;
    let mut out: Vec<Op> = Vec::with_capacity(f.ops.len() + 4);
    out.push(Op::Charge { site: site_id(&f.name, idx), cost: 1 });
    idx += 1;
    for op in f.ops.drain(..) {
        let is_loop_start = matches!(op, Op::LoopStart);
        // T3-5: a bulk string concat charges 1 + result_len/16 (the dyn
        // charge reads the result AFTER the op, so it sits right behind it).
        let dyn_src = match &op {
            Op::CallFn { name, dst: Some(d), .. } if name == "__str_concat" => Some(*d),
            _ => None,
        };
        out.push(op);
        if is_loop_start {
            out.push(Op::Charge { site: site_id(&f.name, idx), cost: 1 });
            idx += 1;
        }
        if let Some(src) = dyn_src {
            out.push(Op::ChargeDyn { site: site_id(&f.name, idx), src });
            idx += 1;
        }
    }
    f.ops = out;
}

/// T1-2: clone the region-reachable call graph into `__fuel` variants and
/// meter ONLY those (plus the region fns themselves). Lifted lambdas cannot
/// be cloned (table dispatch indexes by name-sorted position), so when a
/// region reaches a `FuncRef` every `__lambda_*` fn stays metered globally —
/// the one documented remainder of "zero metering outside regions".
fn specialize_metered_clones(functions: &mut Vec<MirFunction>) {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    let names: BTreeSet<String> = functions.iter().map(|f| f.name.as_str().to_string()).collect();
    let by_name: BTreeMap<String, usize> =
        functions.iter().enumerate().map(|(i, f)| (f.name.as_str().to_string(), i)).collect();
    let is_root = |n: &str| n.starts_with("__almd_bounded_");

    // Transitive CallFn closure from the region roots (user fns only), plus
    // whether any region-reachable fn takes a FuncRef (a table-dispatched
    // lambda the clone map cannot retarget).
    let mut reachable: BTreeSet<String> = BTreeSet::new();
    let mut uses_funcref = false;
    let mut queue: VecDeque<String> =
        names.iter().filter(|n| is_root(n)).cloned().collect();
    let mut visited: BTreeSet<String> = queue.iter().cloned().collect();
    while let Some(n) = queue.pop_front() {
        let Some(&i) = by_name.get(&n) else { continue };
        for op in &functions[i].ops {
            match op {
                Op::CallFn { name, .. } if names.contains(name.as_str()) => {
                    let callee = name.as_str().to_string();
                    if !is_root(&callee) {
                        reachable.insert(callee.clone());
                    }
                    if visited.insert(callee.clone()) {
                        queue.push_back(callee);
                    }
                }
                Op::FuncRef { .. } => uses_funcref = true,
                _ => {}
            }
        }
    }

    // Clone each reachable fn as `<name>__fuel`, retargeting region-internal
    // calls to the clone family (recursion included).
    let retarget = |f: &mut MirFunction, reachable: &BTreeSet<String>| {
        for op in f.ops.iter_mut() {
            if let Op::CallFn { name, .. } = op {
                if reachable.contains(name.as_str()) {
                    *name = format!("{name}__fuel");
                }
            }
        }
    };
    let mut clones: Vec<MirFunction> = Vec::with_capacity(reachable.len());
    for n in &reachable {
        let i = by_name[n];
        let mut c = functions[i].clone();
        c.name = format!("{n}__fuel");
        retarget(&mut c, &reachable);
        charge_fn(&mut c);
        clones.push(c);
    }
    for f in functions.iter_mut() {
        let name = f.name.as_str().to_string();
        if is_root(&name) {
            retarget(f, &reachable);
            charge_fn(f);
        } else if uses_funcref && name.starts_with("__lambda_") {
            charge_fn(f);
        }
    }
    functions.extend(clones);
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

// ───────────────────── budget activation (Stage 2) ─────────────────────

thread_local! {
    /// Set during MIR lowering when a `fan.bounded` budget intrinsic lowers on
    /// this thread; read by the charge-insertion gate and the wasm preamble so
    /// budget machinery (fuel globals + charges) exists exactly when a program
    /// uses `fan.bounded` — and never otherwise (normal builds byte-identical).
    static BUDGET_USED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Record that the current program lowers a budget intrinsic.
pub fn note_budget_used() {
    BUDGET_USED.with(|b| b.set(true));
}

/// True when this thread's program needs the fuel machinery.
pub fn budget_used() -> bool {
    BUDGET_USED.with(|b| b.get())
}

/// Reset at pipeline entry (one pipeline run per thread).
pub fn reset_budget_used() {
    BUDGET_USED.with(|b| b.set(false));
}

/// The counters start at i64::MAX and count DOWN; consumed = MAX - remaining.
pub const FUEL_START: i64 = i64::MAX;

/// CM-1: the single definition lives beside the unit tables in
/// `almide_types::time_units` (the interp's budget prims read it there);
/// re-exported here for the renderers, the CLI, and the calibration gate.
/// History (v0.2 → v0.3): the v0.2 value of 50 came from a 47µs/1002-unit
/// reference measurement that a millisecond-scale process spawn cannot
/// resolve — the standing D5 gate falsified it at ratio 0.05 the moment it
/// was installed, and the 100M-unit min-of-3 remeasure pinned 3ns/unit.
pub use almide_lang::time_units::CM1_NS_PER_CHARGE;
