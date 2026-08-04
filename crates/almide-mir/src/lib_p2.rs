// ──────────────────────────────── tests ───────────────────────────────────
//
// The Phase 0 decision gate (research/spike/v1-mir/) proved, in a standalone
// spike, that one ownership decision per shape renders faithfully to both
// idioms. Here those five shapes are encoded as REAL MIR and checked by the
// REAL verifier: the balanced skeleton verifies clean, and a renderer-style
// "re-decision" (a dropped Drop, a deep free, a consume-on-call) is caught.

#[cfg(test)]
mod tests {
    use super::*;

    fn v(n: u32) -> ValueId {
        ValueId(n)
    }
    fn heap() -> Repr {
        Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
    }
    fn func(ops: Vec<Op>) -> MirFunction {
        MirFunction { name: "shape".into(), ops, ..Default::default() }
    }

    // Shape 2 — list_get_643: a per-iteration heap temp `t` is alloc'd and
    // consumed (pushed into `out`); the alias `nx` is dup'd and dropped at
    // scope end. The leak of a per-iteration temp is exactly #643's class.
    fn shape_643() -> MirFunction {
        let (nx, t) = (v(0), v(1));
        func(vec![
            Op::Alloc { dst: nx, repr: heap(), init: Init::Opaque }, // nx acquires its own ref (alias-inc)
            Op::Alloc { dst: t, repr: heap(), init: Init::Opaque },  // the slice|>join temp
            Op::Consume { v: t },                // pushed into `out` (moved)
            Op::Borrow { v: nx },                // used
            Op::Drop { v: nx },                  // scope end
        ])
    }

    // Shape 1 — alias_return: move the payload OUT (consume), free the shell
    // ONLY. A renderer that deep-frees the returned payload double-frees.
    fn shape_alias_return() -> MirFunction {
        let (payload, shell) = (v(0), v(1));
        func(vec![
            Op::Alloc { dst: payload, repr: heap(), init: Init::Opaque },
            Op::Alloc { dst: shell, repr: heap(), init: Init::Opaque },
            Op::Consume { v: payload }, // transferred to the caller (returned)
            Op::Drop { v: shell },      // free the Option shell only
        ])
    }

    // Shape 3 — boxed_pattern_610: read through the box is a Borrow (no
    // dup/drop of the child); the Leaf payload is a Scalar. One Drop of the node.
    fn shape_boxed_pattern() -> MirFunction {
        let (node, a) = (v(0), v(1));
        func(vec![
            Op::Alloc { dst: node, repr: heap(), init: Init::Opaque },
            Op::Const { dst: a },         // scalar leaf payload (Borrow-through-box copy)
            Op::Borrow { v: node },       // the nested read
            Op::Pure { dst: v(2), uses: vec![a, node] }, // e.g. a + node-tag use
            Op::Drop { v: node },         // scope end
        ])
    }

    // Shape 4 — closure_capture: capture = dup into env (a new handle `env`
    // sharing x's object); each call borrows the env handle; env-drop and the
    // original drop release the two refs. Read-only, callable twice (Fn).
    fn shape_closure_capture() -> MirFunction {
        let (x, env) = (v(0), v(1));
        func(vec![
            Op::Alloc { dst: x, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: env, src: x }, // capture into the closure env
            Op::Borrow { v: env },        // call 1
            Op::Borrow { v: env },        // call 2
            Op::Drop { v: env },          // closure/env drop
            Op::Drop { v: x },            // original drop
        ])
    }

    // Shape 5 — alias_cow: `b` aliases `a` (a new handle sharing a's object),
    // MakeUnique before the in-place mutate, both handles dropped. (The AliasCow
    // *value* bug is wrong-output with the refcount BALANCED — caught by the
    // semantic-law oracle, finding #3 — so the ownership skeleton here is,
    // correctly, balanced.)
    fn shape_alias_cow() -> MirFunction {
        let (a, b) = (v(0), v(1));
        func(vec![
            Op::Alloc { dst: a, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: b, src: a }, // b aliases a (object now shared, rc 2)
            Op::MakeUnique { v: a },    // clone-on-shared before mutating
            Op::Drop { v: a },          // a
            Op::Drop { v: b },          // b
        ])
    }

    #[test]
    fn all_five_gate_shapes_verify_balanced() {
        for f in [
            shape_643(),
            shape_alias_return(),
            shape_boxed_pattern(),
            shape_closure_capture(),
            shape_alias_cow(),
        ] {
            assert_eq!(verify_ownership(&f), Ok(()), "shape `{}` must verify clean", f.name);
        }
    }

    #[test]
    fn dropped_drop_is_caught_as_leak() {
        // #643 with the per-iteration alias Drop omitted (the renderer-side leak).
        let mut f = shape_643();
        f.ops.retain(|op| !matches!(op, Op::Drop { .. }));
        let errs = verify_ownership(&f).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::Leak && e.value == ValueId(0)));
    }

    #[test]
    fn deep_free_of_a_moved_payload_is_caught() {
        // alias_return where the renderer ALSO frees the returned payload.
        let mut f = shape_alias_return();
        f.ops.push(Op::Drop { v: ValueId(0) }); // drop after consume
        let errs = verify_ownership(&f).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::DoubleFree && e.value == ValueId(0)));
    }

    #[test]
    fn capture_consumed_on_call_over_releases() {
        // closure_capture mis-modeled: a call CONSUMES the env capture handle,
        // so the 2nd call uses an already-moved handle (the re-decision is caught
        // — UseAfterMove here; the point is it does not pass silently).
        let (x, env) = (ValueId(0), ValueId(1));
        let f = func(vec![
            Op::Alloc { dst: x, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: env, src: x },
            Op::Consume { v: env }, // call 1 wrongly consumes the capture
            Op::Consume { v: env }, // call 2 — env already moved
            Op::Drop { v: x },
        ]);
        let errs = verify_ownership(&f).unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e.kind, ViolationKind::UseAfterMove | ViolationKind::DoubleFree)));
    }

    #[test]
    fn use_after_free_is_caught() {
        let x = ValueId(0);
        let f = func(vec![
            Op::Alloc { dst: x, repr: heap(), init: Init::Opaque },
            Op::Drop { v: x },   // freed
            Op::Borrow { v: x }, // used after free
        ]);
        let errs = verify_ownership(&f).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::UseAfterFree));
    }

    #[test]
    fn scalars_need_no_ownership() {
        // A Const used by a Pure must not be flagged (no refcount on scalars).
        let f = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Pure { dst: ValueId(1), uses: vec![ValueId(0)] },
        ]);
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn repr_heap_predicate() {
        assert!(heap().is_heap());
        assert!(Repr::Boxed { layout: PLACEHOLDER_LAYOUT }.is_heap());
        assert!(!Repr::Scalar { width: ScalarWidth::Double }.is_heap());
    }

    // ── #1037: the payload-borrow chain and the heap branch result ──

    /// The `option.unwrap_or` payload chain: `prim.handle(opt) + 12` →
    /// `LoadHandle` (the BORROWED payload) → `Dup` (a fresh owned ref) → reads
    /// → `Drop`. Pre-#1037 the address `IntBinOp` broke the alias chain, the
    /// loaded handle fell off the model, and the `Dup` was a phantom
    /// UseAfterFree.
    fn shape_payload_borrow() -> MirFunction {
        let (opt, addr_h, off, addr, payload, owned) = (v(0), v(1), v(2), v(3), v(4), v(5));
        func(vec![
            Op::Alloc { dst: opt, repr: heap(), init: Init::Opaque },
            Op::Prim { kind: PrimKind::Handle, dst: Some(addr_h), args: vec![opt] },
            Op::ConstInt { dst: off, value: 12 },
            Op::IntBinOp { dst: addr, op: IntOp::Add, a: addr_h, b: off },
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(payload), args: vec![addr] },
            Op::Dup { dst: owned, src: payload },
            Op::ListGetScalar { dst: v(6), list: owned, idx: off },
            Op::Drop { v: owned },
            Op::Drop { v: opt },
        ])
    }

    #[test]
    fn payload_borrow_chain_verifies_balanced() {
        assert_eq!(verify_ownership(&shape_payload_borrow()), Ok(()));
    }

    #[test]
    fn payload_borrow_after_parent_free_is_caught() {
        // The same chain with the PARENT dropped before the Dup: the loaded
        // handle aliases a freed object — must stay a violation, or the
        // widening would accept a real use-after-free.
        let (opt, addr_h, off, addr, payload, owned) = (v(0), v(1), v(2), v(3), v(4), v(5));
        let f = func(vec![
            Op::Alloc { dst: opt, repr: heap(), init: Init::Opaque },
            Op::Prim { kind: PrimKind::Handle, dst: Some(addr_h), args: vec![opt] },
            Op::ConstInt { dst: off, value: 12 },
            Op::IntBinOp { dst: addr, op: IntOp::Add, a: addr_h, b: off },
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(payload), args: vec![addr] },
            Op::Drop { v: opt }, // parent freed FIRST
            Op::Dup { dst: owned, src: payload },
            Op::Drop { v: owned },
        ]);
        let errs = verify_ownership(&f).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::UseAfterFree));
    }

    #[test]
    fn untracked_load_handle_stays_off_the_model() {
        // A LoadHandle through a RAW address (no tracked root): the loaded
        // handle stays unknown, and a Dup of it still reports — the load64
        // floor is not widened (#1037's discipline: unknown, never guessed).
        let (raw, payload, owned) = (v(0), v(1), v(2));
        let f = func(vec![
            Op::ConstInt { dst: raw, value: 8192 },
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(payload), args: vec![raw] },
            Op::Dup { dst: owned, src: payload },
            Op::Drop { v: owned },
        ]);
        let errs = verify_ownership(&f).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::UseAfterFree));
    }

    /// A HEAP branch result: each arm nets 0 (its value is Consumed into the
    /// merge), the join owns the moved reference on the `IfThen` dst, and the
    /// scope-end Consume releases it. Pre-#1037 the dst was unmodeled and the
    /// release was a phantom UseAfterMove.
    fn shape_heap_branch_result() -> MirFunction {
        let (opt, cond, res, payload, owned, lit) = (v(0), v(1), v(2), v(3), v(4), v(5));
        func(vec![
            Op::Alloc { dst: opt, repr: heap(), init: Init::Opaque },
            Op::Prim { kind: PrimKind::Handle, dst: Some(v(6)), args: vec![opt] },
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(payload), args: vec![v(6)] },
            Op::Const { dst: cond },
            Op::IfThen { cond, dst: Some(res) },
            Op::Dup { dst: owned, src: payload },
            Op::Consume { v: owned }, // moved into the merge
            Op::Else { val: Some(owned) },
            Op::ListLit { dst: lit, elems: vec![] },
            Op::Consume { v: lit }, // moved into the merge
            Op::EndIf { val: Some(lit) },
            Op::Drop { v: opt },
            Op::Consume { v: res }, // the branch result, released at scope end
        ])
    }

    #[test]
    fn heap_branch_result_verifies_balanced() {
        assert_eq!(verify_ownership(&shape_heap_branch_result()), Ok(()));
    }

    #[test]
    fn double_release_of_a_branch_result_is_caught() {
        let mut f = shape_heap_branch_result();
        f.ops.push(Op::Drop { v: v(2) }); // a second release of the result
        let errs = verify_ownership(&f).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::DoubleFree && e.value == v(2)));
    }

    #[test]
    fn scalar_branch_result_stays_unmodeled() {
        // A SCALAR IfThen dst (both arm vals untracked) must NOT become an
        // owned object — no phantom Leak at function end.
        let (cond, res) = (v(0), v(1));
        let f = func(vec![
            Op::Const { dst: cond },
            Op::IfThen { cond, dst: Some(res) },
            Op::ConstInt { dst: v(2), value: 1 },
            Op::Else { val: Some(v(2)) },
            Op::ConstInt { dst: v(3), value: 2 },
            Op::EndIf { val: Some(v(3)) },
        ]);
        assert_eq!(verify_ownership(&f), Ok(()));
    }
}
