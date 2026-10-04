//! Unit tests for `js_host.rs` (#3352): the wrapper's return plan.

use super::*;

fn export(name: &str, ret: Ty, is_effect: bool) -> HostFn {
    HostFn { name: name.to_string(), params: Vec::new(), ret, is_effect }
}

/// The wrapper follows the return ABI the emitter recorded (#3352): an
/// effect fn's Result block is unwrapped for every marshalled type, and a
/// record that disagrees with the source type is refused by name — the
/// #3352 shape (an effect fn read as a plain String) can no longer be
/// emitted even if the emitter's convention moved.
#[test]
fn the_return_plan_is_the_recorded_abi_or_a_refusal() {
    let cells = [
        (Ty::Int, AbiKind::Int, Marshal::Int),
        (Ty::Float, AbiKind::Float, Marshal::Float),
        (Ty::Bool, AbiKind::Bool, Marshal::Bool),
        (Ty::String, AbiKind::Str, Marshal::Str),
        (Ty::Unit, AbiKind::Unit, Marshal::Unit),
    ];
    for (ty, kind, m) in cells {
        let effect = export("e", ty.clone(), true);
        let rec = ExportRet::Result(kind.clone(), AbiKind::Str);
        assert_eq!(ret_plan(&effect, Some(&rec)), Ok(RetPlan::Unwrap(m)), "{ty:?}");
        let plain = export("p", ty.clone(), false);
        let rec = if m == Marshal::Unit { ExportRet::Void } else { ExportRet::Value(kind.clone()) };
        assert_eq!(ret_plan(&plain, Some(&rec)), Ok(RetPlan::Plain(m)), "{ty:?}");
        // Disagreements are refusals, in both directions.
        let e = ret_plan(&effect, Some(&ExportRet::Value(kind.clone()))).unwrap_err();
        assert!(e.contains("cannot wrap the return of `e`"), "{e}");
        let e = ret_plan(&plain, Some(&ExportRet::Result(kind.clone(), AbiKind::Str))).unwrap_err();
        assert!(e.contains("cannot wrap the return of `p`"), "{e}");
        assert!(ret_plan(&effect, None).is_err(), "no record, no wrapper");
    }
    // An err slot the host cannot read as a message is refused.
    let effect = export("e", Ty::Int, true);
    let rec = ExportRet::Result(AbiKind::Int, AbiKind::Other("Named(0)".into()));
    assert!(ret_plan(&effect, Some(&rec)).is_err());
}
