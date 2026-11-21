// Host callback arguments use the ordinary typed closure dispatch. The host
// borrows this adapter for the call; safe Rust cannot retain that borrow.
fn callback(value: &str, params: &[Ty], ret: &Ty, effect: bool, kind: NTy) -> Result<String, LowerError> {
    let NTy::Rec(i) = kind else { return Err(wall("native: foreign callback has no typed environment")); };
    let (kinds, result) = crate::render_native_rec::closure_sig(i)
        .ok_or_else(|| wall("native: foreign callback has no checked signature"))?;
    if kinds.len() != params.len() { return Err(wall("native: foreign callback arity differs from its checked signature")); }
    let ret_ty = if effect && ret.result_ok_ty().is_none() { Ty::result(ret.clone(), Ty::String) } else { ret.clone() };
    let key = crate::render_native_closure::demand_borrowed_dispatch(&kinds, result);
    let mut bindings = String::new();
    let mut args = Vec::new();
    let mut host_params = Vec::new();
    for (j, (ty, kind)) in params.iter().zip(kinds).enumerate() {
        host_params.push(format!("__host_p{j}: {}", host_type(ty)?));
        let native = convert_value(&format!("&__host_p{j}"), ty, kind, false, 0, true)?;
        write!(bindings, "let __native_p{j}: {} = {native}; ", crate::render_native_rec::owned_ty(kind)).expect("writing to a String cannot fail");
        let borrowed = matches!(kind, NTy::Str | NTy::StrRef | NTy::Vec | NTy::VecRef | NTy::VecStr | NTy::VecStrRef | NTy::Rec(_) | NTy::RecRef(_));
        args.push(format!("{}__native_p{j}", if borrowed { "&" } else { "" }));
    }
    let call = format!("__almd_ci_{key}(__foreign_index, __foreign_env{})", args.iter().map(|a| format!(", {a}")).collect::<String>());
    let body = match result {
        Some(k) => format!("let __native_result = {call}; {}", convert_value("&__native_result", &ret_ty, k, true, 0, true)?),
        None if ret_ty == Ty::Unit => format!("{call}; ()"),
        _ => return Err(wall("native: foreign callback return differs from its checked signature")),
    };
    Ok(format!("{{ let __foreign_env = &{value}; let __foreign_index = __foreign_env.words().first().copied().unwrap_or_else(|| almide_abort(\"index out of bounds\")); move |{}| -> {} {{ {bindings}{body} }} }}", host_params.join(", "), host_type(&ret_ty)?))
}

thread_local! {
    static IN_NOMINAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `f` with the host spellings of a value held INSIDE a record or variant, or
/// in a generic argument of one: the program's own types, which the host uses
/// as they are (ADR-0037 ruling (A)) — a list there is the `AlmideList` block,
/// where a list the host passes or receives directly is std's `Vec`.
pub(crate) fn in_nominal<R>(f: impl FnOnce() -> R) -> R {
    let was = IN_NOMINAL.with(|n| n.replace(true));
    let r = f();
    IN_NOMINAL.with(|n| n.set(was));
    r
}

pub(crate) fn nominal() -> bool {
    IN_NOMINAL.with(std::cell::Cell::get)
}

pub(crate) fn host_type(ty: &Ty) -> Result<String, LowerError> {
    if let Some(s) = scalar_host_type(ty) { return Ok(s.into()); }
    let list = if nominal() { "AlmideList" } else { "Vec" };
    Ok(match ty {
        Ty::Unit => "()".into(), Ty::String => "AlmideString".into(), Ty::Bytes => format!("AlmideRcCow<{list}<u8>>"),
        t if crate::render_native_rec::is_matrix_ty(t) => "AlmideRcCow<AlmideMatrix>".into(),
        Ty::Applied(c, a) => {
            let name = match c {
                C::List => list, C::Option => "Option", C::Result => "Result",
                C::Map => "AlmideMap", C::Set => "AlmideSet",
                _ => return Err(wall("native: foreign callback has no host container type")),
            };
            format!("{name}<{}>", a.iter().map(host_type).collect::<Result<Vec<_>, _>>()?.join(", "))
        }
        Ty::Tuple(a) => format!("({},)", a.iter().map(host_type).collect::<Result<Vec<_>, _>>()?.join(", ")),
        Ty::Named(name, args) => crate::native_callback_types::host_named(name.as_str(), args)?,
        Ty::Record { .. } => match crate::native_callback_types::nominal(ty) {
            Some(named) => host_type(&named)?,
            None => return Err(wall("native: foreign callback has no host value type")),
        },
        _ => return Err(wall("native: foreign callback has no host value type")),
    })
}
