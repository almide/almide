//! MIR well-formedness: def-before-use over the lowered op stream (#777, F3
//! item 2).
//!
//! The 2026-07-03 audit's five output-breaking bugs all lived in the IR → MIR
//! row of the trust map (`docs/contracts/proven-vs-trusted.md`): a registry or
//! convention drifted between producer and consumer INSIDE the trusted zone,
//! and the certificate accepted the result because drift produces *valid* MIR.
//! The prescribed countermeasure is a post-pass over the lowering's own output
//! checking invariants the lowering is supposed to maintain — the same shape as
//! `assert_names_resolvable` and the `ConcretizeTypes` gate.
//!
//! This module checks the invariant that survived the earlier attempt's
//! post-mortem: **every value an op READS is defined before it** (params and
//! prior defining ops), and the returned value is defined. The earlier attempt
//! gated on tracking-SET consistency phrased over `op_values`, which reports
//! every value an op *touches* — not a definition set — and fired on 950+
//! correct programs because MIR is NOT single-assignment (loop-carried slots
//! reassign via `SetLocal`). The split this file rides on fixes that framing:
//!
//! - [`defined_value`] — the op's fresh definition (exhaustive; a new variant
//!   is a compile error, not a silent "defines nothing"),
//! - [`op_reads`] — the operands, with multiplicity,
//! - `SetLocal` — a REDEFINITION of an existing slot: its `local` is a read
//!   (the slot must already exist), never a fresh definition.
//!
//! The partition is itself checked: for every real op in every lowered
//! function, `defines ⊎ reads` must equal `op_values` as a multiset. That
//! consistency claim is exactly the one a hand-built unit test would sample;
//! asserting it corpus-wide on every build means the three functions cannot
//! drift apart on any op shape the compiler actually produces.
//!
//! A violation is reported as a WALL (`LowerError::Unsupported` with an
//! `INTERNAL` marker), not a panic: it is a compiler bug, but the T6
//! convention owes the user a named refusal rather than a crash, and the
//! differential fuzzer's ladder classifies the wall loudly.

use std::collections::HashSet;

use crate::render_wasm::{defined_value, op_reads, op_values};
use crate::{MirFunction, Op, ValueId};

/// Check def-before-use (+ the defines/reads partition) over one lowered
/// function. `Ok(())` for a well-formed body; `Err(reason)` names the first
/// offending op.
pub(crate) fn check_def_before_use(func: &MirFunction) -> Result<(), String> {
    let mut defined: HashSet<ValueId> = func.params.iter().map(|p| p.value).collect();
    let mut reads: Vec<ValueId> = Vec::new();
    let mut values: Vec<ValueId> = Vec::new();
    for (i, op) in func.ops.iter().enumerate() {
        // The partition consistency claim: defines ⊎ reads == op_values, as
        // multisets. Checked on the real op stream so the three functions
        // cannot drift on any shape the compiler actually produces.
        reads.clear();
        values.clear();
        op_reads(op, &mut reads);
        op_values(op, &mut values);
        let mut partition: Vec<ValueId> = defined_value(op).into_iter().collect();
        partition.extend(reads.iter().copied());
        let mut sorted_partition = partition.clone();
        let mut sorted_values = values.clone();
        sorted_partition.sort_unstable();
        sorted_values.sort_unstable();
        if sorted_partition != sorted_values {
            return Err(format!(
                "INTERNAL mir-wellformed: op {i} ({op:?}) in `{}`: defines+reads {:?} \
                 does not partition op_values {:?}",
                func.name, partition, values
            ));
        }
        // Def-before-use: every read must already be defined.
        for r in &reads {
            if !defined.contains(r) {
                return Err(format!(
                    "INTERNAL mir-wellformed: op {i} ({op:?}) in `{}` reads {:?} \
                     before any definition",
                    func.name, r
                ));
            }
        }
        if let Some(d) = defined_value(op) {
            defined.insert(d);
        }
        // `SetLocal` redefines an existing slot — its `local` was just checked
        // as a read, so the slot is known-defined; nothing new to insert.
        let _ = matches!(op, Op::SetLocal { .. });
    }
    if let Some(r) = func.ret {
        if !defined.contains(&r) {
            return Err(format!(
                "INTERNAL mir-wellformed: `{}` returns {:?}, which no op defines",
                func.name, r
            ));
        }
    }
    Ok(())
}

/// The [`Op::Return`] TERMINAL discipline (law 6). Four rules, each keeping a
/// downstream consumer simple enough to verify:
///
/// 1. `Return` sits inside an `IfThen` arm, never at top level — a top-level
///    early return IS the tail, and the certificate's divergence marker only
///    has meaning inside a branch bracket.
/// 2. Nothing follows a `Return` in its arm (the next op is the arm's closing
///    `Else`/`EndIf`) — the verifier's diverged flag is consumed by exactly
///    the next marker, and the renderers never emit dead code after it.
/// 3. Both arms of one `IfThen` must not both end in `Return` — a both-return
///    branch is the tail in disguise, and it would leave the merge
///    continuation (and the fn's tail `ret`) unreachable-but-accounted.
/// 4. The `Return`'s value presence matches the fn's `ret` presence (the wasm
///    `(result …)` signature is one or the other for the whole function).
pub(crate) fn check_return_terminal(func: &MirFunction) -> Result<(), String> {
    struct Fr {
        then_returned: bool,
        in_else: bool,
    }
    let mut stack: Vec<Fr> = Vec::new();
    let mut prev_was_return = false;
    for (i, op) in func.ops.iter().enumerate() {
        if prev_was_return && !matches!(op, Op::Else { .. } | Op::EndIf { .. }) {
            return Err(format!(
                "INTERNAL mir-wellformed: op {i} ({op:?}) in `{}` follows a `Return` \
                 inside the same arm — `Return` is terminal",
                func.name
            ));
        }
        match op {
            Op::Return { val } => {
                if stack.is_empty() {
                    return Err(format!(
                        "INTERNAL mir-wellformed: op {i} in `{}` is a top-level `Return` \
                         — the tail already returns; `Return` must sit inside an `IfThen` arm",
                        func.name
                    ));
                }
                if val.is_some() != func.ret.is_some() {
                    return Err(format!(
                        "INTERNAL mir-wellformed: op {i} in `{}`: `Return` value presence \
                         ({val:?}) does not match the fn's ret ({:?})",
                        func.name, func.ret
                    ));
                }
                prev_was_return = true;
                continue;
            }
            Op::IfThen { .. } => stack.push(Fr { then_returned: false, in_else: false }),
            Op::Else { .. } => {
                if let Some(fr) = stack.last_mut() {
                    fr.then_returned = prev_was_return;
                    fr.in_else = true;
                }
            }
            Op::EndIf { .. } => {
                if let Some(fr) = stack.pop() {
                    let then_returned = fr.then_returned || (prev_was_return && !fr.in_else);
                    let else_returned = prev_was_return && fr.in_else;
                    if then_returned && else_returned {
                        return Err(format!(
                            "INTERNAL mir-wellformed: op {i} in `{}`: both arms of one \
                             `IfThen` end in `Return` — lower the both-return shape as \
                             the ordinary tail instead",
                            func.name
                        ));
                    }
                }
            }
            _ => {}
        }
        prev_was_return = false;
    }
    Ok(())
}

/// Negative controls: proofs the verifier FIRES (the swift
/// `verifier-fail-*.sil` pattern). A gate that has never been seen
/// rejecting anything is indistinguishable from `Ok(())`; each test here
/// hands the checker a minimal violating function and asserts the named
/// wall comes back.
#[cfg(test)]
mod tests {
    use super::check_def_before_use;
    use crate::{MirFunction, Op, ValueId};

    fn func(ops: Vec<Op>, ret: Option<ValueId>) -> MirFunction {
        MirFunction {
            name: "negctl".to_string(),
            ops,
            ret,
            ..MirFunction::default()
        }
    }

    #[test]
    fn well_formed_function_passes() {
        let f = func(
            vec![
                Op::Const { dst: ValueId(0) },
                Op::Dup { dst: ValueId(1), src: ValueId(0) },
            ],
            Some(ValueId(1)),
        );
        assert_eq!(check_def_before_use(&f), Ok(()));
    }

    #[test]
    fn read_before_definition_is_a_named_wall() {
        let f = func(vec![Op::Dup { dst: ValueId(1), src: ValueId(0) }], None);
        let err = check_def_before_use(&f).expect_err("undefined read must be rejected");
        assert!(
            err.contains("INTERNAL mir-wellformed") && err.contains("before any definition"),
            "wall must be named and cite the violation, got: {err}"
        );
    }

    #[test]
    fn undefined_return_value_is_a_named_wall() {
        let f = func(vec![Op::Const { dst: ValueId(0) }], Some(ValueId(9)));
        let err = check_def_before_use(&f).expect_err("undefined return must be rejected");
        assert!(
            err.contains("INTERNAL mir-wellformed") && err.contains("no op defines"),
            "wall must be named and cite the violation, got: {err}"
        );
    }
}
