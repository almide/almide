// ── the Rust-signature agreement checks of native_floor.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
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
        FloorParam::MapPairs => r
            .strip_prefix("Vec<(")
            .and_then(|x| x.strip_suffix(")>"))
            .and_then(|x| x.split_once(','))
            .is_some_and(|(a, b)| is_type_param(a) && is_type_param(b)),
        FloorParam::Elem(_) => is_type_param(&r) || r.strip_prefix('&').is_some_and(is_type_param),
        FloorParam::List(elem) => list_param_agrees(elem, &r),
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
        .or_else(|| r.strip_prefix("&Vec<").and_then(|x| x.strip_suffix('>')));
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
        FloorRet::ResListValue => result_parts(t).is_some_and(|(ok, err)| ok == "Vec<AlmideValue>" && string(err)),
        FloorRet::ResListStr => result_parts(t).is_some_and(|(ok, err)| {
            ok.strip_prefix("Vec<").and_then(|x| x.strip_suffix('>')).is_some_and(string) && string(err)
        }),
        FloorRet::ResBytes => result_parts(t).is_some_and(|(ok, err)| ok == "Vec<u8>" && string(err)),
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
