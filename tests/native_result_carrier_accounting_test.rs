//! #4349: the native Result-window rewrite keeps a MINTED carrier's ownership
//! accounting. It used to turn `Alloc{ResOkScalar|ResErrStr}` into a carrier
//! prim AND delete the carrier's `Drop`/`DropListStr`/`Consume`, leaving the
//! carrier with no object in the ownership mirror. Every borrowing use of it
//! (a call's handle arg, its Err view, a `Dup` of that view) then read as a
//! use-after-free: 17 spec/wasm_cross fixtures walled although the MIR the
//! wasm leg proves was balanced and the rendered Rust holds the carrier as an
//! owned local. Now the carrier prims are alloc-class to the mirror and the
//! certificate, and a carrier read is a dereference that must find it live.

use almide_mir::native_result_rewrite::rewrite_result_ops;
use almide_mir::{
    verify_ownership, Init, MirFunction, Op, PrimKind, Repr, ValueId, ViolationKind, PLACEHOLDER_LAYOUT,
};
use std::collections::BTreeSet;
use std::path::Path;

/// The 17 fixtures the false use-after-free walled. None may wall on the
/// ownership mirror again; the first three render natively outright (the
/// others now stop at an honest subset wall further on — `int op And`,
/// `Alloc DynListStr`, … — which is not this bug).
const FALSE_UAF: &[&str] = &[
    "effect_main_guard_err",
    "tm_res_int_lit_print",
    "tm_res_int_shadow_drop",
    "compound_repr_interp",
    "ctor_if_payload",
    "ctor_var_copy_semantics",
    "effect_main_guard_ok",
    "fan_pure_thunks",
    "fan_var_thunk_list",
    "gleam_guard_matrix",
    "result_ctor_call_payload",
    "result_eq",
    "result_wall_escalations",
    "testing_assert_abort_err",
    "testing_assert_abort_ok",
    "tm_res_int_eq",
    "unwrap_or_strict_default",
];
const RENDERS: usize = 3;

#[test]
fn the_result_carrier_fixtures_pass_the_ownership_mirror() {
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"));
            let mut bad = Vec::new();
            for (i, fx) in FALSE_UAF.iter().enumerate() {
                let p = root.join(format!("spec/wasm_cross/{fx}.almd"));
                let src = std::fs::read_to_string(&p).expect("read the fixture");
                match almide_mir::pipeline::try_render_rust_source(&src) {
                    Ok(_) => {}
                    Err(e) if i < RENDERS => bad.push(format!("{fx} must render: {e}")),
                    Err(e) if e.to_string().contains("ownership verification failed") => {
                        bad.push(format!("{fx}: {e}"))
                    }
                    Err(_) => {}
                }
            }
            assert!(bad.is_empty(), "{bad:#?}");
        })
        .expect("spawn")
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e));
}

fn v(n: u32) -> ValueId {
    ValueId(n)
}

fn func(ops: Vec<Op>) -> MirFunction {
    MirFunction { name: "main".into(), ops, ..Default::default() }
}

fn prim(kind: PrimKind, dst: u32, arg: u32) -> Op {
    Op::Prim { kind, dst: Some(v(dst)), args: vec![v(arg)] }
}

fn kinds(f: &MirFunction) -> Vec<ViolationKind> {
    verify_ownership(f).err().unwrap_or_default().into_iter().map(|x| x.kind).collect()
}

/// The lowered `ok(5)` printed through its Err view, as the shared lowering
/// emits it (tm_res_int_lit_print, reduced): tag read, Err view dup'd for the
/// print, the carrier released at scope end.
fn lowered_ok_err_view() -> MirFunction {
    let ptr = Repr::Ptr { layout: PLACEHOLDER_LAYOUT };
    func(vec![
        Op::ConstInt { dst: v(0), value: 5 },
        Op::Alloc { dst: v(1), repr: ptr, init: Init::ResOkScalar { payload: v(0) } },
        Op::Prim { kind: PrimKind::Handle, dst: Some(v(2)), args: vec![v(1)] },
        Op::ConstInt { dst: v(3), value: 4 },
        Op::IntBinOp { dst: v(4), op: almide_mir::IntOp::Add, a: v(2), b: v(3) },
        Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(v(5)), args: vec![v(4)] },
        Op::IfThen { cond: v(5), dst: None },
        Op::ConstInt { dst: v(6), value: 12 },
        Op::IntBinOp { dst: v(7), op: almide_mir::IntOp::Add, a: v(2), b: v(6) },
        Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(8)), args: vec![v(7)] },
        Op::Dup { dst: v(9), src: v(8) },
        Op::Call { dst: None, func: almide_mir::RtFn::PrintStr, args: vec![almide_mir::CallArg::Handle(v(9))], result: None },
        Op::Drop { v: v(9) },
        Op::Else { val: None },
        Op::EndIf { val: None },
        Op::DropListStr { v: v(1) },
    ])
}

#[test]
fn the_rewrite_keeps_a_minted_carriers_release_and_verifies() {
    let mut f = lowered_ok_err_view();
    assert_eq!(kinds(&f), vec![], "the lowered MIR is balanced");
    rewrite_result_ops(&mut f, &BTreeSet::new(), &[]);
    assert!(
        f.ops.iter().any(|op| matches!(op, Op::Prim { kind: PrimKind::ResMakeOk, .. })),
        "the producer became the carrier prim: {:#?}",
        f.ops
    );
    assert!(
        f.ops.iter().any(|op| matches!(op, Op::DropListStr { v } if *v == ValueId(1))),
        "the minted carrier keeps its release: {:#?}",
        f.ops
    );
    assert_eq!(kinds(&f), vec![], "the rewritten MIR verifies: {:#?}", f.ops);
}

/// NEGATIVE CONTROL: a carrier read after the carrier's release is a
/// use-after-free — the mirror is not made blind to carriers, it counts them.
#[test]
fn a_carrier_read_after_its_release_is_a_use_after_free() {
    for read in [PrimKind::ResTag, PrimKind::ResOkScalar, PrimKind::ResErrStr, PrimKind::ResCopy] {
        let f = func(vec![
            Op::ConstInt { dst: v(0), value: 5 },
            prim(PrimKind::ResMakeOk, 1, 0),
            Op::DropListStr { v: v(1) },
            prim(read, 2, 1),
        ]);
        assert!(
            kinds(&f).contains(&ViolationKind::UseAfterFree),
            "{read:?} after the release must read UseAfterFree, got {:?}",
            kinds(&f)
        );
    }
}

/// NEGATIVE CONTROL: a minted carrier with no release leaks, and a second
/// release is a double free.
#[test]
fn a_carrier_is_released_exactly_once() {
    let leak = func(vec![Op::ConstInt { dst: v(0), value: 5 }, prim(PrimKind::ResMakeOk, 1, 0)]);
    assert_eq!(kinds(&leak), vec![ViolationKind::Leak]);
    let twice = func(vec![
        Op::ConstInt { dst: v(0), value: 5 },
        prim(PrimKind::ResMakeOk, 1, 0),
        Op::DropListStr { v: v(1) },
        Op::DropListStr { v: v(1) },
    ]);
    assert!(kinds(&twice).contains(&ViolationKind::DoubleFree), "{:?}", kinds(&twice));
}

/// A carrier COPY is its own object: each copy is released on its own.
#[test]
fn a_carrier_copy_is_released_on_its_own() {
    let f = func(vec![
        Op::ConstInt { dst: v(0), value: 5 },
        prim(PrimKind::ResMakeOk, 1, 0),
        prim(PrimKind::ResCopy, 2, 1),
        Op::DropListStr { v: v(1) },
        prim(PrimKind::ResTag, 3, 2),
        Op::DropListStr { v: v(2) },
    ]);
    assert_eq!(kinds(&f), vec![]);
}
