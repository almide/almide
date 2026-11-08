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
fn runtime_result(env: &Env, d: &DTy, code: &str) -> Option<String> {
    if !has_runtime_tuple(d) {
        return Some(if *d == DTy::Bool { format!("({code}) as i64") } else { code.to_string() });
    }
    Some(match d {
        DTy::Tuple(es) => {
            let NTy::Rec(i) = kind_of(env, d)? else { return None };
            let names: Vec<String> = (0..es.len()).map(|i| format!("__tuple_{i}")).collect();
            let fields = es.iter().zip(&names).enumerate().map(|(k, (e, name))| {
                Some(format!("f{k}: {}", runtime_result(env, e, name)?))
            }).collect::<Option<Vec<_>>>()?;
            let names = if names.is_empty() { String::new() } else { format!("{},", names.join(", ")) };
            format!("{{ let ({names}) = {code}; {} {{ {} }} }}", rec::struct_name(i), fields.join(", "))
        }
        DTy::List(e) => {
            let body = runtime_result(env, e, "__item")?;
            format!("({code}).into_iter().map(|__item| {body}).collect::<Vec<{}>>()", rec::owned_ty(kind_of(env, e)?))
        }
        DTy::Opt(e) => format!("({code}).map(|__some| {})", runtime_result(env, e, "__some")?),
        DTy::Result(ok, err) => format!("({code}).map(|__ok| {}).map_err(|__err| {})",
            runtime_result(env, ok, "__ok")?, runtime_result(env, err, "__err")?),
        // Collections whose ordering/hash/equality would cross a tuple ABI
        // need a separate adapter. Refuse those shapes instead of guessing.
        _ => return None,
    })
}

#[cfg(test)]
mod tuple_result_tests {
    use super::*;

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
        let expr = runtime_result(&env, &d, "runtime_call()").unwrap();
        assert_eq!(expr.matches("runtime_call()").count(), 1);
        assert!(expr.contains(&rec::struct_name(1)));
        assert!(!expr.contains(&format!("{} {{", rec::struct_name(0))));
        let unsupported = DTy::Set(Box::new(DTy::Tuple(vec![DTy::Int])));
        assert!(runtime_result(&env, &unsupported, "runtime_call()").is_none());
    }
}
