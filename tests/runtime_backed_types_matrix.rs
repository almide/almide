//! The RUNTIME_BACKED_TYPES matrix gate (#1053).
//!
//! A stdlib signature may name a nominal type with no `type` declaration
//! anywhere — a runtime-backed struct (`HttpRequest`). Every such name must
//! be in `stdlib_info::RUNTIME_BACKED_TYPES`, or the docs advertise a type a
//! user annotation cannot spell (E029 with no way out). And every registry
//! row must still be REACHABLE from some bundled signature — a dead row
//! would exempt a name from E029 for no reason. Point-wise drift in either
//! direction fails here, not in the field.

use std::collections::BTreeSet;

use almide_lang::types::Ty;

fn collect_named(ty: &Ty, out: &mut BTreeSet<String>) {
    match ty {
        Ty::Named(s, args) => {
            out.insert(s.as_str().to_string());
            for a in args {
                collect_named(a, out);
            }
        }
        Ty::Applied(_, args) | Ty::Tuple(args) | Ty::Union(args) => {
            for a in args {
                collect_named(a, out);
            }
        }
        Ty::Fn { is_effect: _, params, ret } => {
            for p in params {
                collect_named(p, out);
            }
            collect_named(ret, out);
        }
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            for (_, f) in fields {
                collect_named(f, out);
            }
        }
        _ => {}
    }
}

/// `(module, type name)` for every `type` DECLARED by a bundled module
/// (`type FileStat = …` in fs.almd).
fn declared_bundled_types() -> BTreeSet<(String, String)> {
    let mut declared = BTreeSet::new();
    for module in almide_lang::stdlib_info::BUNDLED_MODULES {
        let Some(source) = almide_lang::stdlib_info::bundled_source(module) else { continue };
        let Some(program) = almide_lang::parse_cached(source) else { continue };
        for decl in &program.decls {
            if let almide_lang::ast::Decl::Type { name, .. } = decl {
                declared.insert((module.to_string(), name.as_str().to_string()));
            }
        }
    }
    declared
}

/// Type names DECLARED by any bundled module, bare spelling — these resolve
/// through `register_bundled_types` and need no runtime-backed row.
fn declared_bundled_type_names() -> BTreeSet<String> {
    declared_bundled_types().into_iter().map(|(_, name)| name).collect()
}

/// The STDLIB_OWNED_TYPES matrix gate (#1828). A user declaration of one of
/// these names takes the #433 module-qualified identity instead of the bare
/// key the stdlib's type is; the registry must therefore be EXACTLY the
/// builtin `Value`, every runtime-backed nominal, and every `type` a bundled
/// module declares. A missing row lets a user declaration rebind a stdlib
/// signature's type (the #1828 miscompile); a dead row qualifies a user type
/// for no reason.
#[test]
fn stdlib_owned_types_are_value_plus_runtime_backed_plus_bundled_decls() {
    let mut expected: BTreeSet<(String, String)> = declared_bundled_types();
    expected.insert(("value".to_string(), "Value".to_string()));
    for (m, t) in almide_lang::stdlib_info::RUNTIME_BACKED_TYPES {
        expected.insert((m.to_string(), t.to_string()));
    }
    let registry: BTreeSet<(String, String)> = almide_lang::stdlib_info::STDLIB_OWNED_TYPES
        .iter()
        .map(|(m, t)| (m.to_string(), t.to_string()))
        .collect();
    let missing: Vec<_> = expected.difference(&registry).collect();
    assert!(
        missing.is_empty(),
        "stdlib-owned type(s) absent from STDLIB_OWNED_TYPES — a user declaration \
         of the name would rebind the stdlib's signatures (#1828); add the row(s): {missing:?}"
    );
    let dead: Vec<_> = registry.difference(&expected).collect();
    assert!(
        dead.is_empty(),
        "STDLIB_OWNED_TYPES row(s) no stdlib module owns — remove them so a user \
         type of that name keeps its bare identity: {dead:?}"
    );
    for (_, t) in almide_lang::stdlib_info::STDLIB_OWNED_TYPES {
        assert!(
            almide_lang::stdlib_info::stdlib_owned_type_owner(t).is_some(),
            "stdlib_owned_type_owner must answer every registry row: {t}"
        );
    }
}

#[test]
fn registry_matches_the_undeclared_bundled_sig_nominals() {
    let declared = declared_bundled_type_names();
    let mut undeclared: BTreeSet<(String, String)> = BTreeSet::new();
    for module in almide_lang::stdlib_info::BUNDLED_MODULES {
        for fname in almide_frontend::bundled_sigs::module_fn_names(module) {
            let Some(sig) = almide_frontend::bundled_sigs::lookup(module, fname) else { continue };
            let mut names = BTreeSet::new();
            for (_, pty) in &sig.params {
                collect_named(pty, &mut names);
            }
            collect_named(&sig.ret, &mut names);
            for name in names {
                let bare = name.rsplit('.').next().unwrap_or(&name).to_string();
                // `Value` is the built-in dynamic type; a sig generic is a
                // type variable of that one signature, not a nominal.
                if bare == "Value"
                    || sig.generics.iter().any(|g| g.as_str() == bare)
                    || declared.contains(&bare)
                {
                    continue;
                }
                undeclared.insert((module.to_string(), bare));
            }
        }
    }
    let registry: BTreeSet<(String, String)> = almide_lang::stdlib_info::RUNTIME_BACKED_TYPES
        .iter()
        .map(|(m, t)| (m.to_string(), t.to_string()))
        .collect();
    let missing: Vec<_> = undeclared.difference(&registry).collect();
    assert!(
        missing.is_empty(),
        "bundled signatures name undeclared nominal type(s) absent from \
         RUNTIME_BACKED_TYPES — user annotations naming them are an E029 dead \
         end; add the row(s): {missing:?}"
    );
    let dead: Vec<_> = registry.difference(&undeclared).collect();
    assert!(
        dead.is_empty(),
        "RUNTIME_BACKED_TYPES row(s) no longer reachable from any bundled \
         signature — remove them so the E029 exemption stays justified: {dead:?}"
    );
}
