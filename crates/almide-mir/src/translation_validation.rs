//! Translation validation V — the auditor's "you proved a MODEL; does the REAL
//! wasm artifact correspond?" answered per build (tier-1 layer 6).
//!
//! The proof `proofs/ALS.v::eager_copy_refines_safety` establishes: an ownership
//! RC trace with NO decrement can never double-free. V makes that proof's
//! PRECONDITION a checked fact about the EMITTED artifact: it scans the actual
//! emitted wasm and confirms it contains no refcount-decrement op. So the C-SAFE
//! safety core (no double-free / use-after-free) holds for the REAL bytes — not
//! merely for a model — and it is re-established on every build.
//!
//! Scope (honest, per `proofs/TRUSTED_BASE.md`): the eager-copy renderer is
//! Dec-free, so this V is satisfied today and certifies no-double-free. It does
//! NOT yet certify leak-freedom (eager-copy leaks) nor the full RC-trace
//! correspondence (emitted `rc_inc`/`rc_dec` == the certificate) — those arrive
//! with the real-RC renderer. V is also the GATE that keeps it honest: if a
//! future renderer emits a decrement without a matching certified trace, this
//! check fails rather than silently certifying an unproven artifact.

/// The proven precondition, checked on the real artifact: does the emitted wasm
/// avoid every refcount decrement? If so, `eager_copy_refines_safety` applies
/// and the artifact cannot double-free.
pub fn artifact_is_dec_free(wat: &str) -> bool {
    !wat.contains("rc_dec")
}

/// V for the C-SAFE safety core: validate that the emitted artifact refines the
/// no-double-free property the proof guarantees. (Today this is exactly the
/// Dec-free check; it is named separately so the call sites read as "validate
/// safety", and so future renderers extend it without changing callers.)
pub fn validate_safety(wat: &str) -> bool {
    artifact_is_dec_free(wat)
}

/// The op → required-wasm-instruction-pattern TABLE — the formal byte-binding
/// object (certificate-format-v1 §4 / G1.4): each MIR op is bound to the wasm
/// instruction the renderer must emit for it. `None` = the eager-copy renderer
/// emits NO instruction (Drop/Consume/MakeUnique/… are no-ops — the very source
/// of the Dec-free safety property). Patterns are `call $…` / arithmetic ops, so
/// they match an actual emitted CALL, never a runtime-preamble `func $…`
/// definition.
///
/// NOTE (honest scope): this checks PRESENCE of each op's pattern (necessary —
/// it catches a renderer that drops an op). The precise per-op byte-WINDOW
/// bijection (op_idx → instruction span) and the SEMANTIC claim that a pattern
/// REALIZES the abstract op (the runtime memory model) are the refinements — the
/// latter is the once-built WasmCert-Coq library (G1.2, the deferred heavy track).
pub fn wasm_pattern(op: &crate::Op) -> Option<String> {
    use crate::{Init, IntOp, Op, RtFn};
    Some(match op {
        Op::Alloc { init: Init::IntList(_), .. } => "call $list_new".into(),
        Op::Dup { .. } => "call $list_copy".into(),
        Op::Call { func: RtFn::PrintInt, .. } => "call $print_int".into(),
        Op::Call { func: RtFn::PrintList, .. } => "call $print_list".into(),
        Op::Call { func: RtFn::ListSet, .. } => "call $list_set".into(),
        Op::Call { func: RtFn::ListPush, .. } => "call $list_push".into(),
        Op::CallFn { name, .. } => format!("call ${name}"),
        Op::IntBinOp { op: IntOp::Add, .. } => "i64.add".into(),
        Op::IntBinOp { op: IntOp::Sub, .. } => "i64.sub".into(),
        Op::IntBinOp { op: IntOp::Mul, .. } => "i64.mul".into(),
        // No emitted instruction (eager no-ops, opaque alloc, not-yet-rendered ops).
        Op::Alloc { .. }
        | Op::Const { .. }
        | Op::Drop { .. }
        | Op::Consume { .. }
        | Op::Borrow { .. }
        | Op::MakeUnique { .. }
        | Op::Pure { .. }
        | Op::Call { func: RtFn::PrintStr, .. } => return None,
    })
}

/// V, table-driven: the emitted wasm REALIZES the MIR iff (1) every op's required
/// instruction pattern is present, and (2) no refcount-decrement appears (the
/// eager-mode safety precondition `eager_copy_refines_safety`). This is `R(M,w)`
/// for the eager fragment — a strict strengthening of `validate_safety` from
/// "no rc_dec" to "each op realized AND no rc_dec".
pub fn validate_translation(wat: &str, mir: &crate::MirFunction) -> bool {
    validate_safety(wat)
        && mir.ops.iter().all(|op| wasm_pattern(op).is_none_or(|p| wat.contains(&p)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_wasm::render_wasm;
    use crate::{Init, MirFunction, Op, Repr, ValueId, PLACEHOLDER_LAYOUT};

    fn heap() -> Repr {
        Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
    }

    #[test]
    fn emitted_wasm_passes_safety_validation() {
        // The real emitted artifact for a value-semantics program is Dec-free, so
        // by ALS.eager_copy_refines_safety it cannot double-free — V certifies
        // the C-SAFE safety core on the ACTUAL bytes.
        let (a, b) = (ValueId(0), ValueId(1));
        let mir = MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::IntList(vec![1, 2, 3]) },
                Op::Dup { dst: b, src: a },
                Op::MakeUnique { v: a },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
            ..Default::default()
        };
        let wat = render_wasm(&mir);
        assert!(validate_safety(&wat), "emitted artifact must be Dec-free (safety core)");
    }

    #[test]
    fn translation_validation_requires_each_op_realized() {
        use crate::{CallArg, RtFn};
        // A real value-semantics program: build a list, print an int, drop.
        let (a, n) = (ValueId(0), ValueId(1));
        let mir = MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::IntList(vec![1, 2, 3]) },
                Op::Const { dst: n },
                Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(n)] },
                Op::Drop { v: a },
            ],
            ..Default::default()
        };
        let wat = render_wasm(&mir);
        // Each op's table pattern is present AND the artifact is Dec-free.
        assert!(validate_translation(&wat, &mir));
        assert!(wat.contains("call $list_new") && wat.contains("call $print_int"));
        // Non-vacuous: a renderer that DROPPED the print fails V (the table catches
        // an unrealized op — stronger than the bare Dec-free scan).
        let stripped = wat.replace("call $print_int", "nop");
        assert!(!validate_translation(&stripped, &mir), "an unrealized op must fail V");
    }

    #[test]
    fn a_decrement_would_fail_validation() {
        // The gate is non-vacuous: an artifact that decrements without a certified
        // trace is rejected, not silently certified.
        assert!(!validate_safety("(func $f (call $rc_dec (local.get 0)))"));
        assert!(validate_safety("(func $f (call $rc_inc (local.get 0)))"));
    }
}
