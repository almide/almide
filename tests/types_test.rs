use almide::types::{Ty, TypeEnv, FnSig, VariantCase, VariantPayload, unify, substitute};
use std::collections::HashMap;

// ---- Ty::display ----

#[test]
fn display_primitives() {
    assert_eq!(Ty::Int.display(), "Int");
    assert_eq!(Ty::Float.display(), "Float");
    assert_eq!(Ty::String.display(), "String");
    assert_eq!(Ty::Bool.display(), "Bool");
    assert_eq!(Ty::Unit.display(), "Unit");
    assert_eq!(Ty::Unknown.display(), "Unknown");
}

#[test]
fn display_list() {
    assert_eq!(Ty::List(Box::new(Ty::Int)).display(), "List[Int]");
}

#[test]
fn display_option() {
    assert_eq!(Ty::Option(Box::new(Ty::String)).display(), "Option[String]");
}

#[test]
fn display_result() {
    assert_eq!(
        Ty::Result(Box::new(Ty::Int), Box::new(Ty::String)).display(),
        "Result[Int, String]"
    );
}

#[test]
fn display_map() {
    assert_eq!(
        Ty::Map(Box::new(Ty::String), Box::new(Ty::Int)).display(),
        "Map[String, Int]"
    );
}

#[test]
fn display_tuple() {
    assert_eq!(
        Ty::Tuple(vec![Ty::Int, Ty::String]).display(),
        "(Int, String)"
    );
}

#[test]
fn display_fn() {
    let ty = Ty::Fn {
        params: vec![Ty::Int, Ty::Int],
        ret: Box::new(Ty::Bool),
    };
    assert_eq!(ty.display(), "fn(Int, Int) -> Bool");
}

#[test]
fn display_record() {
    let ty = Ty::Record {
        fields: vec![
            ("name".into(), Ty::String),
            ("age".into(), Ty::Int),
        ],
    };
    assert_eq!(ty.display(), "{ name: String, age: Int }");
}

#[test]
fn display_variant() {
    let ty = Ty::Variant {
        name: "Color".into(),
        cases: vec![],
    };
    assert_eq!(ty.display(), "Color");
}

#[test]
fn display_named() {
    assert_eq!(Ty::Named("MyType".into(), vec![]).display(), "MyType");
}

#[test]
fn display_type_var() {
    assert_eq!(Ty::TypeVar("T".into()).display(), "T");
}

#[test]
fn display_nested() {
    let ty = Ty::List(Box::new(Ty::Option(Box::new(Ty::Int))));
    assert_eq!(ty.display(), "List[Option[Int]]");
}

// ---- Ty::compatible ----

#[test]
fn compatible_same_primitives() {
    assert!(Ty::Int.compatible(&Ty::Int));
    assert!(Ty::Float.compatible(&Ty::Float));
    assert!(Ty::String.compatible(&Ty::String));
    assert!(Ty::Bool.compatible(&Ty::Bool));
    assert!(Ty::Unit.compatible(&Ty::Unit));
}

#[test]
fn compatible_different_primitives() {
    assert!(!Ty::Int.compatible(&Ty::Float));
    assert!(!Ty::String.compatible(&Ty::Bool));
    assert!(!Ty::Int.compatible(&Ty::String));
}

#[test]
fn compatible_unknown_matches_all() {
    assert!(Ty::Unknown.compatible(&Ty::Int));
    assert!(Ty::Int.compatible(&Ty::Unknown));
    assert!(Ty::Unknown.compatible(&Ty::Unknown));
}

#[test]
fn compatible_typevar_matches_all() {
    assert!(Ty::TypeVar("T".into()).compatible(&Ty::Int));
    assert!(Ty::String.compatible(&Ty::TypeVar("U".into())));
}

#[test]
fn compatible_list() {
    assert!(Ty::List(Box::new(Ty::Int)).compatible(&Ty::List(Box::new(Ty::Int))));
    assert!(!Ty::List(Box::new(Ty::Int)).compatible(&Ty::List(Box::new(Ty::String))));
}

#[test]
fn compatible_option() {
    assert!(Ty::Option(Box::new(Ty::Int)).compatible(&Ty::Option(Box::new(Ty::Int))));
    assert!(!Ty::Option(Box::new(Ty::Int)).compatible(&Ty::Option(Box::new(Ty::String))));
}

#[test]
fn compatible_result() {
    let r1 = Ty::Result(Box::new(Ty::Int), Box::new(Ty::String));
    let r2 = Ty::Result(Box::new(Ty::Int), Box::new(Ty::String));
    let r3 = Ty::Result(Box::new(Ty::Bool), Box::new(Ty::String));
    assert!(r1.compatible(&r2));
    assert!(!r1.compatible(&r3));
}

#[test]
fn compatible_map() {
    let m1 = Ty::Map(Box::new(Ty::String), Box::new(Ty::Int));
    let m2 = Ty::Map(Box::new(Ty::String), Box::new(Ty::Int));
    let m3 = Ty::Map(Box::new(Ty::Int), Box::new(Ty::Int));
    assert!(m1.compatible(&m2));
    assert!(!m1.compatible(&m3));
}

#[test]
fn compatible_fn_types() {
    let f1 = Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Bool) };
    let f2 = Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Bool) };
    let f3 = Ty::Fn { params: vec![Ty::String], ret: Box::new(Ty::Bool) };
    let f4 = Ty::Fn { params: vec![Ty::Int, Ty::Int], ret: Box::new(Ty::Bool) };
    assert!(f1.compatible(&f2));
    assert!(!f1.compatible(&f3));
    assert!(!f1.compatible(&f4)); // different arity
}

#[test]
fn compatible_tuple() {
    let t1 = Ty::Tuple(vec![Ty::Int, Ty::String]);
    let t2 = Ty::Tuple(vec![Ty::Int, Ty::String]);
    let t3 = Ty::Tuple(vec![Ty::Int, Ty::Int]);
    let t4 = Ty::Tuple(vec![Ty::Int]);
    assert!(t1.compatible(&t2));
    assert!(!t1.compatible(&t3));
    assert!(!t1.compatible(&t4)); // different length
}

#[test]
fn compatible_record() {
    let r1 = Ty::Record { fields: vec![("x".into(), Ty::Int)] };
    let r2 = Ty::Record { fields: vec![("x".into(), Ty::Int)] };
    let r3 = Ty::Record { fields: vec![("y".into(), Ty::Int)] };
    assert!(r1.compatible(&r2));
    assert!(!r1.compatible(&r3));
}

#[test]
fn compatible_named_and_variant() {
    let named = Ty::Named("Color".into(), vec![]);
    let variant = Ty::Variant { name: "Color".into(), cases: vec![] };
    assert!(named.compatible(&variant));
    assert!(variant.compatible(&named));
}

#[test]
fn compatible_different_named() {
    assert!(!Ty::Named("A".into(), vec![]).compatible(&Ty::Named("B".into(), vec![])));
}

// ---- unify ----

#[test]
fn unify_binds_typevar() {
    let mut bindings = HashMap::new();
    assert!(unify(&Ty::TypeVar("T".into()), &Ty::Int, &mut bindings));
    assert_eq!(bindings.get("T"), Some(&Ty::Int));
}

#[test]
fn unify_typevar_consistent() {
    let mut bindings = HashMap::new();
    bindings.insert("T".into(), Ty::Int);
    assert!(unify(&Ty::TypeVar("T".into()), &Ty::Int, &mut bindings));
    assert!(!unify(&Ty::TypeVar("T".into()), &Ty::String, &mut bindings));
}

#[test]
fn unify_list_with_typevar() {
    let mut bindings = HashMap::new();
    let sig = Ty::List(Box::new(Ty::TypeVar("T".into())));
    let actual = Ty::List(Box::new(Ty::Int));
    assert!(unify(&sig, &actual, &mut bindings));
    assert_eq!(bindings.get("T"), Some(&Ty::Int));
}

#[test]
fn unify_result_with_typevars() {
    let mut bindings = HashMap::new();
    let sig = Ty::Result(Box::new(Ty::TypeVar("A".into())), Box::new(Ty::TypeVar("B".into())));
    let actual = Ty::Result(Box::new(Ty::Int), Box::new(Ty::String));
    assert!(unify(&sig, &actual, &mut bindings));
    assert_eq!(bindings.get("A"), Some(&Ty::Int));
    assert_eq!(bindings.get("B"), Some(&Ty::String));
}

#[test]
fn unify_fn_types() {
    let mut bindings = HashMap::new();
    let sig = Ty::Fn {
        params: vec![Ty::TypeVar("T".into())],
        ret: Box::new(Ty::TypeVar("T".into())),
    };
    let actual = Ty::Fn {
        params: vec![Ty::Int],
        ret: Box::new(Ty::Int),
    };
    assert!(unify(&sig, &actual, &mut bindings));
    assert_eq!(bindings.get("T"), Some(&Ty::Int));
}

#[test]
fn unify_fn_arity_mismatch() {
    let mut bindings = HashMap::new();
    let sig = Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) };
    let actual = Ty::Fn { params: vec![Ty::Int, Ty::Int], ret: Box::new(Ty::Int) };
    assert!(!unify(&sig, &actual, &mut bindings));
}

#[test]
fn unify_unknown_always_succeeds() {
    let mut bindings = HashMap::new();
    assert!(unify(&Ty::Unknown, &Ty::Int, &mut bindings));
    assert!(unify(&Ty::Int, &Ty::Unknown, &mut bindings));
}

#[test]
fn unify_tuple() {
    let mut bindings = HashMap::new();
    let sig = Ty::Tuple(vec![Ty::TypeVar("A".into()), Ty::TypeVar("B".into())]);
    let actual = Ty::Tuple(vec![Ty::Int, Ty::String]);
    assert!(unify(&sig, &actual, &mut bindings));
    assert_eq!(bindings.get("A"), Some(&Ty::Int));
    assert_eq!(bindings.get("B"), Some(&Ty::String));
}

#[test]
fn unify_map_with_typevars() {
    let mut bindings = HashMap::new();
    let sig = Ty::Map(Box::new(Ty::TypeVar("K".into())), Box::new(Ty::TypeVar("V".into())));
    let actual = Ty::Map(Box::new(Ty::String), Box::new(Ty::Int));
    assert!(unify(&sig, &actual, &mut bindings));
    assert_eq!(bindings.get("K"), Some(&Ty::String));
    assert_eq!(bindings.get("V"), Some(&Ty::Int));
}

// ---- substitute ----

#[test]
fn substitute_typevar() {
    let mut bindings = HashMap::new();
    bindings.insert("T".into(), Ty::Int);
    let result = substitute(&Ty::TypeVar("T".into()), &bindings);
    assert_eq!(result, Ty::Int);
}

#[test]
fn substitute_unbound_typevar() {
    let mut bindings = HashMap::new();
    bindings.insert("X".into(), Ty::Int); // some other binding, not T
    let result = substitute(&Ty::TypeVar("T".into()), &bindings);
    assert_eq!(result, Ty::TypeVar("T".into()));
}

#[test]
fn substitute_list() {
    let mut bindings = HashMap::new();
    bindings.insert("T".into(), Ty::Int);
    let result = substitute(&Ty::List(Box::new(Ty::TypeVar("T".into()))), &bindings);
    assert_eq!(result, Ty::List(Box::new(Ty::Int)));
}

#[test]
fn substitute_result() {
    let mut bindings = HashMap::new();
    bindings.insert("A".into(), Ty::Int);
    bindings.insert("B".into(), Ty::String);
    let result = substitute(
        &Ty::Result(Box::new(Ty::TypeVar("A".into())), Box::new(Ty::TypeVar("B".into()))),
        &bindings,
    );
    assert_eq!(result, Ty::Result(Box::new(Ty::Int), Box::new(Ty::String)));
}

#[test]
fn substitute_fn_type() {
    let mut bindings = HashMap::new();
    bindings.insert("T".into(), Ty::String);
    let result = substitute(
        &Ty::Fn {
            params: vec![Ty::TypeVar("T".into())],
            ret: Box::new(Ty::TypeVar("T".into())),
        },
        &bindings,
    );
    assert_eq!(
        result,
        Ty::Fn { params: vec![Ty::String], ret: Box::new(Ty::String) }
    );
}

#[test]
fn substitute_tuple() {
    let mut bindings = HashMap::new();
    bindings.insert("A".into(), Ty::Int);
    let result = substitute(
        &Ty::Tuple(vec![Ty::TypeVar("A".into()), Ty::String]),
        &bindings,
    );
    assert_eq!(result, Ty::Tuple(vec![Ty::Int, Ty::String]));
}

#[test]
fn substitute_record() {
    let mut bindings = HashMap::new();
    bindings.insert("T".into(), Ty::Bool);
    let result = substitute(
        &Ty::Record { fields: vec![("x".into(), Ty::TypeVar("T".into()))] },
        &bindings,
    );
    assert_eq!(result, Ty::Record { fields: vec![("x".into(), Ty::Bool)] });
}

#[test]
fn substitute_option() {
    let mut bindings = HashMap::new();
    bindings.insert("T".into(), Ty::Float);
    let result = substitute(&Ty::Option(Box::new(Ty::TypeVar("T".into()))), &bindings);
    assert_eq!(result, Ty::Option(Box::new(Ty::Float)));
}

#[test]
fn substitute_map() {
    let mut bindings = HashMap::new();
    bindings.insert("K".into(), Ty::String);
    bindings.insert("V".into(), Ty::Int);
    let result = substitute(
        &Ty::Map(Box::new(Ty::TypeVar("K".into())), Box::new(Ty::TypeVar("V".into()))),
        &bindings,
    );
    assert_eq!(result, Ty::Map(Box::new(Ty::String), Box::new(Ty::Int)));
}

#[test]
fn substitute_empty_bindings_returns_clone() {
    let bindings = HashMap::new();
    let ty = Ty::List(Box::new(Ty::TypeVar("T".into())));
    let result = substitute(&ty, &bindings);
    // With empty bindings, should return clone unchanged
    assert_eq!(result, ty);
}

#[test]
fn substitute_primitives_unchanged() {
    let mut bindings = HashMap::new();
    bindings.insert("T".into(), Ty::Int);
    assert_eq!(substitute(&Ty::Int, &bindings), Ty::Int);
    assert_eq!(substitute(&Ty::String, &bindings), Ty::String);
    assert_eq!(substitute(&Ty::Unknown, &bindings), Ty::Unknown);
}

// ---- TypeEnv ----

#[test]
fn type_env_scope_push_pop() {
    let mut env = TypeEnv::new();
    env.define_var("x", Ty::Int);
    assert!(env.lookup_var("x").is_some());
    env.push_scope();
    env.define_var("y", Ty::String);
    assert!(env.lookup_var("x").is_some()); // outer scope visible
    assert!(env.lookup_var("y").is_some());
    env.pop_scope();
    assert!(env.lookup_var("y").is_none()); // inner scope gone
    assert!(env.lookup_var("x").is_some());
}

#[test]
fn type_env_shadowing() {
    let mut env = TypeEnv::new();
    env.define_var("x", Ty::Int);
    env.push_scope();
    env.define_var("x", Ty::String);
    assert_eq!(env.lookup_var("x"), Some(&Ty::String));
    env.pop_scope();
    assert_eq!(env.lookup_var("x"), Some(&Ty::Int));
}

#[test]
fn type_env_lookup_missing() {
    let env = TypeEnv::new();
    assert!(env.lookup_var("nonexistent").is_none());
}

#[test]
fn type_env_define_var_at() {
    let mut env = TypeEnv::new();
    env.define_var_at("x", Ty::Int, 5, 3);
    assert_eq!(env.var_decl_loc("x"), Some((5, 3)));
}

#[test]
fn type_env_resolve_named() {
    let mut env = TypeEnv::new();
    env.types.insert("Color".into(), Ty::Variant {
        name: "Color".into(),
        cases: vec![],
    });
    let resolved = env.resolve_named(&Ty::Named("Color".into(), vec![]));
    assert!(matches!(resolved, Ty::Variant { name, .. } if name == "Color"));
}

#[test]
fn type_env_resolve_named_unknown() {
    let env = TypeEnv::new();
    let result = env.resolve_named(&Ty::Named("Unknown".into(), vec![]));
    assert_eq!(result, Ty::Named("Unknown".into(), vec![]));
}

#[test]
fn type_env_resolve_non_named() {
    let env = TypeEnv::new();
    let result = env.resolve_named(&Ty::Int);
    assert_eq!(result, Ty::Int);
}

// ---- FnSig ----

#[test]
fn fn_sig_format_params() {
    let sig = FnSig {
        params: vec![("a".into(), Ty::Int), ("b".into(), Ty::String)],
        ret: Ty::Bool,
        is_effect: false,
        generics: vec![],
        structural_bounds: std::collections::HashMap::new(),
    };
    assert_eq!(sig.format_params(), "a: Int, b: String");
}

#[test]
fn fn_sig_format_params_empty() {
    let sig = FnSig {
        params: vec![],
        ret: Ty::Unit,
        is_effect: false,
        generics: vec![],
        structural_bounds: std::collections::HashMap::new(),
    };
    assert_eq!(sig.format_params(), "");
}

// ---- VariantPayload ----

#[test]
fn variant_payload_eq() {
    assert_eq!(VariantPayload::Unit, VariantPayload::Unit);
    assert_eq!(
        VariantPayload::Tuple(vec![Ty::Int]),
        VariantPayload::Tuple(vec![Ty::Int])
    );
    assert_ne!(VariantPayload::Unit, VariantPayload::Tuple(vec![]));
    assert_ne!(
        VariantPayload::Tuple(vec![Ty::Int]),
        VariantPayload::Tuple(vec![Ty::String])
    );
}

// ---- VariantCase ----

#[test]
fn variant_case_eq() {
    let c1 = VariantCase { name: "Red".into(), payload: VariantPayload::Unit };
    let c2 = VariantCase { name: "Red".into(), payload: VariantPayload::Unit };
    let c3 = VariantCase { name: "Blue".into(), payload: VariantPayload::Unit };
    assert_eq!(c1, c2);
    assert_ne!(c1, c3);
}

// ---- HKT Foundation: Type Constructor Infrastructure ----

use almide::types::{TypeConstructorId, TypeConstructorRegistry, Kind, AlgebraicLaw};

#[test]
fn constructor_id_primitives() {
    assert_eq!(Ty::Int.constructor_id(), Some(TypeConstructorId::Int));
    assert_eq!(Ty::String.constructor_id(), Some(TypeConstructorId::String));
    assert_eq!(Ty::Bool.constructor_id(), Some(TypeConstructorId::Bool));
    assert_eq!(Ty::Unit.constructor_id(), Some(TypeConstructorId::Unit));
    assert_eq!(Ty::Float.constructor_id(), Some(TypeConstructorId::Float));
}

#[test]
fn constructor_id_containers() {
    assert_eq!(Ty::List(Box::new(Ty::Int)).constructor_id(), Some(TypeConstructorId::List));
    assert_eq!(Ty::Option(Box::new(Ty::Int)).constructor_id(), Some(TypeConstructorId::Option));
    assert_eq!(
        Ty::Result(Box::new(Ty::Int), Box::new(Ty::String)).constructor_id(),
        Some(TypeConstructorId::Result)
    );
    assert_eq!(
        Ty::Map(Box::new(Ty::String), Box::new(Ty::Int)).constructor_id(),
        Some(TypeConstructorId::Map)
    );
}

#[test]
fn constructor_id_user_defined() {
    let ty = Ty::Named("Tree".into(), vec![Ty::Int]);
    assert_eq!(ty.constructor_id(), Some(TypeConstructorId::UserDefined("Tree".into())));
}

#[test]
fn constructor_id_none_for_special() {
    assert_eq!(Ty::Unknown.constructor_id(), None);
    assert_eq!(Ty::TypeVar("T".into()).constructor_id(), None);
}

#[test]
fn type_args_containers() {
    let list_int = Ty::List(Box::new(Ty::Int));
    assert_eq!(list_int.type_args(), vec![&Ty::Int]);

    let result_ty = Ty::Result(Box::new(Ty::Int), Box::new(Ty::String));
    assert_eq!(result_ty.type_args(), vec![&Ty::Int, &Ty::String]);
}

#[test]
fn type_args_empty_for_primitives() {
    assert!(Ty::Int.type_args().is_empty());
    assert!(Ty::Unknown.type_args().is_empty());
}

#[test]
fn children_leaf_types() {
    assert!(Ty::Int.children().is_empty());
    assert!(Ty::String.children().is_empty());
    assert!(Ty::Unknown.children().is_empty());
    assert!(Ty::TypeVar("T".into()).children().is_empty());
}

#[test]
fn children_containers() {
    let list = Ty::List(Box::new(Ty::Int));
    assert_eq!(list.children(), vec![&Ty::Int]);

    let result = Ty::Result(Box::new(Ty::Int), Box::new(Ty::String));
    assert_eq!(result.children(), vec![&Ty::Int, &Ty::String]);
}

#[test]
fn children_record() {
    let rec = Ty::Record { fields: vec![("x".into(), Ty::Int), ("y".into(), Ty::String)] };
    assert_eq!(rec.children(), vec![&Ty::Int, &Ty::String]);
}

#[test]
fn children_fn_type() {
    let f = Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Bool) };
    assert_eq!(f.children(), vec![&Ty::Int, &Ty::Bool]);
}

#[test]
fn map_children_identity() {
    let ty = Ty::List(Box::new(Ty::Int));
    let mapped = ty.map_children(&|t| t.clone());
    assert_eq!(mapped, ty);
}

#[test]
fn map_children_transform() {
    let ty = Ty::List(Box::new(Ty::TypeVar("T".into())));
    let mapped = ty.map_children(&|t| {
        if matches!(t, Ty::TypeVar(_)) { Ty::Int } else { t.clone() }
    });
    assert_eq!(mapped, Ty::List(Box::new(Ty::Int)));
}

#[test]
fn map_children_result() {
    let ty = Ty::Result(Box::new(Ty::TypeVar("A".into())), Box::new(Ty::TypeVar("B".into())));
    let mapped = ty.map_children(&|t| match t {
        Ty::TypeVar(n) if n == "A" => Ty::Int,
        Ty::TypeVar(n) if n == "B" => Ty::String,
        _ => t.clone(),
    });
    assert_eq!(mapped, Ty::Result(Box::new(Ty::Int), Box::new(Ty::String)));
}

#[test]
fn any_child_recursive_finds_nested() {
    let ty = Ty::List(Box::new(Ty::Option(Box::new(Ty::Unknown))));
    assert!(ty.any_child_recursive(&|t| matches!(t, Ty::Unknown)));
}

#[test]
fn any_child_recursive_not_found() {
    let ty = Ty::List(Box::new(Ty::Option(Box::new(Ty::Int))));
    assert!(!ty.any_child_recursive(&|t| matches!(t, Ty::Unknown)));
}

#[test]
fn is_container() {
    assert!(Ty::List(Box::new(Ty::Int)).is_container());
    assert!(Ty::Option(Box::new(Ty::Int)).is_container());
    assert!(Ty::Result(Box::new(Ty::Int), Box::new(Ty::String)).is_container());
    assert!(Ty::Map(Box::new(Ty::String), Box::new(Ty::Int)).is_container());
    assert!(!Ty::Int.is_container());
    assert!(!Ty::Named("Foo".into(), vec![]).is_container());
}

#[test]
fn constructor_name() {
    assert_eq!(Ty::List(Box::new(Ty::Int)).constructor_name(), Some("List"));
    assert_eq!(Ty::Int.constructor_name(), Some("Int"));
    assert_eq!(Ty::Named("Dog".into(), vec![]).constructor_name(), Some("Dog"));
    assert_eq!(Ty::Unknown.constructor_name(), None);
    assert_eq!(Ty::TypeVar("T".into()).constructor_name(), None);
}

// ---- Kind ----

#[test]
fn kind_arity() {
    assert_eq!(Kind::Star.arity(), 0);
    assert_eq!(Kind::star_to_star().arity(), 1);
    assert_eq!(Kind::star2_to_star().arity(), 2);
}

#[test]
fn kind_display() {
    assert_eq!(format!("{}", Kind::Star), "*");
    assert_eq!(format!("{}", Kind::star_to_star()), "* -> *");
    assert_eq!(format!("{}", Kind::star2_to_star()), "* -> * -> *");
}

// ---- TypeConstructorRegistry ----

#[test]
fn registry_builtins() {
    let reg = TypeConstructorRegistry::new();
    assert!(reg.lookup("List").is_some());
    assert!(reg.lookup("Option").is_some());
    assert!(reg.lookup("Result").is_some());
    assert!(reg.lookup("Map").is_some());
    assert!(reg.lookup("Int").is_some());
    assert!(reg.lookup("NonExistent").is_none());
}

#[test]
fn registry_kind() {
    let reg = TypeConstructorRegistry::new();
    assert_eq!(reg.kind_of("Int"), Some(&Kind::Star));
    assert_eq!(reg.kind_of("List"), Some(&Kind::star_to_star()));
    assert_eq!(reg.kind_of("Result"), Some(&Kind::star2_to_star()));
}

#[test]
fn registry_laws() {
    let reg = TypeConstructorRegistry::new();
    assert!(reg.satisfies("List", AlgebraicLaw::FunctorComposition));
    assert!(reg.satisfies("List", AlgebraicLaw::MapFoldFusion));
    assert!(reg.satisfies("Option", AlgebraicLaw::MonadAssociativity));
    assert!(!reg.satisfies("Map", AlgebraicLaw::FunctorComposition));
    assert!(!reg.satisfies("Int", AlgebraicLaw::FunctorComposition));
}

#[test]
fn registry_user_type() {
    let mut reg = TypeConstructorRegistry::new();
    reg.register_user_type("Tree", 1);
    let info = reg.lookup("Tree").unwrap();
    assert_eq!(info.kind, Kind::star_to_star());
    assert_eq!(info.id, TypeConstructorId::UserDefined("Tree".into()));
}

#[test]
fn registry_user_type_arity_2() {
    let mut reg = TypeConstructorRegistry::new();
    reg.register_user_type("Pair", 2);
    let info = reg.lookup("Pair").unwrap();
    assert_eq!(info.kind.arity(), 2);
}

// ---- Refactored functions still work ----

#[test]
fn contains_unknown_still_works() {
    assert!(Ty::Unknown.contains_unknown());
    assert!(Ty::List(Box::new(Ty::Unknown)).contains_unknown());
    assert!(Ty::Result(Box::new(Ty::Int), Box::new(Ty::Unknown)).contains_unknown());
    assert!(!Ty::Int.contains_unknown());
    assert!(!Ty::List(Box::new(Ty::Int)).contains_unknown());
}

#[test]
fn contains_unknown_nested() {
    let ty = Ty::Map(
        Box::new(Ty::String),
        Box::new(Ty::List(Box::new(Ty::Option(Box::new(Ty::Unknown))))),
    );
    assert!(ty.contains_unknown());
}
