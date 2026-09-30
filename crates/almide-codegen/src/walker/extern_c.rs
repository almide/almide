// ── C FFI extern codegen ──────────────────────────────────────

/// Render `@extern(rust, "module", "function")` as a thin wrapper that
/// delegates to `module::function(..)`.
///
/// The wrapper's params follow the `@extern(rust)` ABI
/// (`extern_rust_borrow_mode`, #3045) through `param.borrow` — the same modes
/// the call sites are decorated with — so a call site and the wrapper cannot
/// disagree. The host fn sees plain Rust types: `&str`, `&[T]`, `&[u8]`,
/// `&AlmideMatrix`, `&T` for records / maps / sets, `&mut T` for `mut`
/// params, and owned scalars / `Option` / `Result` / tuples / variants /
/// `Rc<dyn Fn>`. A `Bytes` / `Matrix` RETURN accepts either the raw
/// `Vec<u8>` / `AlmideMatrix` or the `AlmideRcCow` handle (`.into()`).
fn render_native_call(ctx: &RenderContext, func: &IrFunction, attr: &almide_lang::ast::ExternAttr, emit_name: &str) -> String {
    use types::render_type;
    let mod_name = attr.module.as_str();
    let fn_name = attr.function.as_str();
    let params: Vec<String> = func.params.iter()
        .map(|p| format!("{}: {}", p.name, extern_rust_param_type(ctx, p)))
        .collect();
    let args: Vec<String> = func.params.iter().map(|p| p.name.to_string()).collect();
    let ret = render_type(ctx, &func.ret_ty);
    let call = format!("{}::{}({})", mod_name, fn_name, args.join(", "));
    let body = if extern_rust_is_rc_cow(&func.ret_ty) { format!("{call}.into()") } else { call };
    format!("fn {}({}) -> {} {{\n    {}\n}}", emit_name, params.join(", "), ret, body)
}

/// `Bytes` / `Matrix`: the two value types generated code holds behind an
/// `AlmideRcCow` handle (#617).
fn extern_rust_is_rc_cow(ty: &Ty) -> bool {
    use almide_lang::types::TypeConstructorId;
    matches!(ty, Ty::Bytes | Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _))
}

/// One `@extern(rust)` wrapper param type, from the mode the ABI assigned
/// (`extern_rust_borrow_mode`). A borrowed `Bytes` / `Matrix` is the raw
/// value behind the handle — `&AlmideRcCow<Vec<u8>>` at the call site
/// deref-coerces to `&[u8]` — so the host never sees `AlmideRcCow`.
fn extern_rust_param_type(ctx: &RenderContext, p: &IrParam) -> String {
    use types::render_type;
    use almide_lang::types::TypeConstructorId;
    match (p.borrow, &p.ty) {
        (ParamBorrow::Own, ty) => render_type(ctx, ty),
        (ParamBorrow::RefStr, _) => "&str".to_string(),
        (ParamBorrow::RefSlice, Ty::Applied(TypeConstructorId::List, args)) if args.len() == 1 => {
            format!("&[{}]", render_type(ctx, &args[0]))
        }
        (ParamBorrow::Ref, Ty::Bytes) => "&[u8]".to_string(),
        (ParamBorrow::Ref, ty) if extern_rust_is_rc_cow(ty) => "&AlmideMatrix".to_string(),
        (ParamBorrow::Ref | ParamBorrow::RefSlice, ty) => format!("&{}", render_type(ctx, ty)),
        // `&mut AlmideRcCow<T>` deref-coerces through `make_mut`: the host
        // writes the caller's binding copy-on-write, as a `mut` param means.
        (ParamBorrow::RefMut, Ty::Bytes) => "&mut Vec<u8>".to_string(),
        (ParamBorrow::RefMut, ty) if extern_rust_is_rc_cow(ty) => "&mut AlmideMatrix".to_string(),
        (ParamBorrow::RefMut, ty) => format!("&mut {}", render_type(ctx, ty)),
    }
}

/// Render @extern(c, "lib", "func") as: extern "C" block + safe Almide wrapper.
///
/// Type mapping (Almide → C extern → safe wrapper):
///   Int     → i32 in extern, i64 in wrapper (cast)
///   Float   → f64 (same)
///   Bool    → i32 in extern, bool in wrapper (cast)
///   RawPtr  → *mut u8 (same)
fn render_extern_c(ctx: &RenderContext, func: &IrFunction, attr: &almide_lang::ast::ExternAttr, emit_name: &str) -> String {

    let lib = attr.module.as_str();
    let c_func = attr.function.as_str();
    let almide_name = emit_name;

    // Build C parameter list and Almide parameter list
    let mut c_params = Vec::new();
    let mut almide_params = Vec::new();
    let mut call_args = Vec::new();

    for p in &func.params {
        let name = p.name.as_str();
        let (c_ty, almide_ty, to_c) = extern_c_type_mapping(ctx, &p.ty, name);
        c_params.push(format!("{}: {}", name, c_ty));
        almide_params.push(format!("{}: {}", name, almide_ty));
        call_args.push(to_c);
    }

    let (c_ret, almide_ret, from_c) = extern_c_return_mapping(ctx, &func.ret_ty);

    let c_params_str = c_params.join(", ");
    let almide_params_str = almide_params.join(", ");
    let call_args_str = call_args.join(", ");

    format!(
        "#[link(name = \"{lib}\")]\nextern \"C\" {{ fn {c_func}({c_params_str}) -> {c_ret}; }}\n\
         pub fn {almide_name}({almide_params_str}) -> {almide_ret} {{ {from_c} }}",
        lib = lib,
        c_func = c_func,
        c_params_str = c_params_str,
        c_ret = c_ret,
        almide_name = almide_name,
        almide_params_str = almide_params_str,
        almide_ret = almide_ret,
        from_c = format!("unsafe {{ {} }}", wrap_return(&from_c, c_func, &call_args_str)),
    )
}

/// Map an Almide param type to (C type, Almide type, call expression).
fn extern_c_type_mapping(_ctx: &RenderContext, ty: &almide_lang::types::Ty, name: &str) -> (String, String, String) {
    use almide_lang::types::Ty;
    match ty {
        Ty::Int    => ("i32".into(), "i64".into(), format!("{} as i32", name)),
        Ty::Float  => ("f64".into(), "f64".into(), name.into()),
        Ty::Bool   => ("i32".into(), "bool".into(), format!("if {} {{ 1 }} else {{ 0 }}", name)),
        Ty::RawPtr => ("*mut u8".into(), "*mut u8".into(), name.into()),
        Ty::String => ("*const u8".into(), "String".into(), format!("{}.as_ptr()", name)),
        other      => {
            let s = format!("{:?}", other);
            (s.clone(), s.clone(), name.into())
        }
    }
}

/// Map an Almide return type to (C type, Almide type, conversion wrapper template).
fn extern_c_return_mapping(_ctx: &RenderContext, ty: &almide_lang::types::Ty) -> (String, String, String) {
    use almide_lang::types::Ty;
    match ty {
        Ty::Int    => ("i32".into(), "i64".into(), "as_i64".into()),
        Ty::Float  => ("f64".into(), "f64".into(), "direct".into()),
        Ty::Bool   => ("i32".into(), "bool".into(), "ne_zero".into()),
        Ty::RawPtr => ("*mut u8".into(), "*mut u8".into(), "direct".into()),
        Ty::Unit   => ("()".into(), "()".into(), "direct".into()),
        _other     => ("i32".into(), "i64".into(), "as_i64".into()),
    }
}

fn wrap_return(mode: &str, c_func: &str, call_args: &str) -> String {
    match mode {
        "as_i64"  => format!("{}({}) as i64", c_func, call_args),
        "ne_zero" => format!("{}({}) != 0", c_func, call_args),
        _         => format!("{}({})", c_func, call_args),
    }
}

/// Render @export(c, "symbol") — emits normal Almide fn + thin extern "C" wrapper.
///
/// ```text
/// pub fn my_add(a: i64, b: i64) -> i64 { (a + b) }
///
/// #[export_name = "my_add"]
/// pub extern "C" fn __c_my_add(a: i32, b: i32) -> i32 {
///     my_add(a as i64, b as i64) as i32
/// }
/// ```
fn render_export_c(ctx: &RenderContext, func: &IrFunction, attr: &almide_lang::ast::ExportAttr) -> String {
    use almide_lang::types::Ty;

    let symbol = attr.symbol.as_str();
    let fn_name = func.name.as_str();

    // 1. Render the normal Almide function (strip export_attrs to avoid recursion)
    let mut clean_func = func.clone();
    clean_func.export_attrs.clear();
    let almide_fn = render_function(ctx, &clean_func);

    // 2. Build C wrapper
    let mut c_params = Vec::new();
    let mut call_args = Vec::new();

    for p in &func.params {
        let name = p.name.as_str();
        match &p.ty {
            Ty::Int    => { c_params.push(format!("{}: i32", name)); call_args.push(format!("{} as i64", name)); }
            Ty::Float  => { c_params.push(format!("{}: f64", name)); call_args.push(name.into()); }
            Ty::Bool   => { c_params.push(format!("{}: i32", name)); call_args.push(format!("{} != 0", name)); }
            Ty::RawPtr => { c_params.push(format!("{}: *mut u8", name)); call_args.push(name.into()); }
            _          => { let t = render_type_fn(ctx, &p.ty); c_params.push(format!("{}: {}", name, t)); call_args.push(name.into()); }
        }
    }

    let (c_ret, wrap_open, wrap_close) = match &func.ret_ty {
        Ty::Int    => ("i32", "(", ") as i32"),
        Ty::Bool   => ("i32", "if ", " { 1 } else { 0 }"),
        Ty::RawPtr => ("*mut u8", "", ""),
        Ty::Float  => ("f64", "", ""),
        Ty::Unit   => ("()", "", ""),
        _          => ("i32", "(", ") as i32"),
    };

    let c_params_str = c_params.join(", ");
    let call_args_str = call_args.join(", ");

    let wrapper = format!(
        "#[export_name = \"{symbol}\"]\npub extern \"C\" fn __c_{fn_name}({c_params_str}) -> {c_ret} {{ {wo}{fn_name}({args}){wc} }}",
        symbol = symbol, fn_name = fn_name,
        c_params_str = c_params_str, c_ret = c_ret,
        wo = wrap_open, args = call_args_str, wc = wrap_close,
    );

    format!("{}\n\n{}", almide_fn, wrapper)
}
