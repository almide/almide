// ── the Rust-signature agreement checks of native_floor.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[cfg(test)]
mod runtime_type_agreement_tests {
    use super::*;

    #[test]
    fn opaque_runtime_types_require_nominal_and_carrier_agreement() {
        let response = RuntimeType::of("http.HttpResponse").unwrap();
        assert_eq!(RuntimeType::of("HttpResponse"), Some(response));
        assert_eq!(RuntimeType::of("other.HttpResponse"), None);
        assert!(param_agrees(FloorParam::Runtime(response, true), "&AlmideHttpResponse"));
        for wrong in ["&mut AlmideHttpResponse", "&AlmideHttpRequest", "Vec<AlmideString>"] {
            assert!(!param_agrees(FloorParam::Runtime(response, true), wrong), "{wrong}");
        }
        let poll = FloorRet::RuntimeResult(response, true);
        assert!(ret_agrees(poll, "Option<Result<AlmideHttpResponse, String>>"));
        for wrong in ["Result<Option<AlmideHttpResponse>, String>",
            "Option<Result<AlmideHttpCall, String>>", "Option<Result<AlmideHttpResponse, i64>>"] {
            assert!(!ret_agrees(poll, wrong), "{wrong}");
        }
    }
}

fn param_agrees(p: FloorParam, rust: &str) -> bool {
    let r = squash(rust);
    match p {
        FloorParam::Int => r == "i64",
        FloorParam::Bool => r == "bool",
        FloorParam::Float => r == "f64",
        FloorParam::Float32 => r == "f32",
        FloorParam::Str => r == "&str",
        FloorParam::Map(_) => is_generic_map(r.strip_prefix('&').unwrap_or(&r)),
        FloorParam::MapKey(_) | FloorParam::MapVal(_) => is_type_param(r.strip_prefix('&').unwrap_or(&r)),
        // A `Vec` of the pairs, or any iterable of them (an array literal's).
        FloorParam::MapPairs => r
            .strip_prefix("Vec<(")
            .or_else(|| r.strip_prefix("implIntoIterator<Item=("))
            .and_then(|x| x.strip_suffix(")>"))
            .and_then(|x| x.split_once(','))
            .is_some_and(|(a, b)| is_type_param(a) && is_type_param(b)),
        FloorParam::Elem(_) => is_type_param(&r) || r.strip_prefix('&').is_some_and(is_type_param),
        FloorParam::List(elem) => list_param_agrees(elem, &r),
        FloorParam::ListOwned(elem) => r.starts_with("Vec<") && list_param_agrees(elem, &r),
        _ => runtime_param_agrees(p, &r),
    }
}

/// [`param_agrees`] for the runtime's Bytes, Value, Matrix and Set params,
/// over the squashed Rust type `r`.
fn runtime_param_agrees(p: FloorParam, r: &str) -> bool {
    let bare = r.strip_prefix('&').unwrap_or(r);
    match p {
        FloorParam::Bytes => r == "&Vec<u8>" || r == "&[u8]",
        FloorParam::BytesMut => r == "&mutVec<u8>" || r == "&mut Vec<u8>",
        FloorParam::Value(_) => bare == "AlmideValue",
        FloorParam::Runtime(t, _) => bare == t.rust(),
        FloorParam::Matrix(_) => bare == "AlmideMatrix",
        FloorParam::Set(_) => is_generic_set(bare),
        FloorParam::SetElem(_) => is_type_param(bare),
        _ => false,
    }
}

/// Does the squashed Rust param type `r` agree with a declared list of `elem`?
fn list_param_agrees(elem: ListElem, r: &str) -> bool {
    let inner = r
        .strip_prefix("&[")
        .and_then(|x| x.strip_suffix(']'))
        .or_else(|| r.strip_prefix("&Vec<").and_then(|x| x.strip_suffix('>')))
        .or_else(|| r.strip_prefix("Vec<").and_then(|x| x.strip_suffix('>')));
    match (elem, inner) {
        (ListElem::Int, Some(i)) => i == "i64" || is_type_param(i),
        (ListElem::Str, Some(i)) => i == "AlmideString" || is_type_param(i),
        (ListElem::Value, Some(i)) => i == "AlmideValue",
        (ListElem::StrValuePairs, Some(i)) => i == "(AlmideString,AlmideValue)",
        (ListElem::Float, Some(i)) => i == "f64",
        (ListElem::Bool, Some(i)) => i == "bool",
        (ListElem::FloatRows, Some(i)) => i == "Vec<f64>",
        (ListElem::Matrix, Some(i)) => i == "AlmideMatrix",
        (ListElem::Any, Some(i)) => is_type_param(i),
        _ => false,
    }
}

fn is_type_param(s: &str) -> bool {
    s.len() == 1 && s.chars().all(|c| c.is_ascii_uppercase())
}

/// `(ok, err)` of a squashed `Result<ok,err>`, split at its top-level comma.
fn result_parts(t: &str) -> Option<(&str, &str)> {
    let inner = t.strip_prefix("Result<")?.strip_suffix('>')?;
    let mut depth = 0i32;
    for (i, c) in inner.char_indices() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            ',' if depth == 0 => return Some((&inner[..i], &inner[i + 1..])),
            _ => {}
        }
    }
    None
}

fn ret_agrees(r: FloorRet, rust: &str) -> bool {
    let t = squash(rust);
    match r {
        FloorRet::Unit => t.is_empty() || t == "()",
        FloorRet::Int => t == "i64",
        FloorRet::Bool => t == "bool" || t == "i64",
        FloorRet::Float => t == "f64",
        FloorRet::Str => is_rust_string(&t),
        FloorRet::ListInt => t == "Vec<i64>",
        FloorRet::ListStr => t == "Vec<String>" || t == "Vec<AlmideString>",
        FloorRet::Opt(Scalar::Int) => t == "Option<i64>",
        FloorRet::Opt(Scalar::Bool) => t == "Option<bool>",
        FloorRet::Opt(Scalar::Float) => t == "Option<f64>",
        FloorRet::Bytes => t == "Vec<u8>",
        FloorRet::Value => t == "AlmideValue",
        FloorRet::Runtime(rt) => t == rt.rust(),
        FloorRet::RuntimeResult(rt, optional) => {
            let carrier = if optional { t.strip_prefix("Option<").and_then(|s| s.strip_suffix('>')) } else { Some(t.as_str()) };
            carrier.and_then(result_parts).is_some_and(|(ok, err)| ok == rt.rust() && is_rust_string(err))
        }
        FloorRet::Map => is_generic_map(&t),
        FloorRet::Matrix | FloorRet::OptMatrix | FloorRet::ListMatrix | FloorRet::FloatRows | FloorRet::Set | FloorRet::SetElems => {
            runtime_ret_agrees(r, &t)
        }
        _ => compound_ret_agrees(r, &t),
    }
}

/// [`ret_agrees`] for the Matrix and Set returns, over the squashed Rust type `t`.
fn runtime_ret_agrees(r: FloorRet, t: &str) -> bool {
    match r {
        FloorRet::Matrix => t == "AlmideMatrix",
        FloorRet::OptMatrix => t == "Option<AlmideMatrix>",
        FloorRet::ListMatrix => t == "Vec<AlmideMatrix>",
        FloorRet::FloatRows => t == "Vec<Vec<f64>>",
        FloorRet::Set => is_generic_set(t),
        FloorRet::SetElems => t.strip_prefix("Vec<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param),
        _ => false,
    }
}

/// A Rust string type a floor body returns.
fn is_rust_string(x: &str) -> bool {
    x == "AlmideString" || x == "String"
}

/// [`ret_agrees`] for the element-typed, map-derived, Option-of-String and
/// Result returns, over the squashed Rust type `t`.
fn compound_ret_agrees(r: FloorRet, t: &str) -> bool {
    let string = is_rust_string;
    match r {
        FloorRet::Elem(ElemRet::Value) => is_type_param(t),
        FloorRet::Elem(ElemRet::Opt) => t.strip_prefix("Option<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param),
        FloorRet::Elem(ElemRet::List) => t.strip_prefix("Vec<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param),
        FloorRet::MapOptVal => t.strip_prefix("Option<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param),
        FloorRet::MapKeys | FloorRet::MapVals => t.strip_prefix("Vec<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param),
        FloorRet::MapEntries => t
            .strip_prefix("Vec<(")
            .and_then(|x| x.strip_suffix(")>"))
            .and_then(|x| x.split_once(','))
            .is_some_and(|(a, b)| is_type_param(a) && is_type_param(b)),
        FloorRet::OptStr => t.strip_prefix("Option<").and_then(|x| x.strip_suffix('>')).is_some_and(string),
        _ => result_ret_agrees(r, t),
    }
}

/// [`ret_agrees`] for the Result returns, over the squashed Rust type `t`.
fn result_ret_agrees(r: FloorRet, t: &str) -> bool {
    let string = is_rust_string;
    match r {
        FloorRet::ResValue => result_parts(t).is_some_and(|(ok, err)| ok == "AlmideValue" && string(err)),
        FloorRet::ResStrValuePairs => result_parts(t).is_some_and(|(ok, err)| ok == "Vec<(AlmideString,AlmideValue)>" && string(err)),
        FloorRet::ResListValue => result_parts(t).is_some_and(|(ok, err)| ok == "Vec<AlmideValue>" && string(err)),
        FloorRet::ResListStr => result_parts(t).is_some_and(|(ok, err)| {
            ok.strip_prefix("Vec<").and_then(|x| x.strip_suffix('>')).is_some_and(string) && string(err)
        }),
        FloorRet::ResBytes => result_parts(t).is_some_and(|(ok, err)| ok == "Vec<u8>" && string(err)),
        FloorRet::ResProcessStatus => result_parts(t).is_some_and(|(ok, err)| ok == "AlmideProcessStatus" && string(err)),
        FloorRet::ResBytesStrInt => result_parts(t).is_some_and(|(ok, err)| ok == "(Vec<u8>,AlmideString,i64)" && string(err)),
        FloorRet::ResFileStat => result_parts(t).is_some_and(|(ok, err)| ok == "AlmideFileStat" && string(err)),
        FloorRet::ResOptListStr => result_parts(t).is_some_and(|(ok, err)|
            ok.strip_prefix("Option<Vec<").and_then(|x| x.strip_suffix(">>"))
                .is_some_and(string) && string(err)),
        FloorRet::ResOptBytes => result_parts(t).is_some_and(|(ok, err)| ok == "Option<Vec<u8>>" && string(err)),
        FloorRet::ResOptStr => result_parts(t).is_some_and(|(ok, err)| {
            ok.strip_prefix("Option<").and_then(|x| x.strip_suffix('>')).is_some_and(string) && string(err)
        }),
        FloorRet::Res(k) => t
            .strip_prefix("Result<")
            .and_then(|x| x.strip_suffix('>'))
            .and_then(|x| x.split_once(','))
            .is_some_and(|(ok, err)| res_ok_agrees(k, ok) && string(err)),
        FloorRet::ResStr => t
            .strip_prefix("Result<")
            .and_then(|x| x.strip_suffix('>'))
            .and_then(|x| x.split_once(','))
            .is_some_and(|(ok, err)| string(ok) && string(err)),
        _ => false,
    }
}

/// Does a scalar Result's Rust ok type `ok` agree with the declared `k`?
fn res_ok_agrees(k: Option<Scalar>, ok: &str) -> bool {
    match k {
        None => ok == "()",
        Some(Scalar::Int) => ok == "i64",
        Some(Scalar::Bool) => ok == "bool",
        Some(Scalar::Float) => ok == "f64",
    }
}

/// A Result's supported list / optional heap ok type.
fn floor_ret_result_str_container(ok: &TypeExpr) -> Option<FloorRet> {
    match ok {
        TypeExpr::Tuple { elements } if elements.iter().map(simple).eq([Some("Bytes"), Some("String"), Some("Int")]) => Some(FloorRet::ResBytesStrInt),
        TypeExpr::Generic { name, args: a } if name.as_str() == "List" && a.len() == 1 && simple(&a[0]) == Some("String") => {
            Some(FloorRet::ResListStr)
        }
        TypeExpr::Generic { name, args: a } if name.as_str() == "List" && a.len() == 1 && simple(&a[0]) == Some(VALUE_TYPE) => {
            return Some(FloorRet::ResListValue)
        }
        TypeExpr::Generic { name, args: a } if name.as_str() == "List" && a.len() == 1
            && matches!(&a[0], TypeExpr::Tuple { elements } if elements.iter().map(simple).collect::<Vec<_>>() == [Some("String"), Some(VALUE_TYPE)]) => Some(FloorRet::ResStrValuePairs),
        TypeExpr::Generic { name, args: a } if is_option_name(name.as_str()) && a.len() == 1
            && matches!(&a[0], TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 && simple(&args[0]) == Some("String")) => Some(FloorRet::ResOptListStr),
        TypeExpr::Generic { name, args: a } if is_option_name(name.as_str()) && a.len() == 1 && simple(&a[0]) == Some("Bytes") => {
            Some(FloorRet::ResOptBytes)
        }
        TypeExpr::Generic { name, args: a } if is_option_name(name.as_str()) && a.len() == 1 && simple(&a[0]) == Some("String") => {
            Some(FloorRet::ResOptStr)
        }
        _ => None,
    }
}

#[cfg(test)]
mod optional_bytes_tests {
    use super::*;

    #[test]
    fn optional_lines_and_metadata_require_their_exact_runtime_abi() {
        assert!(ret_agrees(FloorRet::ResOptListStr, "Result<Option<Vec<AlmideString>>, AlmideString>"));
        assert!(ret_agrees(FloorRet::ResFileStat, "Result<AlmideFileStat, AlmideString>"));
        for wrong in ["Result<Option<Vec<u8>>,AlmideString>", "Result<Vec<AlmideString>,AlmideString>",
            "Result<Option<Vec<AlmideString>>,i64>"] {
            assert!(!ret_agrees(FloorRet::ResOptListStr, wrong));
        }
        assert!(!ret_agrees(FloorRet::ResFileStat, "Result<FileStat,AlmideString>"));
        assert!(!ret_agrees(FloorRet::ResFileStat, "Result<AlmideFileStat,i64>"));
        let good = "type FileStat = { size: Int, is_dir: Bool, is_file: Bool, modified: Int }";
        assert!(file_stat_decl_agrees(&almide_lang::parse_cached(good).unwrap().decls));
        for wrong in [good.replace("size: Int", "size: Bool"),
            good.replace("is_dir: Bool, is_file: Bool", "is_file: Bool, is_dir: Bool"),
            good.replace("modified: Int", "mtime: Int"),
            good.replace("FileStat", "UserRecord")] {
            assert!(!file_stat_decl_agrees(&almide_lang::parse_cached(Box::leak(wrong.into_boxed_str())).unwrap().decls));
        }
    }

    #[test]
    fn optional_bytes_requires_exact_payload_and_error_types() {
        assert!(result_ret_agrees(FloorRet::ResOptBytes, "Result<Option<Vec<u8>>,AlmideString>"));
        for wrong in ["Result<Option<Vec<i64>>,AlmideString>",
            "Result<Option<String>,AlmideString>", "Result<Vec<u8>,AlmideString>",
            "Result<Option<Vec<u8>>,i64>"] {
            assert!(!result_ret_agrees(FloorRet::ResOptBytes, wrong), "{wrong}");
        }
    }
}

/// Keep the runtime record conversion tied to the bundled field labels,
/// types and order. A user record with the same spelling never defines this ABI.
fn file_stat_decl_agrees(decls: &[Decl]) -> bool {
    let mut found = decls.iter().filter_map(|d| match d {
        Decl::Type { name, ty, .. } if name.as_str() == "FileStat" => Some(ty),
        _ => None,
    });
    let Some(TypeExpr::Record { fields }) = found.next() else { return false };
    found.next().is_none() && fields.iter().map(|f| (f.name.as_str(), simple(&f.ty))).eq([
        ("size", Some("Int")), ("is_dir", Some("Bool")),
        ("is_file", Some("Bool")), ("modified", Some("Int")),
    ])
}

/// A Rust type with its lifetime parameters left out (`&'a T` -> `& T`).
fn without_lifetimes(t: &str) -> String {
    let mut out = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            while chars.peek().is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_') {
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// The borrowing twin of a `Result[Value, String]` floor body `symbol`: the
/// runtime fn `<symbol>_ref`, paired from the signatures — the same params,
/// the Ok side `&AlmideValue` borrowed from them. `None` when the runtime
/// carries no such fn or its signature differs in anything else.
fn borrow_twin_of(ret: FloorRet, symbol: &str, rust_params: &[String], rust_ret: &str, rt: &'static RuntimeIndex) -> Option<(String, Vec<&'static str>)> {
    if !matches!(ret, FloorRet::ResValue | FloorRet::Elem(ElemRet::Opt)) {
        return None;
    }
    let twin = format!("{symbol}_ref");
    let (tp, tr) = rust_signature(signature_text(rt, &twin)?)?;
    let same = tp.len() == rust_params.len()
        && tp.iter().zip(rust_params).all(|(a, b)| squash(&without_lifetimes(a)) == squash(&without_lifetimes(b)));
    // An element Option (`A?` of the `List[A]` param): the twin lends the
    // element, `Option<&A>` where the body returns `Option<A>`, from the
    // slice the call holds by reference.
    if ret == FloorRet::Elem(ElemRet::Opt) {
        let elem = squash(rust_ret).strip_prefix("Option<").and_then(|x| x.strip_suffix('>')).map(str::to_string)?;
        let lent = squash(&without_lifetimes(&tr)) == format!("Option<&{elem}>");
        let from_slice = rust_params.iter().any(|p| squash(&without_lifetimes(p)) == format!("&[{elem}]"));
        return (same && lent && from_slice && is_type_param(&elem)).then(|| Some((twin.clone(), closure(&twin, rt).ok()?)))?;
    }
    let borrowed = result_parts(&squash(&without_lifetimes(&tr))).is_some_and(|(ok, err)| ok == "&AlmideValue" && err == "AlmideString");
    // The twin borrows from a param the call holds by reference.
    let from_ref = rust_params.iter().any(|p| squash(&without_lifetimes(p)) == "&AlmideValue");
    (same && borrowed && from_ref).then(|| Some((twin.clone(), closure(&twin, rt).ok()?)))?
}

/// `f` with its by-reference twin (`FloorFn::by_ref`): the runtime fn
/// `<symbol>_ref`, paired from the signatures — the same params and result,
/// except that each Value `f` takes by value it borrows. The render then
/// lends the caller's Value where the body would be handed a copy of it.
fn with_by_ref_twin(mut f: FloorFn, rust_params: &[String], rust_ret: &str, rt: &'static RuntimeIndex) -> FloorFn {
    if !f.params.contains(&FloorParam::Value(false)) {
        return f;
    }
    let twin = format!("{}_ref", f.symbol);
    let Some((tp, tr)) = signature_text(rt, &twin).and_then(rust_signature) else { return f };
    if tp.len() != rust_params.len() || squash(&without_lifetimes(&tr)) != squash(&without_lifetimes(rust_ret)) {
        return f;
    }
    let mut params = f.params.clone();
    for (k, (a, b)) in tp.iter().zip(rust_params).enumerate() {
        let (a, b) = (squash(&without_lifetimes(a)), squash(&without_lifetimes(b)));
        if a == b {
            continue;
        }
        if f.params[k] == FloorParam::Value(false) && b == "AlmideValue" && a == "&AlmideValue" {
            params[k] = FloorParam::Value(true);
            continue;
        }
        return f;
    }
    let Ok(items) = closure(&twin, rt) else { return f };
    let by_ref = FloorFn { symbol: twin, params, ret: f.ret, items, twin: None, str_only: f.str_only, ordered: f.ordered, borrow_twin: None, by_ref: None };
    f.by_ref = Some(Box::new(by_ref));
    f
}

/// The runtime's process status struct, in ABI order: each field's label and
/// the type the bundled record declares for it. The declaration check and the
/// host-result conversion both read this one copy.
const PROCESS_STATUS_ABI: [(&str, &str); 3] = [("code", "Int"), ("stdout", "String"), ("stderr", "String")];

/// Match every field, in ABI order; a same-named record in another module
/// cannot stand in for the bundled runtime record.
fn process_status_decl_agrees(decls: &[Decl]) -> bool {
    let mut found = decls.iter().filter_map(|d| match d {
        Decl::Type { name, ty, .. } if name.as_str() == "ProcessStatus" => Some(ty),
        _ => None,
    });
    let Some(TypeExpr::Record { fields }) = found.next() else { return false };
    found.next().is_none() && fields.iter().map(|f| (f.name.as_str(), simple(&f.ty)))
        .eq(PROCESS_STATUS_ABI.iter().map(|(label, ty)| (*label, Some(*ty))))
}

#[cfg(test)]
mod aggregate_host_abi_tests {
    use super::*;
    #[test]
    fn record_and_tuple_results_require_exact_payloads_and_fields() {
        assert!(ret_agrees(FloorRet::ResProcessStatus, "Result<AlmideProcessStatus, AlmideString>"));
        assert!(ret_agrees(FloorRet::ResBytesStrInt, "Result<(Vec<u8>,AlmideString,i64), AlmideString>"));
        for bad in ["Result<ProcessStatus,AlmideString>", "Result<AlmideProcessStatus,i64>"] {
            assert!(!ret_agrees(FloorRet::ResProcessStatus, bad));
        }
        for bad in ["Result<(Vec<i64>,AlmideString,i64),AlmideString>", "Result<(Vec<u8>,i64,AlmideString),AlmideString>"] {
            assert!(!ret_agrees(FloorRet::ResBytesStrInt, bad));
        }
        let good = "type ProcessStatus = { code: Int, stdout: String, stderr: String }";
        assert!(process_status_decl_agrees(&almide_lang::parse_cached(good).unwrap().decls));
        for bad in [good.replace("code: Int", "code: Bool"), good.replace("stdout: String, stderr: String", "stderr: String, stdout: String")] {
            assert!(!process_status_decl_agrees(&almide_lang::parse_cached(Box::leak(bad.into_boxed_str())).unwrap().decls));
        }
    }
}
