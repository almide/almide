use super::*;

#[test]
fn indirect_arguments_allocated_payloads_and_early_returns_are_live() {
    let v = ValueId;
    let f = MirFunction {
        ops: vec![
            Op::CallIndirect { dst: None, table_idx: v(1),
                args: vec![CallArg::Handle(v(2)), CallArg::Handle(v(3))], result: None },
            Op::Alloc { dst: v(4), repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: crate::Init::OptSome { payload: v(5) } },
            Op::Return { val: Some(v(6)) },
            Op::Drop { v: v(7) },
            Op::Consume { v: v(8) },
        ],
        ..Default::default()
    };
    assert_eq!(native_used_values(&f), [v(1), v(2), v(3), v(5), v(6)].into_iter().collect());
}
