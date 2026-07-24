//! #826 optimization ②: inline single-prim SCALAR wrapper calls.
//!
//! The self-hosted stdlib links `math.sqrt`, `math.abs`, … as real wasm
//! functions whose whole body is ONE pure-scalar prim (`fn sqrt(x) =
//! prim.fsqrt(x)`). Leaving the call in place costs twice:
//!
//! 1. The call itself — wasmtime's Cranelift does not inline across wasm
//!    functions, so a hot loop pays call/return + the i64-uniform ABI's
//!    reinterpret pair per invocation (nbody: 50M `math.sqrt` calls).
//! 2. Worse, `classify_f64_locals` must POISON every value crossing a call
//!    boundary (the ABI is i64), so ONE `math.sqrt(dsq)` forces `dsq` — and
//!    every local in its copy component — back to an i64 slot for its whole
//!    lifetime, re-introducing reinterpret round-trips into arithmetic that
//!    never goes near the call.
//!
//! This pass rewrites `CallFn { name, args }` → the callee's `Op::Prim`
//! directly when the callee's body is EXACTLY one pure-scalar prim over its
//! (all-scalar) params returning that prim's dst. After the rewrite the f64
//! classifier sees a HARD float site instead of a poisoning call, so the
//! surrounding chain stays in real f64 locals.
//!
//! Soundness: the wrapper body IS the prim — same op, same operands, same
//! result value; the prim kinds admitted are pure register arithmetic (no
//! memory, no trap, no ownership events), so the certificate stream is
//! unchanged (neither `CallFn` with scalar args nor scalar `Prim` is an
//! ownership event). Target-agnostic MIR rewrite, applied before the
//! renderers run.

use crate::{CallArg, MirFunction, Op, PrimKind, ValueId};
use std::collections::BTreeMap;

/// Prim kinds a wrapper may consist of: pure scalar-in/scalar-out register
/// arithmetic. Memory/WASI/rc prims are excluded — inlining those would move
/// an effect across a call boundary the certificate accounts differently.
fn scalar_pure_prim(kind: &PrimKind) -> bool {
    matches!(
        kind,
        PrimKind::FloatUn(_)
            | PrimKind::FloatBin(_)
            | PrimKind::FloatCmp(_)
            | PrimKind::F64FromInt
            | PrimKind::IntToFloat
            | PrimKind::FloatToInt
            | PrimKind::FloatBits
            | PrimKind::F32Demote
            | PrimKind::F32Promote
            | PrimKind::IntToF32
            | PrimKind::F32Bits
            | PrimKind::F32Bin(_)
            | PrimKind::F32Cmp(_)
            | PrimKind::F32Un(_)
    )
}

/// `f` is a single-prim scalar wrapper: all-scalar params, body = one
/// admitted prim whose args are exactly the params (any order, each once),
/// returning the prim's dst. Returns the kind and, per prim arg, the PARAM
/// INDEX it reads — the call-site rewrite maps caller args through it.
fn wrapper_shape(f: &MirFunction) -> Option<(PrimKind, Vec<usize>)> {
    if f.ops.len() != 1 || f.params.iter().any(|p| p.repr.is_heap()) {
        return None;
    }
    let Op::Prim { kind, dst: Some(d), args } = &f.ops[0] else { return None };
    if !scalar_pure_prim(kind) || f.ret != Some(*d) || args.len() != f.params.len() {
        return None;
    }
    let param_vals: Vec<ValueId> = f.params.iter().map(|p| p.value).collect();
    let mut used = vec![false; param_vals.len()];
    let mut order = Vec::with_capacity(args.len());
    for a in args {
        let i = param_vals.iter().position(|p| p == a)?;
        if used[i] {
            return None;
        }
        used[i] = true;
        order.push(i);
    }
    Some((*kind, order))
}

/// Rewrite every `CallFn` to a single-prim scalar wrapper into the prim
/// itself. Runs after the self-host runtime link (so `math.sqrt` & co are
/// present as `MirFunction`s) and before rendering; a wrapper left with no
/// remaining callers is dropped later by the unreachable-function prune.
pub fn inline_scalar_prim_wrappers(functions: &mut [MirFunction]) {
    let wrappers: BTreeMap<String, (PrimKind, Vec<usize>)> = functions
        .iter()
        .filter_map(|f| wrapper_shape(f).map(|s| (f.name.clone(), s)))
        .collect();
    if wrappers.is_empty() {
        return;
    }
    for f in functions.iter_mut() {
        for op in f.ops.iter_mut() {
            let Op::CallFn { dst: Some(d), name, args, result } = op else { continue };
            if result.is_some_and(|r| r.is_heap()) {
                continue;
            }
            let Some((kind, order)) = wrappers.get(name.as_str()) else { continue };
            // Positional call args must all be plain scalar locals (an Imm
            // would need a materialization site — not worth the case).
            let vals: Option<Vec<ValueId>> = args
                .iter()
                .map(|a| match a {
                    CallArg::Scalar(v) => Some(*v),
                    _ => None,
                })
                .collect();
            let Some(vals) = vals else { continue };
            if vals.len() != order.len() {
                continue;
            }
            let prim_args: Vec<ValueId> = order.iter().map(|&pi| vals[pi]).collect();
            *op = Op::Prim { kind: *kind, dst: Some(*d), args: prim_args };
        }
    }
}
