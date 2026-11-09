// Declaration types shared by native runtime callback adapters.
fn dty_of(t: &TypeExpr, tps: &BTreeSet<String>) -> Option<DTy> {
    Some(match t {
        TypeExpr::Simple { name } => simple_dty(name.as_str(), tps)?,
        TypeExpr::Tuple { elements } => DTy::Tuple(elements.iter().map(|e| dty_of(e, tps)).collect::<Option<_>>()?),
        TypeExpr::Generic { name, args } => generic_dty(name.as_str(), args, tps)?,
        TypeExpr::Fn { params, ret, is_effect } => {
            let ret = effect_return(dty_of(ret, tps)?, *is_effect);
            DTy::Fn(params.iter().map(|p| dty_of(p, tps)).collect::<Option<_>>()?, Box::new(ret))
        }
        _ => return None,
    })
}

/// [`dty_of`] for a bare type name: a primitive or a declared type parameter.
fn simple_dty(name: &str, tps: &BTreeSet<String>) -> Option<DTy> {
    Some(match name {
        "Unit" => DTy::Unit,
        "Int" => DTy::Int,
        "Bool" => DTy::Bool,
        "Float" => DTy::Float,
        "String" => DTy::Str,
        "Matrix" => DTy::Matrix,
        "Value" => DTy::Value,
        "Bytes" => DTy::Bytes,
        a if tps.contains(a) => DTy::Var(a.to_string()),
        a => DTy::Runtime(crate::native_runtime_types::RuntimeType::of(a)?),
    })
}

/// [`dty_of`] for an applied generic: the one- and two-argument containers.
fn generic_dty(name: &str, args: &[TypeExpr], tps: &BTreeSet<String>) -> Option<DTy> {
    Some(match (name, args.len()) {
        ("List", 1) => DTy::List(Box::new(dty_of(&args[0], tps)?)),
        ("Set", 1) => DTy::Set(Box::new(dty_of(&args[0], tps)?)),
        ("?" | "Option", 1) => DTy::Opt(Box::new(dty_of(&args[0], tps)?)),
        ("Map", 2) => DTy::Map(Box::new(dty_of(&args[0], tps)?), Box::new(dty_of(&args[1], tps)?)),
        ("Result", 2) => DTy::Result(Box::new(dty_of(&args[0], tps)?), Box::new(dty_of(&args[1], tps)?)),
        _ => return None,
    })
}


fn effect_return(ret: DTy, effect: bool) -> DTy {
    if effect && !matches!(ret, DTy::Result(..)) {
        DTy::Result(Box::new(ret), Box::new(DTy::Str))
    } else { ret }
}

// Resolve non-generic aliases from this declaration's module, including
// effect callback aliases. Cycles and generic aliases are not guessed.
fn declared_dty(t: &TypeExpr, tps: &BTreeSet<String>, decls: &[Decl]) -> Option<DTy> {
    fn expand(t: &TypeExpr, tps: &BTreeSet<String>, decls: &[Decl], seen: &mut BTreeSet<String>) -> Option<TypeExpr> {
        Some(match t {
            TypeExpr::Simple { name } if simple_dty(name.as_str(), tps).is_none() => {
                let mut aliases = decls.iter().filter_map(|d| match d {
                    Decl::Type { name: n, ty, generics, .. } if n == name && generics.as_ref().is_none_or(|g| g.is_empty()) => Some(ty),
                    _ => None,
                });
                let alias = aliases.next()?;
                if aliases.next().is_some() || !seen.insert(name.to_string()) { return None; }
                let ty = expand(alias, tps, decls, seen)?;
                seen.remove(name.as_str());
                ty
            }
            TypeExpr::Fn { params, ret, is_effect } => TypeExpr::Fn {
                params: params.iter().map(|p| expand(p, tps, decls, seen)).collect::<Option<_>>()?,
                ret: Box::new(expand(ret, tps, decls, seen)?), is_effect: *is_effect,
            },
            TypeExpr::Tuple { elements } => TypeExpr::Tuple { elements: elements.iter().map(|e| expand(e, tps, decls, seen)).collect::<Option<_>>()? },
            TypeExpr::Generic { name, args } => TypeExpr::Generic { name: *name, args: args.iter().map(|a| expand(a, tps, decls, seen)).collect::<Option<_>>()? },
            _ => t.clone(),
        })
    }
    dty_of(&expand(t, tps, decls, &mut BTreeSet::new())?, tps)
}

#[cfg(test)]
mod declared_callback_tests {
    use super::*;
    #[test]
    fn module_aliases_keep_effect_carriers_and_runtime_identity() {
        let program = almide_lang::parse_cached(almide_lang::stdlib_info::bundled_source("http").unwrap()).unwrap();
        let ty = TypeExpr::Simple { name: almide_lang::intern::sym("HttpHandler") };
        let request = crate::native_runtime_types::RuntimeType::of("HttpRequest").unwrap();
        let response = crate::native_runtime_types::RuntimeType::of("HttpResponse").unwrap();
        let want = DTy::Fn(vec![DTy::Runtime(request)], Box::new(DTy::Result(Box::new(DTy::Runtime(response)), Box::new(DTy::Str))));
        assert_eq!(declared_dty(&ty, &BTreeSet::new(), &program.decls), Some(want));
        assert_eq!(rust_agrees(&DTy::Runtime(request), "&AlmideHttpRequest"), Some(true));
        assert_eq!(rust_agrees(&DTy::Runtime(request), "&AlmideHttpResponse"), None);
        assert_eq!(rust_agrees(&DTy::Runtime(request), "&mutAlmideHttpRequest"), None);
        let ret = DTy::Result(Box::new(DTy::Int), Box::new(DTy::Str));
        assert_eq!(effect_return(ret.clone(), true), ret);
        let cyclic = almide_lang::parse_cached("type A = B\ntype B = A\n").unwrap();
        assert_eq!(declared_dty(&TypeExpr::Simple { name: almide_lang::intern::sym("A") }, &BTreeSet::new(), &cyclic.decls), None);
    }
    #[test]
    fn listener_runtime_has_the_declared_unit_abi() {
        let hof = derive_explained("http.serve").unwrap();
        assert_eq!(hof.ret, DTy::Unit);
        assert!(hof.items.iter().any(|s| s.contains("mod almide_http_signals")));
        assert!(declared_call("http.serve"));
    }
    #[test]
    fn listener_keeps_network_and_callback_effects_in_the_capability_graph() {
        let main = MirFunction { name: "main".into(), ops: vec![
            Op::FuncRef { dst: ValueId(0), name: "on_request".into() },
            Op::CallFn { name: "http.serve".into(), dst: None, args: vec![], result: None },
        ], ..Default::default() };
        let handler = MirFunction { name: "on_request".into(), ops: vec![
            Op::CallFn { name: "fs.write_text".into(), dst: None, args: vec![], result: None },
        ], ..Default::default() };
        reset(Default::default(), Default::default());
        STATE.with(|s| s.borrow_mut().sites.insert(("main".into(), 1), "almide_rt_http_serve".into()));
        let graph = capability_program(vec![main, handler]);
        let caps: BTreeSet<_> = crate::certificate::reachable_caps("main", &graph, &mut BTreeSet::new()).into_iter().collect();
        assert_eq!(caps, [crate::Capability::Net, crate::Capability::FsWrite].into_iter().collect());
        reset(Default::default(), Default::default());
    }

}
