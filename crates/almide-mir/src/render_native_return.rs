// Return spelling is independent of a function's final MIR value: Never
// has no value, while a Result[Never, E] can return its real Err carrier.
fn native_return_spelling(func: &MirFunction, tys: &BTreeMap<ValueId, NTy>, ret_override: Option<NTy>) -> Result<String, LowerError> {
    if ret_override == Some(NTy::Never) { return Ok(" -> !".into()); }
    let ret = match func.ret {
        None => String::new(),
        Some(v) => match ret_override.as_ref().or_else(|| tys.get(&v)) {
            Some(NTy::Never) => " -> !".to_string(),
            Some(NTy::I64) => " -> i64".to_string(),
            Some(NTy::Str) => " -> AlmideString".to_string(),
            Some(NTy::StrRef) => " -> AlmideString".to_string(),
            Some(NTy::Vec) => " -> Vec<i64>".to_string(),
            Some(NTy::VecRef) => " -> Vec<i64>".to_string(),
            Some(NTy::F64) => " -> f64".to_string(), Some(NTy::F32) => " -> f32".to_string(),
            Some(NTy::Res) => " -> Result<i64, AlmideString>".to_string(),
            Some(NTy::VecStr | NTy::VecStrRef) => " -> Vec<AlmideString>".to_string(),
            Some(NTy::Rec(i) | NTy::RecRef(i)) => format!(" -> {}", crate::render_native_rec::struct_name(*i)),
            Some(NTy::Opt) => " -> Option<i64>".to_string(),
            Some(NTy::ResStr) => " -> Result<AlmideString, AlmideString>".to_string(),
            None => return Err(wall("native: return value untyped")),
        },
    };
    Ok(ret)
}
