// Included from native_floor.rs: the slicing twin of a string floor body.

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
