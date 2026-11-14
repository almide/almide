// Included by native_hof: the receivers a call lends its callback.
/// The receiver a call lends its one callback: an element Option held as
/// the lent element, handed to a combinator whose callback only takes its
/// payload ([`lent_var`]); or else a list receiver whose elements reach the
/// callback through the twin that hands each by reference
/// ([`elem_lent_twin`]).
#[allow(clippy::type_complexity)]
fn lent_receivers<'h>(args: &[CallArg], hof: &'h Hof, env: &Env) -> (Option<(usize, ValueId, &'h str)>, Option<(&'h str, &'static str, &'static str)>) {
    let lent = args.iter().enumerate().find_map(|(k, a)| match a {
        CallArg::Handle(v) if crate::render_native_borrow_read::borrowed_option(*v) => Some((k, *v, lent_var(hof, k)?)),
        _ => None,
    });
    let elem = if lent.is_none() { elem_lent_twin(hof, env) } else { None };
    (lent, elem)
}

/// The twin a call names once its callback took the lent element: the
/// element twin (its text joins the shims) when no other route chose one.
fn elem_twin_taken(twin: &'static str, hof: &Hof, elem: Option<&(&str, &'static str, &'static str)>, side: &mut FnArgOut) -> &'static str {
    match elem {
        Some((_, name, item)) if side.lent && twin == hof.twin => {
            side.shims.push(item);
            name
        }
        _ => twin,
    }
}

thread_local! {
    static ELEM_LENT: std::cell::RefCell<BTreeMap<String, Option<(&'static str, &'static str)>>> = const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// The element type param `A` of `h`'s borrowed `List[A]` receiver, its
/// twin's name and text, when the twin hands every element to the one
/// callback by reference: the static body reads `f(x.clone())` once, its
/// callback is `impl Fn(A)`, and `A` (a heap kind in `env`) reaches nothing
/// else — not the result, another argument or what the callback returns.
/// The twin reads `f(x)` over `impl Fn(&A)`: the same elements in the same
/// order, none copied (`list.any(xs, (s) => …)` copied each element for a
/// callback that only reads it).
fn elem_lent_twin<'h>(h: &'h Hof, env: &Env) -> Option<(&'h str, &'static str, &'static str)> {
    let k = h.params.iter().position(|(d, by_ref)| *by_ref && matches!(d, DTy::List(e) if matches!(&**e, DTy::Var(_))))?;
    let DTy::List(e) = &h.params[k].0 else { return None };
    let DTy::Var(t) = &**e else { return None };
    let mut callbacks = h.params.iter().filter(|(d, _)| matches!(d, DTy::Fn(..)));
    let (Some((DTy::Fn(ps, r), _)), None) = (callbacks.next(), callbacks.next()) else { return None };
    let free = matches!(ps.as_slice(), [DTy::Var(v)] if v == t)
        && !mentions_var(r, t)
        && h.owned.is_none()
        && !h.mutable.contains(&true)
        && !mentions_var(&h.ret, t)
        && !h.bounds.iter().any(|(v, _)| v == t)
        && h.params.iter().enumerate().all(|(j, (d, _))| j == k || matches!(d, DTy::Fn(..)) || !mentions_var(d, t));
    let heap = matches!(kind_of(env, &**e).map(crate::render_native::owned_nty), Some(NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_)));
    if !free || !heap {
        return None;
    }
    let (name, item) = ELEM_LENT.with(|m| {
        m.borrow_mut().entry(h.twin.clone()).or_insert_with(|| {
            let head = format!("fn {}", h.twin);
            let text = h.items.iter().find(|i| i.matches("fn ").count() == 1 && i.contains(&format!("{head}<")))?;
            // The runtime spells the element param under its own name.
            let (_, rest) = text.split_once("impl Fn(")?;
            let (param, _) = rest.split_once(") ->")?;
            let callback = format!("impl Fn({param}) ->");
            let single = !param.is_empty() && param.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !single || text.matches("f(x.clone())").count() != 1 || text.matches("impl Fn(").count() != 1 || text.matches("f(").count() != 2 {
                return None;
            }
            let name = format!("{}_lent", h.twin);
            let lent = text.replacen(&head, &format!("fn {name}"), 1)
                .replacen(&callback, &format!("impl Fn(&{param}) ->"), 1)
                .replacen("f(x.clone())", "f(x)", 1);
            Some((&*Box::leak(name.into_boxed_str()), &*Box::leak(lent.into_boxed_str())))
        }).clone()
    })?;
    Some((t.as_str(), name, item))
}

/// The type param `B` the one callback of `h` returns when the runtime only
/// orders it: bounded `Ord`, and named by nothing else — not the result,
/// another argument or a callback param (`list.sort_by`'s key).
fn key_only_var(h: &Hof) -> Option<&str> {
    let mut callbacks = h.params.iter().filter(|(d, _)| matches!(d, DTy::Fn(..)));
    let (Some((DTy::Fn(ps, r), _)), None) = (callbacks.next(), callbacks.next()) else { return None };
    let DTy::Var(b) = &**r else { return None };
    let ordered = h.bounds.iter().any(|(v, c)| v == b && *c == almide_lang::capability_bounds::Cap::Ord);
    let alone = !mentions_var(&h.ret, b)
        && !ps.iter().any(|p| mentions_var(p, b))
        && h.params.iter().all(|(d, _)| matches!(d, DTy::Fn(..)) || !mentions_var(d, b));
    (ordered && alone).then_some(b.as_str())
}

/// The callback `closure` (returning the tuple record `i`) returning the
/// same fields as a Rust tuple: a tuple orders as the derived record order
/// does, field by field, and std's sort runs its cheaper comparison on it.
/// `None` unless the record derives that order and the closure spells its
/// return type as expected.
fn tuple_key(closure: &str, i: u16) -> Option<String> {
    let fields = rec::tuple_fields(i)?;
    let boxed = fields.iter().any(|f| rec::enum_field_boxed(i, *f));
    if boxed || !crate::render_native_rec::native_order_safe(i) || !(2..=12).contains(&fields.len()) {
        return None;
    }
    let ret = format!("-> {} {{ ", rec::struct_name(i));
    let at = closure.find(&ret)?;
    let body = closure[at + ret.len()..].strip_suffix(" }")?;
    let tys: Vec<String> = fields.iter().map(|f| rec::owned_ty(*f)).collect();
    let parts: Vec<String> = (0..fields.len()).map(|k| format!("__k.f{k}")).collect();
    Some(format!("{}-> ({},) {{ let __k = {{ {body} }}; ({},) }}", &closure[..at], tys.join(", "), parts.join(", ")))
}
