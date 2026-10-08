// result-family-from-type gate (the arc's Phase 3, first slice): the family of
// a materialized Result is a TOTAL function of the type (`result_family`), and
// the name set answers ONLY "is this call's result materialized". Each test
// pins one past incident of the name-keyed-family bug class so it cannot
// regress silently; the full registry-derivation of the name set is the arc's
// documented next step (docs/roadmap/active/result-family-from-type.md).

#[test]
fn result_family_is_a_total_function_of_the_type() {
    use crate::lower::{result_family, ResultFamily};
    let res = |ok: Ty, err: Ty| Ty::Applied(TypeConstructorId::Result, vec![ok, err]);
    // The one historical COLLISION type: Result[Unit, String] had two physical
    // layouts (ctor len-as-tag vs prim cap-as-tag) until Phase 1 unified the
    // ctor blocks onto the prim bytes. Its family is HeapOk (tag @16).
    assert_eq!(result_family(&res(Ty::Unit, Ty::String)), ResultFamily::HeapOk);
    // Heap-Ok classes — cap-as-tag: both arms own heap.
    assert_eq!(result_family(&res(Ty::String, Ty::String)), ResultFamily::HeapOk);
    assert_eq!(
        result_family(&res(Ty::Applied(TypeConstructorId::List, vec![Ty::Int]), Ty::String)),
        ResultFamily::HeapOk
    );
    assert_eq!(
        result_family(&res(Ty::Applied(TypeConstructorId::List, vec![Ty::String]), Ty::String)),
        ResultFamily::HeapOk
    );
    // Scalar classes — len-as-tag: scalar Ok payload (the int.parse /
    // float.parse / fs.file_size shapes), including the scalar-scalar class
    // (Result[Int, Int] — the err payload's high bits live at @16, which is
    // exactly why a cap-as-tag read must never be applied to this family).
    assert_eq!(result_family(&res(Ty::Int, Ty::String)), ResultFamily::Scalar);
    assert_eq!(result_family(&res(Ty::Float, Ty::String)), ResultFamily::Scalar);
    assert_eq!(result_family(&res(Ty::Bool, Ty::String)), ResultFamily::Scalar);
    assert_eq!(result_family(&res(Ty::Int, Ty::Int)), ResultFamily::Scalar);
}

#[test]
fn one_mapper_name_covers_all_nine_pairings_and_the_type_splits_them() {
    use crate::lower::{is_self_host_materialized_result_fn, result_family, ResultFamily};
    // #1406: the classify sites see the PRE-routing name `fan.any_map` for
    // every 3×3 pairing (`fan_any_call_name` suffixes at emit). ONE name row +
    // the type function must therefore cover the whole matrix — the suffixed
    // rows this bug class once grew (keyed on names no classify site ever
    // sees) were dead code by construction.
    assert!(is_self_host_materialized_result_fn("fan", "any_map"));
    let res = |ok: Ty| Ty::Applied(TypeConstructorId::Result, vec![ok, Ty::String]);
    // Scalar-output pairings (ii/si/if/fi/ff/sf) — len-as-tag.
    assert_eq!(result_family(&res(Ty::Int)), ResultFamily::Scalar);
    assert_eq!(result_family(&res(Ty::Float)), ResultFamily::Scalar);
    // String-output pairings (is/ss/fs) — the heap-Ok cap-as-tag family the
    // name tables mis-familied before the type split (the D7 wall).
    assert_eq!(result_family(&res(Ty::String)), ResultFamily::HeapOk);
}

#[test]
fn the_merged_name_set_survives_the_historical_name_mangling_incidents() {
    use crate::lower::is_self_host_materialized_result_fn;
    // C-145: a mono instance must resolve to its BASE name.
    assert!(is_self_host_materialized_result_fn("result", "__almd_mono7_or_else__Int_cString_cString"));
    // #1144: a carrier whose name BEGINS with `__` must not base to "".
    assert!(is_self_host_materialized_result_fn("fs", "__fallible_fold_lines"));
    // The Phase-1 moved family: ctor-built Result[Unit, String] producers are
    // materialized (their layout now byte-matches the prim family's).
    for f in ["copy", "append", "remove", "write_bytes", "write_bytes_raw", "write", "mkdir_p", "rename", "remove_all"] {
        assert!(is_self_host_materialized_result_fn("fs", f), "fs.{f} must be in the merged set");
    }
    // Non-members stay out (the set still gates "materialized at all").
    assert!(!is_self_host_materialized_result_fn("fan", "nonexistent"));
    // A host op is an ordinary call (#2739): its result is materialized.
    assert!(is_self_host_materialized_result_fn("http", "get"));
    // An http fn that is not an admitted host op stays out.
    assert!(!is_self_host_materialized_result_fn("http", "serve"));
}

#[test]
fn every_registry_result_fn_is_in_the_materialized_set() {
    // #3159: the merged name set had drifted from the registry — `fs.for_each_line`
    // and its fallible carrier build their Result through the ok()/err() ctors, yet a
    // `match`/`!` over them read as UNTRACKED and walled. The set may only differ from
    // the registry by a NAMED reason, so it cannot drift silently again: every
    // registry-served `module.fn` whose impl returns a Result is in the set — itself,
    // or as the mono-suffixed twin of a member (`fs.fold_lines_i` of `fs.fold_lines`,
    // the pre-routing name the classify sites see) — or listed below.
    use crate::lower::is_self_host_materialized_result_fn;
    use crate::lower::registry_sig::{registered_call_names, registry_signature};
    // http: every client entry point is an admitted host op (crate::host_ops,
    // #2739), an ordinary call whose result is materialized — none is listed.
    const NOT_YET: &[&str] = &[
        // process (#2589): these registry rows are the wasm legs' self-host
        // (stdlib/process_wasm.almd). The v1 lowering keeps treating a process
        // Result as it did before the rows existed (untracked); admit them with
        // the layout audit of their bodies, not before.
        "process.exec",
        "process.exec_attached",
        "process.exec_in",
        "process.exec_status",
        "process.exec_status_timeout",
        "process.exec_with_stdin",
        "process.kill",
        "process.run",
        "process.run_in",
        "process.spawn",
    ];
    let member = |module: &str, func: &str| {
        is_self_host_materialized_result_fn(module, func)
            || func.match_indices('_').any(|(i, _)| i > 0 && is_self_host_materialized_result_fn(module, &func[..i]))
    };
    let mut missing = Vec::new();
    for name in registered_call_names() {
        // A dot-less name is a Named-call helper, never a Module call subject.
        let Some((module, func)) = name.split_once('.') else { continue };
        let Some(sig) = registry_signature(name) else { continue };
        if !matches!(&sig.ret, Ty::Applied(TypeConstructorId::Result, _)) || NOT_YET.contains(&name) {
            continue;
        }
        if !member(module, func) {
            missing.push(name);
        }
    }
    assert!(missing.is_empty(), "registry Result fns missing from the materialized set: {missing:?}");
    for name in NOT_YET {
        let Some((module, func)) = name.split_once('.') else { continue };
        assert!(!member(module, func), "{name} is in the set now — drop it from NOT_YET");
    }
}
