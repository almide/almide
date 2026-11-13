// Included from native_floor.rs: runtime twins paired from the signatures —
// the slicing twin of a string body, the in-place twin of a consuming one,
// the refilling twin of a building one.

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

/// A paired twin: its name and its items.
type Twin = Option<(String, Vec<&'static str>)>;

/// The in-place twin of the consuming runtime body `symbol`: the runtime fn
/// `<symbol>_mut`, paired from the signatures. The body takes its first param
/// by value and returns that type; the twin takes it `&mut`, every other
/// param as the body does (a boxed callback as the same `impl Fn`), and
/// returns nothing. Returns the twin and its items. `None` when the runtime
/// carries no such fn.
pub(crate) fn in_place_twin(symbol: &str) -> Twin {
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

/// The shared-key twin of the in-place twin `twin`: the runtime fn
/// `<twin>_by_ref`, paired from the signatures — the same params, the second
/// (the key) by reference, nothing returned. It copies the key only where it
/// stores it. Returns the twin and its items. `None` when the runtime
/// carries no such fn.
pub(crate) fn shared_key_twin(twin: &str) -> Twin {
    let rt = runtime_index();
    let (params, ret) = rust_signature(signature_text(rt, twin)?)?;
    let keyed = format!("{twin}_by_ref");
    let (keyed_params, keyed_ret) = rust_signature(signature_text(rt, &keyed)?)?;
    let same = |(at, (a, b)): (usize, (&String, &String))| if at == 1 { squash(b) == format!("&{}", squash(a)) } else { squash(a) == squash(b) };
    let paired = ret.is_empty() && keyed_ret.is_empty() && params.len() >= 2 && params.len() == keyed_params.len()
        && params.iter().zip(&keyed_params).enumerate().all(same);
    paired.then(|| Some((keyed.clone(), closure(&keyed, rt).ok()?)))?
}

/// The refilling twin of the building runtime body `symbol`: the runtime fn
/// `<symbol>_into`, paired from the signatures — a `&mut` of the body's
/// result type, then the body's own params, and nothing returned. It leaves
/// in the place it is handed what the body returns. Returns the twin and its
/// items. `None` when the runtime carries no such fn.
pub(crate) fn refill_twin(symbol: &str) -> Twin {
    static CACHE: OnceLock<Mutex<HashMap<String, Twin>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(symbol) {
        return hit.clone();
    }
    let made = (|| {
        let rt = runtime_index();
        let (body_params, body_ret) = rust_signature(signature_text(rt, symbol)?)?;
        let twin = format!("{symbol}_into");
        let (twin_params, twin_ret) = rust_signature(signature_text(rt, &twin)?)?;
        let (place, rest) = twin_params.split_first()?;
        let paired = !body_ret.is_empty()
            && squash(place) == format!("&mut{}", squash(&body_ret))
            && twin_ret.is_empty()
            && rest.len() == body_params.len()
            && rest.iter().zip(&body_params).all(|(a, b)| squash(a) == squash(b));
        paired.then(|| Some((twin.clone(), closure(&twin, rt).ok()?)))?
    })();
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(symbol.to_string(), made.clone());
    made
}
