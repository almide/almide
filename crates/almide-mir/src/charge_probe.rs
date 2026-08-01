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
