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

/// Type names DECLARED by any bundled module (`type FileStat = …` in
/// fs.almd), bare spelling — these resolve through `register_bundled_types`
/// and need no registry row.
fn declared_bundled_type_names() -> BTreeSet<String> {
    let mut declared = BTreeSet::new();
    for module in almide_lang::stdlib_info::BUNDLED_MODULES {
        let Some(source) = almide_lang::stdlib_info::bundled_source(module) else { continue };
        let Some(program) = almide_lang::parse_cached(source) else { continue };
        for decl in &program.decls {
            if let almide_lang::ast::Decl::Type { name, .. } = decl {
                declared.insert(name.as_str().to_string());
            }
        }
    }
    declared
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
