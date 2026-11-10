// Runtime tuple results use Rust tuples; native MIR slots use generated structs.
// Derive this conversion from the declaration and its bound type parameters.

fn has_runtime_tuple(d: &DTy) -> bool {
    match d {
        DTy::Tuple(_) => true,
        DTy::List(e) | DTy::Opt(e) | DTy::Set(e) => has_runtime_tuple(e),
        DTy::Result(a, b) | DTy::Map(a, b) => has_runtime_tuple(a) || has_runtime_tuple(b),
        DTy::Fn(ps, r) => ps.iter().any(has_runtime_tuple) || has_runtime_tuple(r),
        _ => false,
    }
}

/// Evaluate the runtime expression exactly once, transferring each payload
/// into its declared native field. No layout casts or positional byte reads.
fn runtime_result_as(env: &Env, d: &DTy, code: &str, native: NTy) -> Option<String> {
    // A concrete call already has a checked tuple/container identity. A
    // structural runtime tuple must rebuild that exact type, so a subsequent
    // callback sees the same type it was checked against.
    convert_callback_value(env, d, native, code, true)
}

/// Explicit runtime payloads differ from MIR lanes even without a tuple.
/// Generic type variables remain bound to their native value representation.
fn has_value_conversion(d: &DTy) -> bool {
    match d {
        DTy::Tuple(_) | DTy::Float | DTy::Bool | DTy::Unit => true,
        DTy::List(e) | DTy::Opt(e) => has_value_conversion(e),
        DTy::Result(a, b) => has_value_conversion(a) || has_value_conversion(b),
        _ => has_runtime_tuple(d),
    }
}

#[cfg(test)]
mod tuple_result_tests {
    use super::*;

    fn runtime_result(env: &Env, d: &DTy, code: &str) -> Option<String> {
        runtime_result_as(env, d, code, kind_of(env, d)?)
    }

    #[test]
    fn tuple_returning_declarations_can_route_native_representation_refusals() {
        for call in ["list.enumerate", "list.zip", "list.partition"] {
            assert!(declared_call(call), "{call}");
        }
        assert!(!declared_call("user.enumerate"));
    }

    #[test]
    fn tuple_results_use_structural_identity_and_keep_runtime_calls_single() {
        let _guard = rec::install(vec![
            rec::RecDecl { name: "NamedRecord".into(), fields: vec![NTy::I64, NTy::Str], ..Default::default() },
            rec::RecDecl { name: "Tuple([Int, String])".into(), fields: vec![NTy::I64, NTy::Str], ..Default::default() },
        ]);
        let d = DTy::List(Box::new(DTy::Tuple(vec![DTy::Int, DTy::Str])));
        let env = Env::new();
        assert_eq!(kind_of(&env, &DTy::Tuple(vec![DTy::Int, DTy::Str])), Some(NTy::Rec(1)));
        let expr = runtime_result(&env, &d, "runtime_call()").expect("a list of int-string tuples has a runtime result");
        assert_eq!(expr.matches("runtime_call()").count(), 1);
        assert!(expr.contains(&rec::struct_name(1)));
        assert!(!expr.contains(&format!("{} {{", rec::struct_name(0))));
        let unsupported = DTy::Set(Box::new(DTy::Tuple(vec![DTy::Int])));
        assert!(runtime_result(&env, &unsupported, "runtime_call()").is_none());
    }

    #[test]
    fn generic_runtime_float_tuple_fields_preserve_bits() {
        let _guard = rec::install(vec![rec::RecDecl {
            name: "Tuple([String, Float])".into(), fields: vec![NTy::Str, NTy::I64],
            ..Default::default()
        }]);
        let env = Env::from([("V".into(), NTy::F64)]);
        let pair = DTy::Tuple(vec![DTy::Str, DTy::Var("V".into())]);
        assert_eq!(kind_of(&env, &pair), Some(NTy::Rec(0)));
        let code = runtime_result(&env, &pair, "read_map()").expect("a (String, V) pair has a runtime result");
        assert!(code.contains("to_bits() as i64"));
        assert_eq!(code.matches("read_map()").count(), 1);
    }
}

/// A representation-specialized body can disagree with a concrete source
/// result. Leave its call sites borrowing until the runtime adapter resolves
/// that representation; moving first would forbid the adapter and its proof.
pub(crate) fn representation_adapters_needed(functions: &[MirFunction], sigs: &NativeSigs) -> BTreeSet<String> {
    let mut pinned = BTreeSet::new();
    for caller in functions {
        for op in &caller.ops {
            let Op::CallFn { name, dst: Some(dst), .. } = op else { continue };
            if of(name).is_none() { continue; }
            let Some(want) = caller.aggregate_blocks.get(dst).and_then(|k| rec::declared_kind(k)) else { continue };
            let Some((_, Some(got))) = sigs.get(name.as_str()) else { continue };
            if !crate::native_value_repr::same_owned(want, clo::owned_nty(*got)) {
                pinned.insert(name.clone());
            }
        }
    }
    pinned
}
