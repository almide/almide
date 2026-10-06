//! Unit tests for `js_host.rs` (#3352, #3354): the wrapper plan follows the
//! boundary ABI the emitter recorded, or the build refuses.

use super::exports::{plan_export, ParamPlan, RetPlan};
use super::*;
use almide_lang::types::constructor::TypeConstructorId;

fn export(name: &str, params: Vec<Ty>, ret: Ty, is_effect: bool) -> HostFn {
    let params = params.into_iter().enumerate().map(|(i, t)| (format!("a{i}"), t)).collect();
    HostFn { name: name.to_string(), params, ret, is_effect }
}

fn list(t: Ty) -> Ty {
    Ty::Applied(TypeConstructorId::List, vec![t])
}

fn option(t: Ty) -> Ty {
    Ty::Applied(TypeConstructorId::Option, vec![t])
}

/// The #3352 cells: an effect fn's Result block is unwrapped for every
/// scalar type, and a record that disagrees with the source type is refused
/// by name — the #3352 shape (an effect fn read as a plain String) can no
/// longer be emitted even if the emitter's convention moved.
#[test]
fn the_return_plan_is_the_recorded_abi_or_a_refusal() {
    let cells = [
        (Ty::Int, AbiShape::Int, Marshal::Int),
        (Ty::Float, AbiShape::Float, Marshal::Float),
        (Ty::Bool, AbiShape::Bool, Marshal::Bool),
        (Ty::String, AbiShape::Str, Marshal::Str),
    ];
    for (ty, shape, m) in cells {
        let effect = export("e", vec![], ty.clone(), true);
        let rec = ExportRet::Result(shape.clone(), AbiShape::Str);
        assert_eq!(plan_export(&effect, Some(&[]), Some(&rec)).map(|p| p.ret), Ok(RetPlan::Unwrap(shape.clone())), "{ty:?}");
        let plain = export("p", vec![], ty.clone(), false);
        assert_eq!(plan_export(&plain, Some(&[]), Some(&ExportRet::Value(shape.clone()))).map(|p| p.ret), Ok(RetPlan::Scalar(m)), "{ty:?}");
        let e = plan_export(&effect, Some(&[]), Some(&ExportRet::Value(shape.clone()))).unwrap_err();
        assert!(e.contains("cannot wrap the return of `e`"), "{e}");
        let e = plan_export(&plain, Some(&[]), Some(&ExportRet::Result(shape.clone(), AbiShape::Str))).unwrap_err();
        assert!(e.contains("cannot wrap the return of `p`"), "{e}");
        assert!(plan_export(&effect, Some(&[]), None).is_err(), "no record, no wrapper");
    }
    let unit = export("u", vec![], Ty::Unit, false);
    assert_eq!(plan_export(&unit, Some(&[]), Some(&ExportRet::Void)).map(|p| p.ret), Ok(RetPlan::Void));
    let effect = export("e", vec![], Ty::Int, true);
    let rec = ExportRet::Result(AbiShape::Int, AbiShape::Other("Named(0)".into()));
    assert!(plan_export(&effect, Some(&[]), Some(&rec)).is_err(), "an err the host cannot read as a message");
}

/// #3354: Bytes, List, Option and records are blocks read by the recorded
/// layout, as params and returns; a shape that disagrees with the source
/// type, or that holds something unsupported, is refused naming the slot.
#[test]
fn block_shapes_plan_from_the_record_and_refuse_a_disagreement() {
    let ints = AbiShape::List { el: Box::new(AbiShape::Int), stride: 8 };
    let point = AbiShape::Record { name: "Point".into(), size: 16, fields: vec![("x".into(), 0, AbiShape::Int), ("y".into(), 8, AbiShape::Int)] };
    let cells = [
        (Ty::Bytes, AbiShape::Bytes),
        (list(Ty::Int), ints.clone()),
        (list(Ty::String), AbiShape::List { el: Box::new(AbiShape::Str), stride: 4 }),
        (option(Ty::Int), AbiShape::Option(Box::new(AbiShape::Int))),
        (Ty::Named(almide_base::intern::sym("Point"), vec![]), point.clone()),
    ];
    for (ty, shape) in cells {
        let f = export("f", vec![ty.clone()], ty.clone(), false);
        let plan = plan_export(&f, Some(std::slice::from_ref(&shape)), Some(&ExportRet::Value(shape.clone()))).expect("a recorded block shape plans");
        assert_eq!(plan.params, vec![ParamPlan::Block(shape.clone())], "{ty:?}");
        assert_eq!(plan.ret, RetPlan::Block(shape.clone()), "{ty:?}");
        assert!(plan.needs_values());
        let e = export("e", vec![], ty.clone(), true);
        let plan = plan_export(&e, Some(&[]), Some(&ExportRet::Result(shape.clone(), AbiShape::Str))).expect("an effect fn over a block shape plans");
        assert_eq!(plan.ret, RetPlan::Unwrap(shape.clone()), "{ty:?}");
    }
    // A List[Int] source recorded as List[Float]: refused, naming the param.
    let f = export("f", vec![list(Ty::Int)], Ty::Unit, false);
    let floats = AbiShape::List { el: Box::new(AbiShape::Float), stride: 8 };
    let e = plan_export(&f, Some(&[floats]), Some(&ExportRet::Void)).unwrap_err();
    assert!(e.contains("cannot wrap parameter `a0` of `f`"), "{e}");
    // A record of another name, and a variant: refused.
    let other = AbiShape::Record { name: "Other".into(), size: 0, fields: vec![] };
    let named = Ty::Named(almide_base::intern::sym("Point"), vec![]);
    let f = export("f", vec![named.clone()], Ty::Unit, false);
    assert!(plan_export(&f, Some(&[other]), Some(&ExportRet::Void)).is_err());
    let variant = AbiShape::Other("variant `Shape`".into());
    let e = plan_export(&f, Some(&[variant]), Some(&ExportRet::Void)).unwrap_err();
    assert!(e.contains("variant `Shape`"), "{e}");
    // A parameter count that disagrees with the source.
    assert!(plan_export(&f, Some(&[]), Some(&ExportRet::Void)).is_err());
}
