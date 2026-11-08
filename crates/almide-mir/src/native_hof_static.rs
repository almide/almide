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
    while let Some(i) = out.find(RC) {
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
    found.then_some((twin, out))
}

// A borrowed runtime wrapper may forward its callback to another runtime
// function. Specialize that dependency too; otherwise an impl Fn reaches a
// parameter that still demands an Rc<dyn Fn + 'static>. Keep original items
// for other callers and derive every forwarded name from the runtime body.
fn static_runtime_closure(symbol: &str, mut items: Vec<&'static str>) -> Option<(String, Vec<&'static str>)> {
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
    fn forwarded_callbacks_specialize_the_dependency_without_changing_literals() {
        let root = "pub fn root(f: std::rc::Rc<dyn Fn(i64) -> i64>) -> i64 { helper(f) }";
        let helper = "pub fn helper(f: std::rc::Rc<dyn Fn(i64) -> i64>) -> i64 { let _ = \"helper(f)\"; f(1) }";
        let (name, texts) = static_runtime_closure("root", vec![root, helper]).unwrap();
        assert_eq!(name, "root_static");
        let text = texts.join("\n");
        assert!(text.contains("fn root_static(f: impl Fn(i64) -> i64) -> i64 { helper_static(f) }"));
        assert!(text.contains("fn helper_static(f: impl Fn(i64) -> i64)"));
        assert!(text.contains("\"helper(f)\""));
    }
}
