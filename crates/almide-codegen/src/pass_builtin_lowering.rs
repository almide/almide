//! BuiltinLoweringPass: transform special function calls into codegen-specific IR nodes.
//!
//! Converts Named calls to RustMacro, prefixed runtime calls, etc.
//! After this pass, the walker has zero special-case function handling.
//!
//! Transformations:
//! - assert_eq(a, b) → RustMacro { "assert_eq", [a, b] }
//! - assert_ne(a, b) → RustMacro { "assert_ne", [a, b] }
//! - assert_some(x) → RustMacro { "assert", [x.is_some()] }
//! - println(x) → RustMacro { "println", ["{}", x] }
//! - __encode_list_T / __decode_list_T → appropriate runtime call
//! - Type.method(x) → Named { "Type_method" }
//! - Method { "encode"/"decode" } → Named { "Type_encode"/"Type_decode" }
//!
//! NOTE: stdlib intrinsic dispatch (e.g. `value.as_float(v)` →
//! `almide_rt_value_as_float`) is the responsibility of the
//! `@intrinsic`-driven `IntrinsicLoweringPass`. This pass MUST NOT
//! rewrite calls based purely on a name prefix like `value_*`,
//! because user-defined functions can legitimately use such names
//! (`fn value_to_float(...)`) and the prefix carries no information
//! about whether the call resolves to a real runtime symbol.

use almide_ir::*;
use almide_lang::types::Ty;
use almide_base::Span;
use almide_base::intern::Sym;
use super::pass::{NanoPass, PassResult, Target};
use std::collections::HashMap;
use std::cell::RefCell;

thread_local! {
    /// Maps the full IR name of a module-defined function (a derived method like
    /// `Color.encode`) to its module prefix (`colors`). A cross-module reference
    /// reaches this pass as a bare `CallTarget::Named { "Color.encode" }` (the field
    /// type carries no module), and flattening it to `Color_encode` would dangle —
    /// the definition is `almide_rt_colors_Color_encode` (module_origin). So when a
    /// dotted Named method call resolves through this map it is emitted with the
    /// matching module prefix (#411-B). In-module calls are already prefixed by the
    /// caller's module before this pass, so they are not keyed here.
    static MODULE_METHOD_FNS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    /// `T.decode` fns whose `Value` param is passed BY REFERENCE after borrow
    /// inference (#1679) — keyed like `MODULE_METHOD_FNS`. Every derived
    /// decode borrows; a user-written `T.decode` does whatever its body
    /// needs. The list/option codec reroutes below pick the runtime driver
    /// whose `Fn` bound matches the per-element FnRef they hand it.
    static DECODE_BY_REF: RefCell<std::collections::HashSet<String>> = RefCell::new(std::collections::HashSet::new());
}

/// Collect the `T.decode` fns that borrow their `Value` input, under every
/// spelling a reroute can look up: the IR name, the module-bare name, and the
/// trailing `Type.method` (the same three keys `collect_module_method_fns` uses).
fn collect_decode_by_ref(program: &IrProgram) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    let mut add = |f: &IrFunction, origin: Option<&str>| {
        let by_ref = f.name.ends_with(".decode")
            && f.params.first().map_or(false, |p| matches!(p.borrow, ParamBorrow::Ref)
                && matches!(&p.ty, Ty::Named(n, _) if n.as_str() == "Value"));
        if !by_ref { return; }
        let name = f.name.as_str();
        set.insert(name.to_string());
        if let Some(bare) = origin.and_then(|o| name.strip_prefix(&format!("{}.", o))) {
            set.insert(bare.to_string());
        }
        let segs: Vec<&str> = name.split('.').collect();
        if segs.len() > 2 {
            set.insert(format!("{}.{}", segs[segs.len() - 2], segs[segs.len() - 1]));
        }
    };
    for f in &program.functions {
        add(f, f.module_origin.as_deref());
    }
    for m in &program.modules {
        let ident = m.versioned_name
            .map(|v| v.to_string().replace('.', "_"))
            .unwrap_or_else(|| m.name.to_string().replace('.', "_"));
        for f in &m.functions {
            add(f, Some(&ident));
        }
    }
    set
}

/// The `Value` argument of a rerouted codec driver, normalized to what the
/// chosen runtime fn takes. `BorrowInsertion` already wrapped it in a `Borrow`
/// for the derived worker's `&Value` param; the by-value driver wants it bare.
fn codec_value_arg(arg: IrExpr, by_ref: bool) -> IrExpr {
    let bare = match arg.kind {
        IrExprKind::Borrow { expr, as_str: false, mutable: false } => *expr,
        kind => IrExpr { kind, ..arg },
    };
    if by_ref {
        let ty = bare.ty.clone();
        IrExpr { kind: IrExprKind::Borrow { expr: Box::new(bare), as_str: false, mutable: false }, ty, span: None, def_id: None }
    } else {
        bare
    }
}

/// Collect every module-defined dotted function (`Color.encode`) → its module
/// prefix, from whichever side of `IrLinkFlattenPass` we run on: merged root
/// functions carry `module_origin`; not-yet-merged ones live under `program.modules`.
fn collect_module_method_fns(program: &IrProgram) -> HashMap<String, String> {
    let mut map = HashMap::new();
    // Also key by the bare-type method name: a convention/Codec method fn whose
    // type is now the namespaced `mod.Type` is named `mod.Type.method`, but a
    // caller writing the unqualified `Type.method` must still resolve to the same
    // `almide_rt_<origin>_Type_method` definition (#433 × #411-B).
    // Values are the EMITTED SYMBOL, not the origin: the definition builds
    // `almide_rt_{origin}_{base}` where `base` is the flattened IR name with a
    // leading `{origin}_` stripped when it happens to match. For a DEPENDENCY it
    // does not match — the fn is `codeclib.inner.Pigment.encode` while the
    // origin is the versioned `codeclib_v0_inner` — so every call site that
    // rebuilt the name from its own guess disagreed with the definition
    // (#1094). Storing what the definition emits means no consumer has to guess.
    let add = |map: &mut HashMap<String, String>, name: &str, origin: &str| {
        if !name.contains('.') {
            return;
        }
        let flat = name.replace('.', "_");
        let base = flat.strip_prefix(&format!("{}_", origin)).unwrap_or(&flat);
        let symbol = format!("almide_rt_{}_{}", origin, base);
        map.insert(name.to_string(), symbol.clone());
        if let Some(bare) = name.strip_prefix(&format!("{}.", origin)) {
            map.insert(bare.to_string(), symbol.clone());
        }
        // The trailing `Type.method` is what a caller inside the owning module
        // emits, and it is the only spelling that survives when the module
        // segments differ from the origin's.
        let segs: Vec<&str> = name.split('.').collect();
        if segs.len() > 2 {
            let tail = format!("{}.{}", segs[segs.len() - 2], segs[segs.len() - 1]);
            map.entry(tail).or_insert(symbol);
        }
    };
    for f in &program.functions {
        if let Some(origin) = &f.module_origin {
            add(&mut map, f.name.as_str(), origin);
        }
    }
    for m in &program.modules {
        let ident = m.versioned_name
            .map(|v| v.to_string().replace('.', "_"))
            .unwrap_or_else(|| m.name.to_string().replace('.', "_"));
        for f in &m.functions {
            add(&mut map, f.name.as_str(), &ident);
        }
    }
    map
}

#[derive(Debug)]
pub struct BuiltinLoweringPass;

impl NanoPass for BuiltinLoweringPass {
    fn name(&self) -> &str { "BuiltinLowering" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["ResultPropagation"] }
    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let method_fns = collect_module_method_fns(&program);
        MODULE_METHOD_FNS.with(|c| *c.borrow_mut() = method_fns);
        let by_ref = collect_decode_by_ref(&program);
        DECODE_BY_REF.with(|c| *c.borrow_mut() = by_ref);
        for func in &mut program.functions {
            func.body = rewrite_expr(std::mem::take(&mut func.body));
        }
        for tl in &mut program.top_lets {
            tl.value = rewrite_expr(std::mem::take(&mut tl.value));
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                func.body = rewrite_expr(std::mem::take(&mut func.body));
            }
            for tl in &mut module.top_lets {
                tl.value = rewrite_expr(std::mem::take(&mut tl.value));
            }
        }
        PassResult { program, changed: true }
    }
}

/// `IrExprKind::Call { target: CallTarget::Named { name }, .. }` handling,
/// extracted verbatim from `rewrite_expr`'s `Call` arm (cog>100
/// decomposition, pattern 2). Every early `return` here used to return from
/// `rewrite_expr` directly; now it returns from this helper instead — same
/// value, same short-circuit, just delegated construction. `name: Sym` is
/// `Copy`, so passing it by value (rather than the original `ref name`
/// borrow out of `target`) changes nothing observable.
/// `__encode_list_T` / `__decode_list_T` handling, extracted verbatim from
/// `rewrite_call_named`'s largest `if` block (pattern 2, same rationale).
fn rewrite_call_list_codec(name: Sym, args: Vec<IrExpr>, type_args: Vec<Ty>, ty: Ty, span: Option<Span>) -> IrExpr {
    let type_name = if name.starts_with("__encode_list_") {
        &name["__encode_list_".len()..]
    } else {
        &name["__decode_list_".len()..]
    };
    let primitives = ["string", "int", "float", "bool"];
    if primitives.contains(&type_name) {
        // The primitive list DECODERS borrow their input (#1679) — the
        // runtime `almide_rt___decode_list_<prim>(v: &Value)`; the encoders
        // still consume their Vec. The call reaches this pass under its
        // stdlib name, which BorrowInsertion has no signature for, so the
        // borrow is placed here, where the runtime target is chosen.
        let mut args = args;
        if name.starts_with("__decode_list_") {
            if let Some(v) = args.pop() { args.push(codec_value_arg(v, true)); }
        }
        IrExpr { kind: IrExprKind::Call {
            target: CallTarget::Named { name: format!("almide_rt_{}", name).into() },
            args, type_args,
        }, ty, span, def_id: None }
    } else {
        // Custom type: use generic encode/decode. A module-defined
        // element type carries its module prefix so the per-element
        // codec FnRef matches its definition (#411-B, the `List`
        // element case of the same cross-module fix below).
        let is_encode = name.starts_with("__encode");
        let codec_op = if is_encode { "encode" } else { "decode" };
        let codec_method = format!("{}.{}", type_name, codec_op);
        let func_ref = MODULE_METHOD_FNS
            .with(|c| c.borrow().get(&codec_method).cloned())
            .unwrap_or_else(|| format!("{}_{}", type_name, codec_op));
        // The per-element codec function reference has a
        // precise signature — leaving it `Ty::Unknown` here
        // is exactly the latent unresolved-type that the
        // codegen-entry completeness gate now rejects, and
        // the Unknown would otherwise pick an arbitrary repr.
        //   encode: Item.encode : (Item) -> Value
        //   decode: Item.decode : (Value) -> Result[Item, String]
        // (`Value` is the codec intermediate, `Ty::Named("Value")`.)
        let elem_ty = Ty::Named(type_name.into(), vec![]);
        let value_ty = Ty::Named("Value".into(), vec![]);
        use almide_lang::types::constructor::TypeConstructorId;
        let fn_ref_ty = if is_encode {
            Ty::Fn { is_effect: false, params: vec![elem_ty], ret: Box::new(value_ty) }
        } else {
            Ty::Fn { is_effect: false, 
                params: vec![value_ty],
                ret: Box::new(Ty::Applied(
                    TypeConstructorId::Result,
                    vec![elem_ty, Ty::String],
                )),
            }
        };
        let by_ref = !is_encode && DECODE_BY_REF.with(|c| c.borrow().contains(&codec_method));
        let mut new_args = args;
        if !is_encode {
            if let Some(v) = new_args.pop() { new_args.push(codec_value_arg(v, by_ref)); }
        }
        new_args.push(IrExpr {
            kind: IrExprKind::FnRef { name: func_ref.into() },
            ty: fn_ref_ty,
            span: None, def_id: None,
        });
        let rt_func = if is_encode {
            "almide_rt_value_encode_list"
        } else if by_ref {
            "almide_rt_value_decode_list_ref"
        } else {
            "almide_rt_value_decode_list"
        };
        IrExpr { kind: IrExprKind::Call {
            target: CallTarget::Named { name: rt_func.into() },
            args: new_args, type_args,
        }, ty, span, def_id: None }
    }
}

/// `__encode_option_T` / `__decode_option_T` handling for a CUSTOM element
/// type, extracted verbatim from `rewrite_call_named`. Only called once the
/// caller has confirmed `type_name` is non-primitive — mirrors the original
/// `if !primitives.contains(&type_name) { ... }` guard, just with the cheap
/// primitive check left in the caller so this helper can unconditionally
/// return (no `Option`-shaped "fall through" signal needed).
fn rewrite_call_option_codec(name: Sym, type_name: String, args: Vec<IrExpr>, type_args: Vec<Ty>, ty: Ty, span: Option<Span>) -> IrExpr {
    let type_name = type_name.as_str();
    let is_encode = name.starts_with("__encode");
    let codec_op = if is_encode { "encode" } else { "decode" };
    let codec_method = format!("{}.{}", type_name, codec_op);
    let func_ref = MODULE_METHOD_FNS
        .with(|c| c.borrow().get(&codec_method).cloned())
        .unwrap_or_else(|| format!("{}_{}", type_name, codec_op));
    let elem_ty = Ty::Named(type_name.into(), vec![]);
    let value_ty = Ty::Named("Value".into(), vec![]);
    use almide_lang::types::constructor::TypeConstructorId;
    let fn_ref_ty = if is_encode {
        Ty::Fn { is_effect: false, params: vec![elem_ty], ret: Box::new(value_ty) }
    } else {
        Ty::Fn { is_effect: false, 
            params: vec![value_ty],
            ret: Box::new(Ty::Applied(TypeConstructorId::Result, vec![elem_ty, Ty::String])),
        }
    };
    let by_ref = !is_encode && DECODE_BY_REF.with(|c| c.borrow().contains(&codec_method));
    let mut new_args = args;
    if !is_encode && !new_args.is_empty() {
        let v = new_args.remove(0);
        new_args.insert(0, codec_value_arg(v, by_ref));
    }
    new_args.push(IrExpr {
        kind: IrExprKind::FnRef { name: func_ref.into() },
        ty: fn_ref_ty, span: None, def_id: None,
    });
    let rt_func = if is_encode {
        "almide_rt_value_option_encode"
    } else if by_ref {
        "almide_rt_value_decode_option_custom_ref"
    } else {
        "almide_rt_value_decode_option_custom"
    };
    IrExpr { kind: IrExprKind::Call {
        target: CallTarget::Named { name: rt_func.into() },
        args: new_args, type_args,
    }, ty, span, def_id: None }
}

fn rewrite_call_named(name: Sym, args: Vec<IrExpr>, type_args: Vec<Ty>, ty: Ty, span: Option<Span>) -> IrExpr {
    // Two disjoint families: the builtins that become a Rust macro, and the
    // generated / dotted names that become a renamed free-fn call.
    if matches!(
        name.as_str(),
        "assert" | "assert_eq" | "assert_ne" | "assert_some" | "panic" | "println" | "eprintln"
    ) {
        return rewrite_call_as_macro(name, args, ty, span);
    }
    rewrite_call_rename(name, args, type_args, ty, span)
}

/// A `LitStr` macro argument.
fn macro_lit(value: &str) -> IrExpr {
    IrExpr { kind: IrExprKind::LitStr { value: value.into() }, ty: Ty::String, span: None, def_id: None }
}

/// `at line <N>` — the assertion's own `.almd` source line, threaded into the
/// Rust assertion macro's message so a TEST-block failure names the line in the
/// source instead of only the generated `main.rs` one (which is what libtest's
/// panic banner reports, and which no agent can act on). `almide test` reads it
/// back out of the panic payload; see `src/cli/test_report.rs`.
///
/// Non-test asserts never reach here — the frontend desugars them to the T18
/// abort form (C-153), which carries its own `at:` line.
fn assert_site(span: &Option<Span>) -> Option<String> {
    span.as_ref().map(|s| format!("at line {}", s.line))
}

/// The builtins with no Rust fn behind them: they lower to a macro invocation.
fn rewrite_call_as_macro(name: Sym, args: Vec<IrExpr>, ty: Ty, span: Option<Span>) -> IrExpr {
    // assert / assert_eq / assert_ne → RustMacro
    if name == "assert" || name == "assert_eq" || name == "assert_ne" {
        let site = assert_site(&span);
        // assert(cond, msg) → assert!(cond, "{}", msg)
        // Rust's assert! macro requires a format string literal as second arg
        if name == "assert" && args.len() == 2 {
            let cond = args[0].clone();
            let msg = args[1].clone();
            let fmt = match &site {
                Some(at) => macro_lit(&format!("{{}} ({at})")),
                None => macro_lit("{}"),
            };
            return IrExpr { kind: IrExprKind::RustMacro { name, args: vec![cond, fmt, msg] }, ty, span, def_id: None };
        }
        // Sized Numeric Types (Stage 1c): `assert_eq(x,
        // 30)` where `x: Int32` needs the `30` literal
        // retyped to `Int32` so `rustc`'s `assert_eq!`
        // macro sees matching operand widths. The
        // assertion itself isn't a typed fn call, so
        // the usual arg-coercion in `lower_call` doesn't
        // reach here — patch at the macro build site.
        let mut args = args;
        if args.len() == 2 {
            let l_ty = args[0].ty.clone();
            let r_ty = args[1].ty.clone();
            coerce_macro_arg(&mut args[1], &l_ty);
            coerce_macro_arg(&mut args[0], &r_ty);
        }
        // Append the source site as the macro's message — `assert_eq!(l, r,
        // "{}", "at line 12")` prints `assertion `left == right` failed: at
        // line 12` above libtest's `left:`/`right:` pair.
        if let Some(at) = site {
            args.push(macro_lit("{}"));
            args.push(macro_lit(&at));
        }
        return IrExpr { kind: IrExprKind::RustMacro { name, args }, ty, span, def_id: None };
    }
    // assert_some → assert!(x.is_some())
    if name == "assert_some" {
        // Just use RustMacro with "assert" and transform in walker
        return IrExpr { kind: IrExprKind::RustMacro {
            name: "assert".into(),
            args: vec![IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Method {
                        object: Box::new(args.into_iter().next().unwrap_or(IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None })),
                        method: "is_some".into(),
                    },
                    args: vec![],
                    type_args: vec![],
                },
                ty: Ty::Bool, span: None, def_id: None,
            }],
        }, ty, span, def_id: None };
    }
    // panic → RustMacro
    if name == "panic" {
        let mut macro_args = vec![IrExpr { kind: IrExprKind::LitStr { value: "{}".into() }, ty: Ty::String, span: None, def_id: None }];
        macro_args.extend(args);
        return IrExpr { kind: IrExprKind::RustMacro { name: "panic".into(), args: macro_args }, ty, span, def_id: None };
    }
    // println / eprintln → RustMacro
    if name == "println" || name == "eprintln" {
        let mut macro_args = vec![IrExpr { kind: IrExprKind::LitStr { value: "{}".into() }, ty: Ty::String, span: None, def_id: None }];
        macro_args.extend(args);
        return IrExpr { kind: IrExprKind::RustMacro { name, args: macro_args }, ty, span, def_id: None };
    }
    unreachable!("rewrite_call_as_macro reached with a non-macro builtin: {}", name)
}

/// The generated codec helpers and the dotted `Type.method` spellings: same
/// call, renamed target.
fn rewrite_call_rename(name: Sym, args: Vec<IrExpr>, type_args: Vec<Ty>, ty: Ty, span: Option<Span>) -> IrExpr {
    // __encode_list_T / __decode_list_T
    if name.starts_with("__encode_list_") || name.starts_with("__decode_list_") {
        return rewrite_call_list_codec(name, args, type_args, ty, span);
    }
    // __encode_option_T / __decode_option_T for a CUSTOM element
    // type: route through the generic option codec with a per-element
    // `T.encode`/`T.decode` fn. Primitives keep their existing
    // `almide_rt___{op}_option_<prim>` helper via the `__` arm below (新②).
    if name.starts_with("__encode_option_") || name.starts_with("__decode_option_") {
        let type_name = if name.starts_with("__encode_option_") {
            &name["__encode_option_".len()..]
        } else {
            &name["__decode_option_".len()..]
        };
        let primitives = ["string", "int", "float", "bool"];
        if !primitives.contains(&type_name) {
            return rewrite_call_option_codec(name, type_name.to_string(), args, type_args, ty, span);
        }
    }
    // Remaining generated codec helpers (primitive `__encode_option_*` /
    // `__decode_option_*`, `__decode_default_*`) → their `almide_rt___`
    // prelude definitions. Only the generated `__encode_`/`__decode_`
    // families are rewritten: a USER fn merely named with a `__` prefix must
    // link like any other user fn — the old blanket `__` rewrite renamed its
    // call sites but never its definition, so it failed with E0425 (#868).
    if name.starts_with("__encode_") || name.starts_with("__decode_") {
        return IrExpr { kind: IrExprKind::Call {
            target: CallTarget::Named { name: format!("almide_rt_{}", name).into() },
            args, type_args,
        }, ty, span, def_id: None };
    }
    // Type.method → Type_method. If the method belongs to a
    // module-defined type, carry the module prefix so the call
    // matches its `almide_rt_<module>_Type_method` definition (#411-B).
    if name.contains('.') {
        let flat = name.replace('.', "_");
        let resolved = MODULE_METHOD_FNS
            .with(|c| c.borrow().get(name.as_str()).cloned())
            .unwrap_or(flat);
        return IrExpr { kind: IrExprKind::Call {
            target: CallTarget::Named { name: resolved.into() },
            args, type_args,
        }, ty, span, def_id: None };
    }

    IrExpr { kind: IrExprKind::Call { target: CallTarget::Named { name }, args, type_args }, ty, span, def_id: None }
}

/// `IrExprKind::Call { target: CallTarget::Method { .. }, .. }` handling,
/// extracted verbatim from `rewrite_expr`'s `Call` arm (same decomposition
/// as `rewrite_call_named`, same reasoning).
fn rewrite_call_method(object: Box<IrExpr>, method: Sym, args: Vec<IrExpr>, type_args: Vec<Ty>, ty: Ty, span: Option<Span>) -> IrExpr {
    let object = Box::new(rewrite_expr(*object));

    // encode/decode methods → Type_encode/Type_decode standalone calls
    if method == "encode" || method == "decode"
        || method.ends_with(".encode") || method.ends_with(".decode")
    {
        let flat_method = method.replace('.', "_");
        let call_name: String = if method.contains('.') {
            flat_method
        } else {
            let type_name = match &object.ty {
                Ty::Named(n, _) => n.to_string(),
                Ty::Variant { name, .. } => name.to_string(),
                _ => "Unknown".to_string(),
            };
            format!("{}_{}", type_name, method)
        };
        let mut call_args = vec![*object];
        call_args.extend(args);
        return IrExpr { kind: IrExprKind::Call {
            target: CallTarget::Named { name: call_name.into() },
            args: call_args, type_args,
        }, ty, span, def_id: None };
    }

    // Other Type.method patterns → Type_method standalone calls
    if method.contains('.') {
        // Bundled-stdlib modules (lowercase heads like
        // `uint32.to_int64`) carry the `almide_rt_` prefix
        // at their definition site (see `walker/mod.rs`
        // rename of `fn <clean_name>` → `fn almide_rt_<m>_<clean>`).
        // Mirror that prefix at the call site so UFCS
        // dispatch resolves to the emitted symbol.
        // Convention methods (uppercase head — `List.encode`)
        // use the `Type_method` flat naming and stay as-is.
        let dot_pos = method.find('.').unwrap();
        let module_head = &method.as_str()[..dot_pos];
        let is_bundled = almide_lang::stdlib_info::is_any_stdlib(module_head);
        let flat = method.replace('.', "_");
        let name = if is_bundled {
            format!("almide_rt_{}", flat)
        } else {
            flat
        };
        let mut call_args = vec![*object];
        call_args.extend(args);
        return IrExpr { kind: IrExprKind::Call {
            target: CallTarget::Named { name: name.into() },
            args: call_args, type_args,
        }, ty, span, def_id: None };
    }

    IrExpr { kind: IrExprKind::Call {
        target: CallTarget::Method { object, method },
        args, type_args,
    }, ty, span, def_id: None }
}

/// Lower every builtin call in the tree. Only `Call` needs a rule of its own:
/// every other node just needs its children rewritten, which `map_children`
/// does exhaustively (it lists every `IrExprKind`, so a lowerable call nested
/// inside an un-listed or future kind is still reached — this used to be a
/// silent `other => other` drop; see
/// docs/roadmap/active/codegen-traversal-totality.md). Statement bodies ride
/// along through `IrStmt::map_exprs`.
fn rewrite_expr(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    match expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            let args: Vec<IrExpr> = args.into_iter().map(rewrite_expr).collect();
            match target {
                CallTarget::Named { name } => rewrite_call_named(name, args, type_args, ty, span),
                CallTarget::Method { object, method } => {
                    rewrite_call_method(object, method, args, type_args, ty, span)
                }
                target => IrExpr {
                    kind: IrExprKind::Call { target, args, type_args },
                    ty,
                    span,
                    def_id: None,
                },
            }
        }
        kind => IrExpr { kind, ty, span, def_id: None }
            .map_children(&mut |e| rewrite_expr(e)),
    }
}

/// Retype a bare Int / Float literal whose IR type is `Ty::Int` /
/// `Ty::Float` so it matches a sized-typed peer in the same macro
/// call. See the `assert_eq` site above for the motivation.
fn coerce_macro_arg(arg: &mut IrExpr, peer_ty: &Ty) {
    let sized = matches!(
        peer_ty,
        Ty::Int8 | Ty::Int16 | Ty::Int32
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
            | Ty::Float32
    );
    if !sized { return; }
    match &mut arg.kind {
        IrExprKind::LitInt { .. } if arg.ty == Ty::Int => {
            arg.ty = peer_ty.clone();
        }
        IrExprKind::LitFloat { .. } if arg.ty == Ty::Float => {
            arg.ty = peer_ty.clone();
        }
        IrExprKind::UnOp { op: UnOp::NegInt, operand } => {
            if matches!(&operand.kind, IrExprKind::LitInt { .. }) && operand.ty == Ty::Int {
                operand.ty = peer_ty.clone();
                arg.ty = peer_ty.clone();
            }
        }
        _ => {}
    }
}

