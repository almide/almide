// Included from native_floor.rs: runtime twins paired from the signatures —
// the slicing twin of a string body, the in-place twin of a consuming one.

/// The slicing twin of the floor body `symbol`: the runtime fn `<symbol>_ref`,
/// paired from the signatures. Both take the same number of `&str` params;
/// the body returns `Option<AlmideString>` or a pair of them, and the twin the
/// same shape in `&str` borrowed from its lifetime-carrying params. Returns
/// the twin, whether it yields a pair, which params its result borrows from,
/// and its items. `None` when the runtime carries no such fn.
pub(crate) fn slice_twin(symbol: &str) -> Option<(String, bool, Vec<bool>, Vec<&'static str>)> {
    let rt = runtime_index();
    let plain = |t: &String| squash(&without_lifetimes(t));
    let (body_params, body_ret) = rust_signature(signature_text(rt, symbol)?)?;
    let twin = format!("{symbol}_ref");
    let (twin_params, twin_ret) = rust_signature(signature_text(rt, &twin)?)?;
    let pair = match (plain(&body_ret).as_str(), plain(&twin_ret).as_str()) {
        ("Option<AlmideString>", "Option<&str>") => false,
        ("Option<(AlmideString,AlmideString)>", "Option<(&str,&str)>") => true,
        _ => return None,
    };
    let strs = |ps: &[String]| ps.iter().all(|p| plain(p) == "&str");
    let from: Vec<bool> = twin_params.iter().map(|p| p.contains('\'')).collect();
    let paired = body_params.len() == twin_params.len() && strs(&body_params) && strs(&twin_params) && from.contains(&true);
    paired.then(|| Some((twin.clone(), pair, from, closure(&twin, rt).ok()?)))?
}

/// The in-place twin of the consuming runtime body `symbol`: the runtime fn
/// `<symbol>_mut`, paired from the signatures. The body takes its first param
/// by value and returns that type; the twin takes it `&mut`, every other
/// param as the body does (a boxed callback as the same `impl Fn`), and
/// returns nothing. Returns the twin and its items. `None` when the runtime
/// carries no such fn.
pub(crate) fn in_place_twin(symbol: &str) -> Option<(String, Vec<&'static str>)> {
    let rt = runtime_index();
    let (body_params, body_ret) = rust_signature(signature_text(rt, symbol)?)?;
    let twin = format!("{symbol}_mut");
    let (twin_params, twin_ret) = rust_signature(signature_text(rt, &twin)?)?;
    // A callback: boxed in the body, static in the twin, the same `Fn`.
    let plain = |t: &String| {
        let t = squash(t);
        match t.strip_prefix("std::rc::Rc<dyn").and_then(|f| f.strip_suffix('>')) {
            Some(f) => f.to_string(),
            None => t.strip_prefix("impl").map(str::to_string).unwrap_or(t),
        }
    };
    let (taken, rest) = body_params.split_first()?;
    let (edited, twin_rest) = twin_params.split_first()?;
    let paired = squash(edited) == format!("&mut{}", squash(taken))
        && squash(&body_ret) == squash(taken)
        && twin_ret.is_empty()
        && rest.len() == twin_rest.len()
        && rest.iter().zip(twin_rest).all(|(a, b)| plain(a) == plain(b));
    paired.then(|| Some((twin.clone(), closure(&twin, rt).ok()?)))?
}
