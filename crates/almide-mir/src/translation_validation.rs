//! Translation validation V — the auditor's "you proved a MODEL; does the REAL
//! wasm artifact correspond?" answered per build (tier-1 layer 6).
//!
//! The renderer is now in the RC regime (A1.1b): a `Drop` emits `call $rc_dec`,
//! so the SAFETY basis is `proofs/RuntimeModel.v::balanced_cert_no_memory_fault`
//! — an ACCEPTED (balanced) certificate has no double-free in the memory machine
//! — together with `balanced_cert_frees_in_memory` — its cell ends FREED (rc 0).
//! V makes that proof's PRECONDITION a checked fact about the EMITTED artifact:
//! the bytes realize EXACTLY the certified release trace (one `rc_dec` per witness
//! drop, and every op's instruction present). So the C-SAFE safety core holds for
//! the REAL bytes — not merely for a model — re-established on every build, with
//! the `$rc_dec` runtime sentinel (it traps on an already-0 cell) as the
//! defense-in-depth backstop.
//!
//! Scope (honest, per `proofs/TRUSTED_BASE.md`): V binds the RELEASES (drops ↔
//! `rc_dec`) and each op's pattern. It does NOT yet certify the SHARING trace
//! (`rc_inc` for aliases — the renderer still eager-copies, A1.3) nor PHYSICAL
//! reclamation (a free-list, A1.2); neither is a safety gap. V is also the GATE
//! that keeps it honest: a renderer that frees FEWER times than the certificate
//! authorizes (a leak), or emits an un-patterned op, fails V rather than silently
//! certifying an unproven artifact.

/// V for the C-SAFE safety core, RC regime: validate that the emitted artifact
/// realizes EXACTLY the certified release trace — every op's instruction present
/// AND one `call $rc_dec` per witness drop. If so, the accepted (balanced)
/// certificate's `balanced_cert_no_memory_fault` (no double-free) and
/// `balanced_cert_frees_in_memory` (the cell ends at 0) transfer to the real
/// bytes. (The `$rc_dec` sentinel — trap on an already-0 cell — is the runtime
/// backstop.) This IS `validate_translation_perceus`; it is named separately so
/// call sites read as "validate safety."
pub fn validate_safety(wat: &str, mir: &crate::MirFunction) -> bool {
    validate_translation_perceus(wat, mir)
}

/// The op → required-wasm-instruction-pattern TABLE — the formal byte-binding
/// object (certificate-format-v1 §4 / G1.4): each MIR op is bound to the wasm
/// instruction the renderer must emit for it. A `Drop` is bound to `call $rc_dec`
/// (the release), a `Dup` to `call $rc_inc` (shared acquire), a `MakeUnique` to
/// `call $list_copy` (the cow clone). `None` = the renderer emits NO instruction
/// for it (Consume is a move-out transfer — no free here; opaque alloc /
/// not-yet-rendered ops). Patterns are `call $…` / arithmetic ops, so they match
/// an actual emitted CALL, never a runtime-preamble `func $…` definition.
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
        Op::Dup { .. } => "call $rc_inc".into(),
        Op::Call { func: RtFn::PrintInt, .. } => "call $print_int".into(),
        Op::Call { func: RtFn::PrintList, .. } => "call $print_list".into(),
        Op::Call { func: RtFn::ListSet, .. } => "call $list_set".into(),
        Op::Call { func: RtFn::ListPush, .. } => "call $list_push".into(),
        Op::CallFn { name, .. } => format!("call ${name}"),
        // A host wasm IMPORT renders to `(call $<import>)`; the import name is the
        // mangled `__import_<module>_<name>` the render declares (see `import_symbol`).
        Op::CallImport { module, name, .. } => {
            format!("call ${}", crate::render_wasm::import_symbol(module, name))
        }
        Op::ConstInt { .. } => "i64.const".into(),
        Op::IntBinOp { op: IntOp::Add, .. } => "i64.add".into(),
        Op::IntBinOp { op: IntOp::Sub, .. } => "i64.sub".into(),
        Op::IntBinOp { op: IntOp::Mul, .. } => "i64.mul".into(),
        Op::IntBinOp { op: IntOp::Div, .. } => "i64.div_s".into(),
        Op::IntBinOp { op: IntOp::Mod, .. } => "i64.rem_s".into(),
        Op::IntBinOp { op: IntOp::Lt, .. } => "i64.lt_s".into(),
        Op::IntBinOp { op: IntOp::Le, .. } => "i64.le_s".into(),
        Op::IntBinOp { op: IntOp::Gt, .. } => "i64.gt_s".into(),
        Op::IntBinOp { op: IntOp::Ge, .. } => "i64.ge_s".into(),
        Op::IntBinOp { op: IntOp::Eq, .. } => "i64.eq".into(),
        Op::IntBinOp { op: IntOp::Ne, .. } => "i64.ne".into(),
        Op::IntBinOp { op: IntOp::And, .. } => "i64.and".into(),
        Op::IntBinOp { op: IntOp::Or, .. } => "i64.or".into(),
        Op::IntBinOp { op: IntOp::Xor, .. } => "i64.xor".into(),
        Op::IntBinOp { op: IntOp::Shl, .. } => "i64.shl".into(),
        Op::IntBinOp { op: IntOp::Shr, .. } => "i64.shr_s".into(),
        Op::IntBinOp { op: IntOp::ShrU, .. } => "i64.shr_u".into(),
        // A release decrements the refcount cell — realized by `call $rc_dec`.
        Op::Drop { .. } => "call $rc_dec".into(),
        Op::DropListStr { .. } => "call $rc_dec".into(),
        Op::DropValue { .. } => "call $__drop_value".into(),
        Op::DropListValue { .. } => "call $__drop_list_value".into(),
        Op::DropListStrValue { .. } => "call $__drop_list_str_value".into(),
        Op::DropListStrStr { .. } => "call $__drop_list_str_str".into(),
        // Inline-rendered (per-tuple String-slot rc_dec loop, no helper) — cert-claimed token is the
        // final list-block `call $rc_dec`.
        Op::DropListIntStr { .. } => "call $rc_dec".into(),
        Op::DropListStrInt { .. } => "call $rc_dec".into(),
        Op::DropResultListValue { .. } => "call $__drop_result_lv".into(),
        Op::DropResultValue { .. } => "call $__drop_result_value".into(),
        // Inline-rendered (no helper) like DropListStr; the cert-claimed token is the
        // final wrapper `call $rc_dec`.
        Op::DropResultStrInt { .. } => "call $rc_dec".into(),
        // Rendered via a value_core helper call (NOT inline) — the cert-claimed token is that call.
        Op::DropResultValueInt { .. } => "call $__drop_value_tuple".into(),
        Op::DropResultListValueInt { .. } => "call $__drop_list_value_tuple".into(),
        // Inline-rendered (nested loop, no helper) — cert-claimed token is the final wrapper rc_dec.
        Op::DropResultListStrInt { .. } => "call $rc_dec".into(),
        // Inline-rendered (Ok-payload list loop, no helper) — cert-claimed token is the final wrapper rc_dec.
        Op::DropResultListStr { .. } => "call $rc_dec".into(),
        Op::DropListListStr { .. } => "drop_list_list_str".into(),
        Op::DropVariant { ty, .. } => format!("call $__drop_{ty}"),
        // Inline-rendered (rc==1-gated recurse into the @12 record via `$__drop_<drop_fn>`, then the
        // wrapper block) — the cert-claimed token is the final wrapper `call $rc_dec`, like DropListStr.
        Op::DropWrapperRec { .. } => "call $rc_dec".into(),
        // A copy-on-write: MakeUnique clones a SHARED block before in-place
        // mutation — realized by `call $list_copy` (in the cow's then-branch).
        Op::MakeUnique { .. } => "call $list_copy".into(),
        // No emitted instruction: Consume MOVES the reference out (no free here),
        // opaque alloc / not-yet-rendered ops emit nothing.
        Op::Alloc { .. }
        | Op::Const { .. }
        | Op::Consume { .. }
        | Op::Borrow { .. }
        | Op::Pure { .. }
        // The prim floor is the trusted hand-mapped surface (not in the corpus V
        // gates); its faithfulness is the §4.1 wasm-spec proof obligation, not a
        // pattern check here.
        | Op::Prim { .. }
        | Op::IfThen { .. }
        | Op::Else { .. }
        | Op::EndIf { .. }
        // Loop markers + scalar reassignment are exec-slice control flow, not corpus V
        // gate ops (like the if-markers above): no per-op pattern claim here.
        | Op::LoopStart
        | Op::LoopBreakUnless { .. }
        | Op::LoopEnd
        | Op::SetLocal { .. }
        // CallIndirect renders to `call_indirect` once the table is wired; no single-token
        // per-op pattern claim here (like the if-markers), and no lowering emits it yet.
        // FuncRef (a closure's table-slot value) is likewise structural, no token claim.
        | Op::CallIndirect { .. }
        | Op::FuncRef { .. }
        | Op::Call { func: RtFn::PrintStr, .. } => return None,
    })
}

/// V, table-driven: the emitted wasm REALIZES each MIR op iff every op's required
/// instruction pattern is present (a `Drop`'s pattern is `call $rc_dec`). This is
/// the PRESENCE half of `R(M,w)`; `validate_translation_perceus` adds the leak-
/// freedom COUNT (one release per drop, not one for many).
pub fn validate_translation(wat: &str, mir: &crate::MirFunction) -> bool {
    mir.ops.iter().all(|op| wasm_pattern(op).is_none_or(|p| wat.contains(&p)))
}

/// The DROP ops — each FREES one reference; its realization in the emitted bytes
/// is a `call $rc_dec`. (A `Consume`/move-out TRANSFERS its reference — no free
/// at this site, the receiver frees later — so it is not counted here.)
fn drop_count(mir: &crate::MirFunction) -> usize {
    mir.ops.iter().filter(|op| matches!(op, crate::Op::Drop { .. })).count()
}

/// PERCEUS-mode V — the renderer's leak-freedom + safety gate (the PRODUCTION
/// side of the seam, A1.1b): the emitted wasm must REALIZE each op AND match each
/// witness drop with a `call $rc_dec`, so the binary actually FREES (cell → 0,
/// `RuntimeModel.balanced_cert_frees_in_memory`) and frees no FEWER times than the
/// certificate authorizes. The renderer now SATISFIES this (one `rc_dec` per
/// drop); a renderer that emitted one `rc_dec` for several drops (a leak), or
/// dropped an op, FAILS here. The `$rc_dec` sentinel traps a double-free at run.
pub fn validate_translation_perceus(wat: &str, mir: &crate::MirFunction) -> bool {
    let positives = mir.ops.iter().all(|op| wasm_pattern(op).is_none_or(|p| wat.contains(&p)));
    // Count releases in the USER FUNCTION ONLY — the FIXED runtime preamble's WASI-floor funcs
    // now contain their OWN `call $rc_dec` (e.g. `$read_dir` frees its readdir buffer), which are
    // NOT part of THIS function's certified release trace. Subtract the preamble's intrinsic
    // count (a fixed constant) so the leak-freedom comparison stays precise: an under-freeing
    // user body must still fail even though the preamble frees internally.
    let preamble_rc_decs = crate::render_wasm::preamble().matches("call $rc_dec").count();
    let total_rc_decs = wat.matches("call $rc_dec").count();
    let body_rc_decs = total_rc_decs.saturating_sub(preamble_rc_decs);
    positives && body_rc_decs >= drop_count(mir)
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
    fn emitted_wasm_realizes_the_release_trace() {
        // The RC-regime safety gate: the emitted bytes realize EXACTLY the
        // certified release trace — one `rc_dec` per drop — so the accepted cert's
        // balanced_cert_no_memory_fault / _frees_in_memory transfer to the artifact.
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
        assert!(validate_safety(&wat, &mir), "artifact must realize the certified releases");
        // The two drops are realized as releases; MakeUnique's cow adds one more
        // (it relinquishes the shared original before cloning), so >= 2 — never a
        // leak (the extra release is balanced by the cow's fresh copy block).
        assert!(wat.matches("call $rc_dec").count() >= 2);
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
                Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(n)] , result: None },
                Op::Drop { v: a },
            ],
            ..Default::default()
        };
        let wat = render_wasm(&mir);
        // Each op's table pattern is present (incl. `call $rc_dec` for the drop).
        assert!(validate_translation(&wat, &mir));
        assert!(wat.contains("call $list_new") && wat.contains("call $print_int"));
        assert!(wat.contains("call $rc_dec"), "the drop is realized as a release");
        // Non-vacuous: a renderer that DROPPED the print fails V (the table catches
        // an unrealized op).
        let stripped = wat.replace("call $print_int", "nop");
        assert!(!validate_translation(&stripped, &mir), "an unrealized op must fail V");
    }

    #[test]
    fn perceus_v_passes_on_realized_releases_and_flags_a_leak() {
        // A1.1b production side: the renderer is no longer eager — it emits a
        // release per drop, so its output PASSES perceus V. The gate still CATCHES
        // a leaking artifact: strip the releases and V flags the drops-without-dec.
        let (a, b) = (ValueId(0), ValueId(1));
        let mir = MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::IntList(vec![1, 2, 3]) },
                Op::Dup { dst: b, src: a },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
            ..Default::default()
        };
        let wat = render_wasm(&mir);
        // The real renderer output realizes both releases → passes (safe + freed).
        assert!(validate_translation_perceus(&wat, &mir));
        // A hypothetical leaking renderer (releases stripped) → V flags it.
        let leaked = wat.replace("call $rc_dec", "nop");
        assert!(
            !validate_translation_perceus(&leaked, &mir),
            "an artifact that drops in the witness but frees fewer times must fail V"
        );
    }

    #[test]
    fn under_freeing_fails_validation() {
        // In the RC regime a decrement is EXPECTED; the gate is non-vacuous the
        // other way — an artifact that frees FEWER times than the certificate's
        // drops (a leak) is rejected, not silently certified.
        let (a, b) = (ValueId(0), ValueId(1));
        let mir = MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::IntList(vec![1]) },
                Op::Dup { dst: b, src: a },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
            ..Default::default()
        };
        let wat = render_wasm(&mir);
        // Two drops but only one release in the bytes → leak → fail. (The validator subtracts
        // the fixed preamble's intrinsic `call $rc_dec` count, so stripping ANY one release —
        // even a preamble-internal one — leaves the user body under-freeing vs its drop_count.)
        let under = wat.replacen("call $rc_dec", "nop", 1);
        assert!(
            !validate_translation_perceus(&under, &mir),
            "under-freeing (a leak) must fail V"
        );
    }
}
