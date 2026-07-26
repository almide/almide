// ── C FFI extern codegen ──────────────────────────────────────

/// Render @extern(c, "lib", "func") as: extern "C" block + safe Almide wrapper.
///
/// Type mapping (Almide → C extern → safe wrapper):
/// Render @native("target", "module", "function") — delegates to module::function().
/// Parameters use reference types (&str, &[T]) matching native Rust conventions.
fn render_native_call(ctx: &RenderContext, func: &IrFunction, attr: &almide_lang::ast::ExternAttr, emit_name: &str) -> String {
    use types::render_type;
    use almide_lang::types::{Ty, TypeConstructorId};
    let mod_name = attr.module.as_str();
    let fn_name = attr.function.as_str();

    // Wrapper params: use reference types for String/List to match native Rust conventions
    let params: Vec<String> = func.params.iter().map(|p| {
        let ty = match &p.ty {
            Ty::String => "&str".to_string(),
            Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
                format!("&[{}]", render_type(ctx, &args[0]))
            }
            _ => render_type(ctx, &p.ty),
        };
        format!("{}: {}", p.name, ty)
    }).collect();

    // Call args: pass through directly (wrapper already uses reference types)
    let args: Vec<String> = func.params.iter().map(|p| {
        p.name.to_string()
    }).collect();

    let ret = render_type(ctx, &func.ret_ty);
    format!("fn {}({}) -> {} {{\n    {}::{}({})\n}}",
        emit_name, params.join(", "), ret,
        mod_name, fn_name, args.join(", "))
}

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
