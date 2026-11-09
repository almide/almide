use super::*;
use almide_lang::types::{constructor::TypeConstructorId as C, Ty};

fn applied(c: C, t: Vec<Ty>) -> Ty {
    Ty::Applied(c, t.into())
}

#[test]
fn nested_carriers_keep_their_declared_payload_types() {
    for scalar in [Ty::Int, Ty::Bool, Ty::Float, Ty::Float32, Ty::UInt8] {
        let inner = applied(C::Option, vec![scalar]);
        let outer = applied(C::Option, vec![inner.clone()]);
        let list = applied(C::List, vec![outer.clone()]);
        let result = applied(C::Result, vec![inner.clone(), Ty::String]);
        let mut roots = Vec::new();
        for ty in [&inner, &outer, &list, &result] {
            collect_tuples(ty, &mut roots);
        }
        let decls = derive_records(&Default::default(), &Default::default(), &roots);
        let get = |ty: &Ty| {
            decls
                .iter()
                .find(|d| d.name == tuple_key(ty))
                .expect("declared carrier was pruned")
        };
        assert_eq!(get(&outer).option_of, Some(NTy::Opt));
        assert_eq!(get(&result).result_of, Some((NTy::Opt, NTy::Str)));
        let index = decls
            .iter()
            .position(|d| d.name == tuple_key(&outer))
            .expect("the outer tuple is declared") as u16;
        assert_eq!(get(&list).list_of, Some(NTy::Rec(index)));
    }
}

#[test]
fn result_elements_do_not_require_fabricated_defaults() {
    for (payload, kind) in [(Ty::Int, NTy::Res), (Ty::String, NTy::ResStr)] {
        let result = applied(C::Result, vec![payload, Ty::String]);
        let list = applied(C::List, vec![result.clone()]);
        let option = applied(C::Option, vec![result]);
        let mut roots = Vec::new();
        collect_tuples(&list, &mut roots);
        collect_tuples(&option, &mut roots);
        let decls = derive_records(&Default::default(), &Default::default(), &roots);
        assert_eq!(
            decls
                .iter()
                .find(|d| d.name == tuple_key(&list))
                .unwrap()
                .list_of,
            Some(kind)
        );
        assert_eq!(
            decls
                .iter()
                .find(|d| d.name == tuple_key(&option))
                .unwrap()
                .option_of,
            Some(kind)
        );
        assert!(!has_default(kind));
    }
}

#[test]
fn result_fields_do_not_give_records_a_default() {
    let _guard = install(vec![
        RecDecl {
            name: "result-field".into(),
            fields: vec![NTy::Res],
            ..Default::default()
        },
        RecDecl {
            name: "outer-record".into(),
            fields: vec![NTy::Rec(0)],
            ..Default::default()
        },
        RecDecl {
            name: "list-of-records".into(),
            list_of: Some(NTy::Rec(1)),
            ..Default::default()
        },
        RecDecl {
            name: "optional-record".into(),
            option_of: Some(NTy::Rec(1)),
            ..Default::default()
        },
    ]);
    assert!(!has_default(NTy::Rec(0)));
    assert!(!has_default(NTy::Rec(1)));
    assert!(has_default(NTy::Rec(2)));
    assert!(has_default(NTy::Rec(3)));
}

#[test]
fn tuples_with_result_fields_are_constructible_without_default() {
    for payload in [Ty::Int, Ty::String] {
        let result = applied(C::Result, vec![payload, Ty::String]);
        let tuple = Ty::Tuple(vec![result, Ty::String].into());
        let mut roots = Vec::new();
        collect_tuples(&tuple, &mut roots);
        let decls = derive_records(&Default::default(), &Default::default(), &roots);
        let i = decls.iter().position(|d| d.name == tuple_key(&tuple)).expect("tuple was pruned");
        let _guard = install(decls);
        assert!(!has_default(NTy::Rec(i as u16)));
    }
}

#[test]
fn unit_fields_keep_the_zero_lane_without_changing_void_returns() {
    let tuple = Ty::Tuple(vec![Ty::Unit, Ty::String].into());
    let result = applied(C::Result, vec![tuple.clone(), Ty::String]);
    let mut roots = Vec::new();
    collect_tuples(&result, &mut roots);
    let decls = derive_records(&Default::default(), &Default::default(), &roots);
    let i = decls.iter().position(|d| d.name == tuple_key(&tuple)).unwrap();
    assert_eq!(decls[i].fields, vec![NTy::I64, NTy::Str]);
    assert_eq!(decls.iter().find(|d| d.name == tuple_key(&result)).unwrap().result_of,
        Some((NTy::Rec(i as u16), NTy::Str)));
    assert!(!is_lane_scalar(&Ty::Unit));
    assert!(!is_lane_scalar(&Ty::Never));
}
