// ── C FFI extern codegen ──────────────────────────────────────

/// Render `@extern(rust, "module", "function")` as a thin wrapper that
/// delegates to `module::function(..)`.
///
/// The wrapper's params follow the `@extern(rust)` ABI
/// (`extern_rust_borrow_mode`, #3045) through the param modes — the same modes
/// the call sites are decorated with — so a call site and the wrapper cannot
/// disagree. The host fn sees plain Rust types: `&str`, `&[T]`, `&[u8]`,
/// `&AlmideMatrix`, `&T` for records / maps / sets, `&mut T` for `mut`
/// params, `&dyn Fn(A) -> B` for fn values, and owned scalars / `Option` /
/// `Result` / tuples / variants. A `Bytes` / `Matrix` RETURN accepts either the raw
/// `Vec<u8>` / `AlmideMatrix` or the `AlmideRcCow` handle (`.into()`).
fn render_native_call(ctx: &RenderContext, func: &IrFunction, attr: &almide_lang::ast::ExternAttr, emit_name: &str) -> String {
    use types::render_type;
    let mod_name = attr.module.as_str();
    let fn_name = attr.function.as_str();
    let params: Vec<String> = func.params.iter()
        .map(|p| format!("{}: {}", p.name, extern_rust_param_type(ctx, p)))
        .collect();
    // ADR-0013: generated code holds the shared buffer `AlmideStr`, the host
    // speaks `String`. A value that holds a String in a structural position
    // (List / Set / Map / Option / Result / tuple) is copied into its host
    // shape on the way in, and a `mut` one is copied back after the call; a
    // returned one is copied into the Almide shape. A plain `String` param is
    // `&str` and needs nothing.
    let mut before = String::new();
    let mut after = String::new();
    let args: Vec<String> = func.params.iter().map(|p| extern_rust_arg(p, &mut before, &mut after)).collect();
    let ret = render_type(ctx, &func.ret_ty);
    let call = format!("{}::{}({})", mod_name, fn_name, args.join(", "));
    let result = if extern_rust_is_rc_cow(&func.ret_ty) {
        "__almide_r.into()".to_string()
    } else if extern_rust_has_str(&func.ret_ty) {
        extern_rust_str_conv("&__almide_r", &func.ret_ty, false, 0)
    } else {
        "__almide_r".to_string()
    };
    let body = if before.is_empty() && after.is_empty() && result == "__almide_r" {
        format!("    {call}")
    } else {
        format!("{before}    let __almide_r = {call};\n{after}    {result}")
    };
    format!("fn {}({}) -> {} {{\n{}\n}}", emit_name, params.join(", "), ret, body)
}

/// Whether a value of `ty` holds an Almide String the `@extern(rust)` host
/// sees as `String`: the String itself, or one inside a List / Set / Map /
/// Option / Result / tuple. Records and variants are the program's own types,
/// which the host uses as they are.
fn extern_rust_has_str(ty: &Ty) -> bool {
    use almide_lang::types::TypeConstructorId as C;
    match ty {
        Ty::String => true,
        Ty::Applied(C::List | C::Set | C::Option | C::Result | C::Map, args) => args.iter().any(extern_rust_has_str),
        Ty::Tuple(elems) => elems.iter().any(extern_rust_has_str),
        _ => false,
    }
}

/// An owned copy of the value `e` (an expression of type `&T`) in the host's
/// shape (`to_host`: every Almide String becomes a `String`) or in the Almide
/// shape (every host `String`, or an `AlmideString` a host already returns,
/// becomes an `AlmideString`). `depth` keeps closure binders distinct.
fn extern_rust_str_conv(e: &str, ty: &Ty, to_host: bool, depth: usize) -> String {
    use almide_lang::types::TypeConstructorId as C;
    if !extern_rust_has_str(ty) {
        return format!("({e}).clone()");
    }
    let x = format!("__x{depth}");
    let y = format!("__y{depth}");
    let inner = |e: &str, ty: &Ty| extern_rust_str_conv(e, ty, to_host, depth + 1);
    match ty {
        Ty::String if to_host => format!("({e}).to_string()"),
        Ty::String => format!("AlmideString::from(({e}).as_str())"),
        Ty::Applied(C::List, args) => format!("({e}).iter().map(|{x}| {}).collect::<Vec<_>>()", inner(&x, &args[0])),
        Ty::Applied(C::Set, args) => format!("({e}).iter().map(|{x}| {}).collect::<AlmideSet<_>>()", inner(&x, &args[0])),
        Ty::Applied(C::Option, args) => format!("({e}).as_ref().map(|{x}| {})", inner(&x, &args[0])),
        Ty::Applied(C::Result, args) => format!(
            "match {e} {{ Ok({x}) => Ok({}), Err({x}) => Err({}) }}",
            inner(&x, &args[0]),
            inner(&x, &args[1])
        ),
        Ty::Applied(C::Map, args) => format!(
            "({e}).iter().map(|({x}, {y})| ({}, {})).collect::<AlmideMap<_, _>>()",
            inner(&x, &args[0]),
            inner(&y, &args[1])
        ),
        Ty::Tuple(elems) => {
            let parts: Vec<String> = elems.iter().enumerate().map(|(i, t)| inner(&format!("&{x}.{i}"), t)).collect();
            format!("{{ let {x} = {e}; ({},) }}", parts.join(", "))
        }
        _ => format!("({e}).clone()"),
    }
}

/// `Bytes` / `Matrix`: the two value types generated code holds behind an
/// `AlmideRcCow` handle (#617).
fn extern_rust_is_rc_cow(ty: &Ty) -> bool {
    use almide_lang::types::TypeConstructorId;
    matches!(ty, Ty::Bytes | Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _))
}


/// Render `@extern(c, "lib", "func")` as an `extern "C"` block plus a safe
/// wrapper, from the one C ABI table (`almide_lang::types::extern_abi`, #3054):
///
/// ```text
/// #[link(name = "c")]
/// extern "C" { fn strlen(s: *const u8) -> i32; }
/// pub fn c_strlen(s: &str) -> i64 {
///     let __s_cstr = std::ffi::CString::new(..s up to its first NUL..).unwrap_or_default();
///     unsafe { strlen(__s_cstr.as_ptr() as *const u8) as i64 }
/// }
/// ```
///
/// The wrapper's params follow the borrow modes the call sites were given
/// (`extern_c_borrow_mode`: a `String` is `&str`, everything else by value).
/// A type with no C form never reaches here — `check` refuses it (E090) — so
/// a `None` row renders a `compile_error!` naming it rather than a guessed
/// type.
///
/// A `Region` (ADR-0031 D3) is two C arguments: its base and the length the
/// runtime holds for it, as `usize` — never a length the caller typed. A
/// `Region` RETURN (D2, D6) is the foreign pointer wrapped in a read-only
/// Region whose length is the declaration's `@region(len = …)` (0 when the
/// pointer is null), and whose `keep` closure owns a release guard: the last
/// copy of the Region dropping calls the `release = "…"` function once.
fn render_extern_c(ctx: &RenderContext, func: &IrFunction, attr: &almide_lang::ast::ExternAttr, emit_name: &str) -> String {
    use types::render_type;
    use almide_lang::types::extern_abi::{c_param_abi, c_return_abi, CConv};
    let lib = attr.module.as_str();
    let c_func = attr.function.as_str();
    let refuse = |what: String| format!(
        "compile_error!(\"@extern(c) fn `{}`: {what} has no C representation (E090)\");", func.name
    );

    let mut c_params = Vec::new();
    let mut wrapper_params = Vec::new();
    let mut prelude = String::new();
    let mut call_args = Vec::new();
    for p in &func.params {
        let name = p.name.as_str();
        let Some(abi) = c_param_abi(&p.ty) else {
            return refuse(format!("parameter `{name}: {}`", p.ty.display()));
        };
        if abi.conv == CConv::Region {
            c_params.push(format!("{name}: *mut u8, {name}_len: usize"));
            wrapper_params.push(format!("{name}: {}", render_type(ctx, &p.ty)));
            call_args.push(format!("{name}.base.wrapping_add({name}.off.max(0) as usize), {name}.len.max(0) as usize"));
            continue;
        }
        c_params.push(format!("{name}: {}", abi.c_ty));
        let (wrapper_ty, arg) = match abi.conv {
            CConv::Same => (render_type(ctx, &p.ty), name.to_string()),
            CConv::IntAsI32 => (render_type(ctx, &p.ty), format!("{name} as i32")),
            CConv::BoolAsI32 => (render_type(ctx, &p.ty), format!("if {name} {{ 1 }} else {{ 0 }}")),
            CConv::CString => {
                let c = format!("__{name}_cstr");
                prelude.push_str(&format!(
                    "    let {c} = std::ffi::CString::new({name}.split('\\0').next().unwrap_or_default()).unwrap_or_default();\n"
                ));
                ("&str".to_string(), format!("{c}.as_ptr() as *const u8"))
            }
            CConv::Region => unreachable!("a Region parameter is rendered above"),
        };
        wrapper_params.push(format!("{name}: {wrapper_ty}"));
        call_args.push(arg);
    }
    let Some(ret) = c_return_abi(&func.ret_ty) else {
        return refuse(format!("return type `{}`", func.ret_ty.display()));
    };
    let call = format!("{c_func}({})", call_args.join(", "));
    let mut extra_decls = String::new();
    let mut epilogue = String::new();
    let body = match ret.conv {
        CConv::IntAsI32 => format!("{call} as i64"),
        CConv::BoolAsI32 => format!("{call} != 0"),
        CConv::Same | CConv::CString => call,
        CConv::Region => {
            let Some((len, release)) = region_clause(func) else {
                return refuse("a Region return without `@region(len = …, release = \"…\")`".to_string());
            };
            extra_decls = format!(" fn {release}(p: *mut u8);");
            epilogue = render_foreign_region(&render_type(ctx, &func.ret_ty), &len, &release);
            call
        }
    };
    // libc and libm have no import library on Windows: their symbols live in
    // the C runtime std already links, and `link.exe` refuses `m.lib` (LNK1181).
    let link = match lib {
        "c" | "m" => format!("#[cfg_attr(not(windows), link(name = \"{lib}\"))]"),
        _ => format!("#[link(name = \"{lib}\")]"),
    };
    // A Region return binds the pointer first and builds the Region after.
    let (bind, end) = if epilogue.is_empty() { ("", "") } else { ("let __almide_p: *mut u8 = ", ";") };
    let ret_value = format!("    {bind}unsafe {{ {body} }}{end}\n{epilogue}");
    format!(
        "{link}\nextern \"C\" {{ fn {c_func}({}) -> {};{extra_decls} }}\n\
         pub fn {emit_name}({}) -> {} {{\n{prelude}{ret_value}}}",
        c_params.join(", "), ret.c_ty, wrapper_params.join(", "), render_type(ctx, &func.ret_ty),
    )
}

/// The `@region(len = …, release = "…")` clause of a foreign Region return
/// (ADR-0031 D2): the length as a Rust `i64` expression over the wrapper's
/// params (a parameter name or an integer literal) and the release function's
/// C name. `check` validated both (E090), so a malformed clause is `None`.
fn region_clause(func: &IrFunction) -> Option<(String, String)> {
    use almide_lang::ast::AttrValue;
    let attr = func.attrs.iter().find(|a| a.name.as_str() == almide_lang::stdlib_info::REGION_ATTR)?;
    let arg = |key: &str| attr.args.iter().find(|a| a.name.is_some_and(|n| n.as_str() == key)).map(|a| &a.value);
    let len = match arg("len")? {
        AttrValue::Int { value } => format!("{value}i64"),
        AttrValue::Ident { name } => {
            func.params.iter().find(|p| p.name == *name)?;
            format!("({} as i64)", name.as_str())
        }
        _ => return None,
    };
    let release = match arg("release")? {
        AttrValue::String { value } => value.clone(),
        _ => return None,
    };
    Some((len, release))
}

/// The foreign Region a `@region` wrapper returns around `__almide_p`: no
/// owner Bytes (the memory is the foreign library's), the declared length (0
/// for a null pointer), read-only (ADR-0031 D4: writable regions come only
/// from `ffi.alloc`), and the release guard in `keep` (D6).
fn render_foreign_region(region_ty: &str, len: &str, release: &str) -> String {
    let n = format!("    let __almide_n: i64 = if __almide_p.is_null() {{ 0 }} else {{ {len}.max(0) }};\n");
    let release_call = format!("|__almide_q: *mut u8| unsafe {{ {release}(__almide_q) }}");
    let keep = format!("almide_rt_ffi_release_guard(__almide_p, {release_call})");
    format!(
        "{n}    {region_ty} {{ owner: Vec::<u8>::new().into(), base: __almide_p, off: 0, len: __almide_n, writable: false, keep: {keep} }}\n"
    )
}

/// Render @export(c, "symbol") — emits normal Almide fn + thin extern "C" wrapper.
///
/// ```text
/// pub fn my_add(a: i64, b: i64) -> i64 { (a + b) }
///
/// #[export_name = "my_add"]
/// pub extern "C" fn __c_my_add(a: i32, b: i32) -> i32 {
///     let __almide_ret = my_add(a as i64, b as i64);
///     (__almide_ret) as i32
/// }
/// ```
///
/// A `Region` parameter (ADR-0031 amendment A2) is two C arguments, the
/// caller's buffer and its `size_t` length: `const uint8_t*` and a read-only
/// Region, or — for a parameter the declaration names with
/// `@region(out = p)` — `uint8_t*` and a writable one. The Region is lent for
/// the call: it owns nothing (the memory is the caller's), every access is
/// still checked against the length the caller passed, and after the call the
/// wrapper stops the process if a copy of it is still held
/// (`almide_rt_ffi_lent_end`), so no Region outlives the buffer it views. An
/// `effect fn`'s error has no C form: it is printed and the process exits 1,
/// as an error escaping `main` does.
fn render_export_c(ctx: &RenderContext, func: &IrFunction, attr: &almide_lang::ast::ExportAttr) -> String {
    use almide_lang::types::Ty;
    use almide_lang::types::extern_abi::is_region;

    let symbol = attr.symbol.as_str();
    let fn_name = func.name.as_str();

    // 1. Render the normal Almide function (strip export_attrs to avoid recursion)
    let mut clean_func = func.clone();
    clean_func.export_attrs.clear();
    let almide_fn = render_function(ctx, &clean_func);

    // 2. Build C wrapper
    let outs = export_c_out_params(func);
    let mut c_params = Vec::new();
    let mut call_args = Vec::new();
    let mut lends = String::new();
    let mut ends = String::new();

    for p in &func.params {
        let name = p.name.as_str();
        if is_region(&p.ty) {
            let writable = outs.iter().any(|o| o == name);
            let ptr = if writable { "*mut u8" } else { "*const u8" };
            c_params.push(format!("{name}: {ptr}, {name}_len: usize"));
            let lent = format!("__almide_lent_{name}");
            lends.push_str(&format!("    let {lent} = {};\n", render_lent_region(&render_type(ctx, &p.ty), name, writable)));
            call_args.push(match p.borrow {
                ParamBorrow::Own => format!("{lent}.clone()"),
                _ => format!("&{lent}"),
            });
            ends.push_str(&format!("    almide_rt_ffi_lent_end(&{lent}.keep, \"{symbol}\", \"{name}\");\n"));
            continue;
        }
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
    let call = format!("{fn_name}({})", call_args.join(", "));
    let value = if func.is_effect {
        format!("match {call} {{ Ok(__almide_v) => __almide_v, Err(__almide_e) => {{ eprintln!(\"{{}}\", __almide_e); std::process::exit(1) }} }}")
    } else {
        call
    };

    let wrapper = format!(
        "#[export_name = \"{symbol}\"]\npub extern \"C\" fn __c_{fn_name}({c_params_str}) -> {c_ret} {{\n{lends}    let __almide_ret = {value};\n{ends}    {wrap_open}__almide_ret{wrap_close}\n}}",
    );

    format!("{}\n\n{}", almide_fn, wrapper)
}

/// The parameters an `@export(c)` declaration names as caller-provided
/// writable buffers: `@region(out = p)`, once per parameter (ADR-0031
/// amendment A2). `check` validated each names a Region parameter (E090).
fn export_c_out_params(func: &IrFunction) -> Vec<String> {
    use almide_lang::ast::AttrValue;
    func.attrs
        .iter()
        .filter(|a| a.name.as_str() == almide_lang::stdlib_info::REGION_ATTR)
        .flat_map(|a| a.args.iter())
        .filter(|a| a.name.is_some_and(|n| n.as_str() == "out"))
        .filter_map(|a| match &a.value {
            AttrValue::Ident { name } => Some(name.as_str().to_string()),
            _ => None,
        })
        .collect()
}

/// The Region an `@export(c)` wrapper lends its Almide fn over the C caller's
/// `(name, name_len)`: no owner Bytes (the memory is the caller's), the
/// caller's length (0 for a null pointer), writable only for an
/// `@region(out = …)` parameter, and a fresh `keep` the wrapper checks after
/// the call.
fn render_lent_region(region_ty: &str, name: &str, writable: bool) -> String {
    format!(
        "{region_ty} {{ owner: Vec::<u8>::new().into(), base: {name} as *mut u8, off: 0, \
         len: if {name}.is_null() {{ 0 }} else {{ i64::try_from({name}_len).unwrap_or(i64::MAX) }}, \
         writable: {writable}, keep: almide_rt_ffi_lent_keep() }}"
    )
}
