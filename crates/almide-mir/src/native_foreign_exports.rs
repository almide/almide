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
// The scalar rows and `Handle` (a `*mut u8` whose address the i64 lane holds,
// the same pointer coming back) are adapted here, and a `Region` parameter
// (ADR-0031 amendment A2): the caller's `(uint8_t*, size_t)` is lent to the
// fn as a Region over a block holding the caller's bytes
// (`almide_rt_ffi_block_lend`); after the call a copy of it still held stops
// the process, and a `@region(out = p)` parameter's bytes go back to the
// caller's buffer (`almide_rt_ffi_block_lent_end`). A signature holding any
// other row walls.

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
    let adapted = scalar_host_type(ty).is_some() || *ty == Ty::RawPtr || (!param && *ty == Ty::Unit);
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
        if let Some(p) = f.params.iter().find(|p| p.is_mut || (export_scalar(&p.ty, true).is_none() && !is_region(&p.ty))) {
            return Err(refuse(format!("parameter `{}: {}`", p.name, p.ty.display())));
        }
        if export_scalar(&f.ret_ty, false).is_none() {
            return Err(refuse(format!("return type `{}`", f.ret_ty.display())));
        }
    }
    Ok(())
}

/// The crate-root C adapters, one per exported fn of the linked program.
pub(crate) fn render_exports(ir: &IrProgram, sigs: &NativeSigs, mir: &[MirFunction]) -> Result<String, LowerError> {
    let mut out = String::new();
    for f in &ir.functions {
        if let Some(attr) = c_export(f) {
            // Which params the rendered fn takes over (a move param arrives
            // as the caller's own value, render_native::param_kinds).
            let owned: Vec<bool> = match mir.iter().find(|m| m.name == f.name.as_str()) {
                Some(m) => m.params.iter().map(|p| m.owned_params.contains(&p.value) || m.inout_params.contains(&p.value)).collect(),
                None => vec![false; f.params.len()],
            };
            out.push_str(&export_adapter(f, attr.symbol.as_str(), sigs, &owned)?);
        }
    }
    Ok(out)
}

fn export_adapter(f: &IrFunction, symbol: &str, sigs: &NativeSigs, owned: &[bool]) -> Result<String, LowerError> {
    let unrendered = || wall(format!("native: @export(c) fn `{}` has no rendered scalar signature", f.name));
    let (kinds, result) = sigs.get(f.name.as_str()).filter(|(k, _)| k.len() == f.params.len()).ok_or_else(unrendered)?;
    let mut c_params = Vec::new();
    let mut args = Vec::new();
    let mut lends = String::new();
    let mut ends = String::new();
    for (j, (p, kind)) in f.params.iter().zip(kinds).enumerate() {
        if is_region(&p.ty) {
            let writable = export_out_params(f).iter().any(|o| o == p.name.as_str());
            let (record, block) = lent_export_region(j, *kind, writable, owned.get(j).copied().unwrap_or(false))?;
            c_params.push(format!("a{j}: {}, a{j}_len: usize", if writable { "*mut u8" } else { "*const u8" }));
            lends.push_str(&format!("    let {block} = almide_rt_ffi_block_lend(a{j} as *const u8, a{j}_len);\n"));
            args.push(record);
            ends.push_str(&format!("    almide_rt_ffi_block_lent_end(&{block}, a{j} as *mut u8, {writable}, {symbol:?}, {:?});\n", p.name.as_str()));
            continue;
        }
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
        "#[export_name = {symbol:?}]\npub extern \"C\" fn __c_{name}({}) -> {} {{\n{lends}    let __almide_ret = {name}({});\n{ends}    let __almide_host = {value};\n    {returned}\n}}\n",
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

fn is_region(ty: &Ty) -> bool {
    almide_lang::types::extern_abi::is_region(ty)
}

/// The parameters the declaration names as caller-provided writable buffers,
/// `@region(out = p)` (ADR-0031 amendment A2; `check` validated each names a
/// Region parameter, E090).
fn export_out_params(f: &IrFunction) -> Vec<String> {
    use almide_lang::ast::AttrValue;
    f.attrs.iter()
        .filter(|a| a.name.as_str() == almide_lang::stdlib_info::REGION_ATTR)
        .flat_map(|a| a.args.iter())
        .filter(|a| a.name.is_some_and(|n| n.as_str() == "out"))
        .filter_map(|a| match &a.value { AttrValue::Ident { name } => Some(name.as_str().to_string()), _ => None })
        .collect()
}

/// The argument expression lending parameter `j` (rendered kind `kind`) as a
/// Region over the block `__almide_lent{j}`: the ffi record with that block
/// shared, offset 0, the caller's length, writable as declared and every
/// other field its default (the owner Bytes is empty: the bytes are the
/// block's). The record is a temporary of the call, so after it returns only
/// a copy the fn kept still shares the block.
fn lent_export_region(j: usize, kind: NativeSigKind, writable: bool, owned: bool) -> Result<(String, String), LowerError> {
    let (NTy::Rec(i) | NTy::RecRef(i)) = crate::render_native_closure::owned_nty(kind) else {
        return Err(wall(format!("native: an @export(c) Region parameter must be the ffi record, not {kind:?}")));
    };
    let (base, off, len) = lent_region(NTy::Rec(i))?;
    let (_, fields) = crate::render_native_rec::record_fields(i)
        .ok_or_else(|| wall("native: an @export(c) Region parameter must be the ffi record"))?;
    // `writable` is a `Bool`, carried as `bool` or in the i64 lane.
    let w = fields.iter().position(|(n, t)| n == "writable" && matches!(t, NTy::Bool | NTy::I64))
        .ok_or_else(|| wall("native: an @export(c) Region parameter must be the ffi record"))?;
    let writable_value = if fields[w].1 == NTy::Bool { writable.to_string() } else { i64::from(writable).to_string() };
    let block = format!("__almide_lent{j}");
    // Every field spelled: the block shares the lend, the view is the whole
    // buffer, and the rest (the empty owner Bytes, the no-op keep) is each
    // field's default; the record itself has none (its block has none).
    let mut values = Vec::new();
    for (k, (_, t)) in fields.iter().enumerate() {
        values.push(match k {
            k if k == base => format!("f{k}: {block}.clone()"),
            k if k == off => format!("f{k}: 0"),
            k if k == len => format!("f{k}: i64::try_from(a{j}_len).unwrap_or(i64::MAX)"),
            k if k == w => format!("f{k}: {writable_value}"),
            k if crate::render_native_rec::has_default(*t) => format!("f{k}: Default::default()"),
            _ => return Err(wall("native: an @export(c) Region parameter's ffi record has a field with no default")),
        });
    }
    let record = format!("{} {{ {} }}", crate::render_native_rec::struct_name(i), values.join(", "));
    // A record param is spelled `&AlmdRecN` unless the fn takes it over.
    Ok((if owned { record } else { format!("&{record}") }, block))
}
