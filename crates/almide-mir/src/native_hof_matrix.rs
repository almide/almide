// The native renderer owns Matrix values directly; the ordinary runtime
// also accepts Rc<Matrix> through a declared Deref bound. Borrow each real
// Matrix for that signature, without copying its data or changing its shape.
fn matrix_ref_list(d: &DTy, rust: &str, symbol: &str, items: &[&str]) -> Option<bool> {
    if !matches!(d, DTy::List(e) if **e == DTy::Matrix) { return Some(false); }
    if matches!(rust, "&[AlmideMatrix]" | "&Vec<AlmideMatrix>" | "Vec<AlmideMatrix>") {
        return Some(false);
    }
    let param = rust.strip_prefix("&[")?.strip_suffix(']')?;
    if param.is_empty() || !param.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') { return None; }
    let prefix = format!("fn {symbol}<");
    let header = items.iter().find_map(|item| {
        let start = item.find(&prefix)?;
        Some(item[start..].split('{').next()?)
    })?;
    let compact: String = header.chars().filter(|c| !c.is_whitespace()).collect();
    let bound = format!("{param}:std::ops::Deref<Target=AlmideMatrix>");
    (compact.contains(&format!("<{bound}>")) || compact.contains(&format!("<{bound},")))
        .then_some(true)
}

#[cfg(test)]
mod matrix_arg_tests {
    use super::*;
    #[test]
    fn matrix_list_adapter_requires_the_runtime_deref_contract() {
        let d = DTy::List(Box::new(DTy::Matrix));
        let source = "pub fn join<M: std::ops::Deref<Target = AlmideMatrix>>(xs: &[M]) -> AlmideMatrix { todo!() }";
        assert_eq!(matrix_ref_list(&d, "&[M]", "join", &[source]), Some(true));
        assert_eq!(matrix_ref_list(&d, "&[AlmideMatrix]", "join", &[source]), Some(false));
        assert_eq!(matrix_ref_list(&d, "&[M]", "join", &["fn join<M>(xs: &[M]) {}"]), None);
        assert_eq!(matrix_ref_list(&d, "&[M]", "join", &["fn join<M: std::ops::Deref<Target = i64>>(xs: &[M]) {}"]), None);
    }
}
