/// The runtime body `text` of `symbol` with every `std::rc::Rc<dyn Fn(..) ->
/// ..>` param taken as `impl Fn(..) -> ..` instead, under the name
/// `<symbol>_static`: the same body, monomorphized over the caller's closure
/// (no allocation for the closure, a direct call LLVM can inline), the way
/// the standard codegen inlines these combinators. `None` when the body has
/// no such param or does not spell its own name.
fn static_twin(symbol: &str, text: &str) -> Option<(String, String)> {
    const RC: &str = "std::rc::Rc<dyn ";
    let head = format!("fn {symbol}");
    let at = text.find(&head)?;
    let twin = format!("{symbol}_static");
    let mut out = format!("{}fn {twin}{}", &text[..at], &text[at + head.len()..]);
    let mut found = false;
    let mut dereferenced_params = Vec::new();
    while let Some(i) = out.find(RC) {
        if out.find('{').is_some_and(|body| i < body) {
            if let Some((prefix, _)) = out[..i].rsplit_once(':') {
                let name = prefix.trim_end().rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_').next().unwrap_or("");
                if !name.is_empty() && out.contains(&format!("&*{name}")) { dereferenced_params.push(name.to_string()); }
            }
        }
        // The `>` that closes the `Rc<`.
        let mut depth = 1i32;
        let mut close = None;
        for (k, c) in out[i + RC.len()..].char_indices() {
            match c {
                '<' | '(' => depth += 1,
                ')' => depth -= 1,
                '>' if out[i + RC.len()..][..k].ends_with('-') => {}
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(i + RC.len() + k);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        out = format!("{}impl {}{}", &out[..i], &out[i + RC.len()..close], &out[close + 1..]);
        found = true;
    }
    // A runtime body may pass `&*callback` to a borrowed worker. Preserve
    // that spelling by lending the static callback within its own frame.
    if let Some(body) = out.find('{') {
        let bindings = dereferenced_params.iter().map(|n| format!(" let {n} = &{n};")).collect::<String>();
        out.insert_str(body + 1, &bindings);
    }
    found.then_some((twin, out))
}

// A borrowed runtime wrapper may forward its callback to another runtime
// function. Specialize that dependency too; otherwise an impl Fn reaches a
// parameter that still demands an Rc<dyn Fn + 'static>. Keep original items
// for other callers and derive every forwarded name from the runtime body.
fn static_runtime_closure(symbol: &str, mut items: Vec<&'static str>) -> Option<(String, Vec<&'static str>)> {
    if let Some(body) = crate::native_floor::native_host_callback_body(symbol) {
        let (twin, root) = static_twin(symbol, body)?;
        items.push(Box::leak(root.into_boxed_str()));
        return Some((twin, items));
    }
    let (twin, root) = static_twin(symbol, items.first()?)?;
    let candidates: BTreeMap<String, (String, String)> = items.iter().skip(1).filter_map(|text| {
        let name = crate::native_floor::runtime_fn_name(text)?;
        Some((name.clone(), static_twin(&name, text)?))
    }).collect();
    let mut selected = BTreeMap::new();
    let mut todo = vec![root.clone()];
    while let Some(body) = todo.pop() {
        let names = crate::native_floor::runtime_identifiers(&body);
        for (name, value) in &candidates {
            if names.contains(name) && !selected.contains_key(name) {
                selected.insert(name.clone(), value.clone());
                todo.push(value.1.clone());
            }
        }
    }
    if selected.is_empty() {
        items[0] = Box::leak(root.into_boxed_str());
        return Some((twin, items));
    }
    let mut rename: BTreeMap<String, String> = selected.iter().map(|(n, (t, _))| (n.clone(), t.clone())).collect();
    rename.insert(symbol.to_string(), twin.clone());
    // Preserve the entire original dependency graph for its original callers.
    // The new root and only its reachable callback-bearing helpers get twins.
    items.push(Box::leak(crate::native_floor::rewrite_runtime_identifiers(&root, &rename).into_boxed_str()));
    for (_, body) in selected.into_values() {
        items.push(Box::leak(crate::native_floor::rewrite_runtime_identifiers(&body, &rename).into_boxed_str()));
    }
    Some((twin, items))
}

#[cfg(test)]
mod static_callback_tests {
    use super::*;
    #[test]
    fn static_callback_can_be_lent_to_a_runtime_worker() {
        let source = "fn route(callback: std::rc::Rc<dyn Fn(i64) -> i64>) -> i64 { worker(&*callback) }";
        let (_, body) = static_twin("route", source).expect("route has a static twin");
        assert!(body.contains("callback: impl Fn(i64) -> i64"));
        assert!(body.contains("let callback = &callback;"));
        assert!(body.contains("worker(&*callback)"));
    }

    #[test]
    fn forwarded_callbacks_specialize_the_dependency_without_changing_literals() {
        let root = "pub fn root(f: std::rc::Rc<dyn Fn(i64) -> i64>) -> i64 { helper(f) }";
        let helper = "pub fn helper(f: std::rc::Rc<dyn Fn(i64) -> i64>) -> i64 { let _ = \"helper(f)\"; f(1) }";
        let (name, texts) = static_runtime_closure("root", vec![root, helper]).expect("the forwarded callback specializes");
        assert_eq!(name, "root_static");
        let text = texts.join("\n");
        assert!(text.contains("fn root_static(f: impl Fn(i64) -> i64) -> i64 { helper_static(f) }"));
        assert!(text.contains("fn helper_static(f: impl Fn(i64) -> i64)"));
        assert!(text.contains("\"helper(f)\""));
    }
}

#[cfg(test)]
mod native_host_callback_tests {
    use super::*;

    #[test]
    fn host_callbacks_keep_typed_arguments_and_void_return_abi() {
        for name in ["fs_fold_lines_i", "fs_fold_lines_s", "fs_fallible_fold_lines_i",
            "fs_fallible_fold_lines_msi", "fs_for_each_line", "fs_fallible_for_each_line"] {
            let hof = derive_explained(name).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(crate::host_ops::is_native_fs_host_call(&hof.symbol));
            assert!(hof.items.iter().any(|t| t.contains(&format!("fn {}", hof.twin))));
        }
        assert!(derive_explained("io.read_line").is_err(), "effect calls outside this boundary stay refused");
        let env = Env::new();
        for lambda in [Some("visit"), None] {
            let code = tuple_callback_adapter(&env, lambda, "captured", (&[DTy::Str], &DTy::Unit),
                (&[NativeSigKind::Str], &[]), None).expect("the tuple callback adapts");
            assert!(code.contains("-> ()"), "{code}");
            assert!(!code.contains("0_i64"), "void return must not fabricate a value: {code}");
        }
        assert!(tuple_callback_adapter(&env, Some("visit"), "captured", (&[DTy::Str], &DTy::Int),
            (&[NativeSigKind::Str], &[]), None).is_none());
    }

    #[test]
    fn selected_host_sites_keep_their_own_and_callback_capabilities() {
        let mut main = MirFunction { name: "main".into(), ops: vec![
            Op::FuncRef { dst: ValueId(0), name: "visit".into() },
            Op::CallFn { name: "linked_walker".into(), dst: None, args: vec![], result: None },
        ], ..Default::default() };
        let callback = MirFunction { name: "visit".into(), ops: vec![
            Op::CallFn { name: "fs.write_text".into(), dst: None, args: vec![], result: None },
        ], ..Default::default() };
        reset(Default::default(), Default::default());
        STATE.with(|s| s.borrow_mut().sites.insert(("main".into(), 1), "almide_rt_fs_for_each_line".into()));
        let sites = sites();
        let witness = crate::certificate::native_calls_witness_with(&[main.clone()], &sites);
        let id = crate::host_ops::native_fs_host_id("almide_rt_fs_for_each_line").expect("for_each_line is a native fs host op");
        assert_eq!(witness.lines().next().unwrap().split('|').nth(3), Some(id.to_string().as_str()));
        let graph = capability_program(vec![main.clone(), callback.clone()]);
        let caps: BTreeSet<_> = crate::certificate::reachable_caps("main", &graph, &mut BTreeSet::new()).into_iter().collect();
        assert_eq!(caps, [crate::Capability::FsRead, crate::Capability::FsWrite].into_iter().collect());
        // The same spelling at a different op is no selected host call.
        main.ops.remove(0);
        let graph = capability_program(vec![main, callback]);
        assert!(crate::certificate::reachable_caps("main", &graph, &mut BTreeSet::new()).is_empty());
        reset(Default::default(), Default::default());
    }
}
