// `@export(c)` on the native render (#4447) — include!-spliced into
// native_foreign.rs, sharing that module's imports and its value adapter.
//
// An exported fn is lowered, rendered and certified like any other; what a C
// caller links is a crate-root adapter over it, as a linked callback's is
// (native_callbacks). Every C type and conversion of the adapter is the row
// the one C ABI table gives (`extern_abi::c_export_param_abi` /
// `c_export_return_abi`, the rows the standard codegen's wrapper reads): `Int`
// is an `int64_t`, a sized integer or float crosses at its own width, `Bool`
// is an `int`, `Unit` is `void`. An effect fn answers its ok value; its err
// writes the message to stderr and exits 1 before the caller regains control
// (C-452).
//
// Only the scalar rows are adapted here. A signature holding any other row (a
// `Region` lent over the caller's buffer, a `Handle`) walls, so its build
// keeps the standard codegen, which renders that wrapper.

fn c_export(f: &IrFunction) -> Option<&almide_lang::ast::ExportAttr> {
    f.export_attrs.iter().find(|a| a.target.as_str() == "c")
}

/// Is `f` exported to C? Such a fn is reached from outside the Almide call
/// graph, so the link keeps it.
pub(crate) fn is_c_export(f: &IrFunction) -> bool {
    c_export(f).is_some()
}

/// The table's row for `ty` in an `@export(c)` signature, when it is one of
/// the scalar rows this render adapts.
fn export_scalar(ty: &Ty, param: bool) -> Option<CAbi> {
    let abi = if param { c_export_param_abi(ty) } else { c_export_return_abi(ty) }?;
    let adapted = scalar_host_type(ty).is_some() || (!param && *ty == Ty::Unit);
    (adapted && matches!(abi.conv, CConv::Same | CConv::BoolAsI32)).then_some(abi)
}

/// Wall a program whose `@export(c)` signatures leave the scalar rows, before
/// reachability pruning: an export no Almide code calls is still required by
/// its C caller.
pub(crate) fn check_exports(ir: &IrProgram) -> Result<(), LowerError> {
    for f in ir.functions.iter().chain(ir.modules.iter().flat_map(|m| &m.functions)).filter(|f| is_c_export(f)) {
        let refuse = |what: String| LowerError::at(
            f.body.span,
            format!("native: @export(c) fn `{}`: {what} is outside the scalar C rows the native rungs adapt", f.name),
        );
        if let Some(p) = f.params.iter().find(|p| p.is_mut || export_scalar(&p.ty, true).is_none()) {
            return Err(refuse(format!("parameter `{}: {}`", p.name, p.ty.display())));
        }
        if export_scalar(&f.ret_ty, false).is_none() {
            return Err(refuse(format!("return type `{}`", f.ret_ty.display())));
        }
    }
    Ok(())
}

/// The crate-root C adapters, one per exported fn of the linked program.
pub(crate) fn render_exports(ir: &IrProgram, sigs: &NativeSigs) -> Result<String, LowerError> {
    let mut out = String::new();
    for f in &ir.functions {
        if let Some(attr) = c_export(f) {
            out.push_str(&export_adapter(f, attr.symbol.as_str(), sigs)?);
        }
    }
    Ok(out)
}

fn export_adapter(f: &IrFunction, symbol: &str, sigs: &NativeSigs) -> Result<String, LowerError> {
    let unrendered = || wall(format!("native: @export(c) fn `{}` has no rendered scalar signature", f.name));
    let (kinds, result) = sigs.get(f.name.as_str()).filter(|(k, _)| k.len() == f.params.len()).ok_or_else(unrendered)?;
    let mut c_params = Vec::new();
    let mut args = Vec::new();
    for (j, (p, kind)) in f.params.iter().zip(kinds).enumerate() {
        let abi = export_scalar(&p.ty, true).ok_or_else(unrendered)?;
        c_params.push(format!("a{j}: {}", abi.c_ty));
        let host = if abi.conv == CConv::BoolAsI32 { format!("(a{j} != 0)") } else { format!("a{j}") };
        args.push(match kind.narrow_rust() {
            // A narrow integer parameter is its real Rust type: the C type.
            Some(rust) if rust == abi.c_ty => host,
            Some(_) => return Err(unrendered()),
            None => convert(&format!("&{host}"), &p.ty, crate::render_native_closure::owned_nty(*kind), false, 0)?,
        });
    }
    let abi = export_scalar(&f.ret_ty, false).ok_or_else(unrendered)?;
    let value = export_value(f, *result)?;
    let returned = if abi.conv == CConv::BoolAsI32 { "__almide_host as i32" } else { "__almide_host" };
    let name = crate::render_native::mangle(f.name.as_str());
    Ok(format!(
        "#[export_name = {symbol:?}]\npub extern \"C\" fn __c_{name}({}) -> {} {{\n    let __almide_ret = {name}({});\n    let __almide_host = {value};\n    {returned}\n}}\n",
        c_params.join(", "), abi.c_ty, args.join(", "),
    ))
}

/// The host value of `__almide_ret`, the rendered fn's result of kind
/// `result`: the declared return, or the ok value of an effect fn's carrier.
fn export_value(f: &IrFunction, result: Option<NativeSigKind>) -> Result<String, LowerError> {
    let Some(sig) = result else { return Ok("__almide_ret".to_string()) };
    if sig.narrow_rust().is_some() {
        return Ok("__almide_ret".to_string());
    }
    let kind = crate::render_native_closure::owned_nty(sig);
    let carrier = matches!(kind, NTy::Res | NTy::ResStr)
        || matches!(kind, NTy::Rec(i) if crate::render_native_rec::result_payloads(i).is_some());
    if !(f.is_effect && carrier) {
        return convert("&__almide_ret", &f.ret_ty, kind, true, 0);
    }
    let lifted = Ty::result(f.ret_ty.clone(), Ty::String);
    let host = convert("&__almide_ret", &lifted, kind, true, 0)?;
    Ok(format!(
        "match {host} {{ Ok(__almide_v) => __almide_v, Err(__almide_e) => {{ eprintln!(\"{{}}\", __almide_e); almide_host_exit(1) }} }}"
    ))
}
