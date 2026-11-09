// A raw Never function has no MIR return value but still has the Rust ! ABI.
fn native_return_spelling(func: &MirFunction, tys: &BTreeMap<ValueId, NTy>, ret_override: Option<NTy>) -> Result<String, LowerError> {
    if ret_override == Some(NTy::Never) { return Ok(" -> !".into()); }
    match func.ret {
        None => match ret_override {
            Some(NTy::F32) => Err(wall("native: metadata kind cannot be a returned MIR value")),
            Some(t) => Ok(format!(" -> {}", crate::render_native_rec::owned_ty(t))),
            None => Ok(String::new()),
        },
        Some(v) => match ret_override.as_ref().or_else(|| tys.get(&v)) {
            Some(NTy::F32 | NTy::Never) => Err(wall("native: metadata kind cannot be a returned MIR value")),
            Some(t) => Ok(format!(" -> {}", crate::render_native_rec::owned_ty(*t))),
            None => Err(wall("native: return value untyped")),
        },
    }
}
