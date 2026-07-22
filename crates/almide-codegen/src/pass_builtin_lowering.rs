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
    let mut add = |map: &mut HashMap<String, String>, name: &str, origin: &str| {
        if name.contains('.') {
            map.insert(name.to_string(), origin.to_string());
            if let Some(bare) = name.strip_prefix(&format!("{}.", origin)) {
                map.insert(bare.to_string(), origin.to_string());
            }
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
        let func_ref = MODULE_METHOD_FNS.with(|c| {
            c.borrow().get(&codec_method)
                .map(|m| format!("almide_rt_{}_{}_{}", m, type_name.rsplit('.').next().unwrap_or(type_name), codec_op))
        }).unwrap_or_else(|| format!("{}_{}", type_name, codec_op));
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
            Ty::Fn { params: vec![elem_ty], ret: Box::new(value_ty) }
        } else {
            Ty::Fn {
                params: vec![value_ty],
                ret: Box::new(Ty::Applied(
                    TypeConstructorId::Result,
                    vec![elem_ty, Ty::String],
                )),
            }
        };
        let mut new_args = args;
        new_args.push(IrExpr {
            kind: IrExprKind::FnRef { name: func_ref.into() },
            ty: fn_ref_ty,
            span: None, def_id: None,
        });
        let rt_func = if name.starts_with("__encode") {
            "almide_rt_value_encode_list"
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
    let func_ref = MODULE_METHOD_FNS.with(|c| {
        c.borrow().get(&codec_method)
            .map(|m| format!("almide_rt_{}_{}_{}", m, type_name.rsplit('.').next().unwrap_or(type_name), codec_op))
    }).unwrap_or_else(|| format!("{}_{}", type_name, codec_op));
    let elem_ty = Ty::Named(type_name.into(), vec![]);
    let value_ty = Ty::Named("Value".into(), vec![]);
    use almide_lang::types::constructor::TypeConstructorId;
    let fn_ref_ty = if is_encode {
        Ty::Fn { params: vec![elem_ty], ret: Box::new(value_ty) }
    } else {
        Ty::Fn {
            params: vec![value_ty],
            ret: Box::new(Ty::Applied(TypeConstructorId::Result, vec![elem_ty, Ty::String])),
        }
    };
    let mut new_args = args;
    new_args.push(IrExpr {
        kind: IrExprKind::FnRef { name: func_ref.into() },
        ty: fn_ref_ty, span: None, def_id: None,
    });
    let rt_func = if is_encode {
        "almide_rt_value_option_encode"
    } else {
        "almide_rt_value_decode_option_custom"
    };
    IrExpr { kind: IrExprKind::Call {
        target: CallTarget::Named { name: rt_func.into() },
        args: new_args, type_args,
    }, ty, span, def_id: None }
}

fn rewrite_call_named(name: Sym, args: Vec<IrExpr>, type_args: Vec<Ty>, ty: Ty, span: Option<Span>) -> IrExpr {
    // assert / assert_eq / assert_ne → RustMacro
    if name == "assert" || name == "assert_eq" || name == "assert_ne" {
        // assert(cond, msg) → assert!(cond, "{}", msg)
        // Rust's assert! macro requires a format string literal as second arg
        if name == "assert" && args.len() == 2 {
            let cond = args[0].clone();
            let msg = args[1].clone();
            let fmt = IrExpr { kind: IrExprKind::LitStr { value: "{}".into() }, ty: Ty::String, span: None, def_id: None };
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
    // Other __ prefixed → almide_rt_
    if name.starts_with("__") {
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
        let resolved = MODULE_METHOD_FNS.with(|c| {
            c.borrow().get(name.as_str()).map(|m| {
                // The method key may be qualified by the type's now-namespaced
                // module (`varlib.Pigment.encode`); the runtime fn is
                // `almide_rt_<origin>_<Type>_<method>`, so strip a leading
                // `<origin>.` before flattening to avoid doubling the module
                // (#433 × #411-B). A bare `Color.encode` is unaffected.
                let rest = name.as_str().strip_prefix(&format!("{}.", m)).unwrap_or(name.as_str());
                format!("almide_rt_{}_{}", m, rest.replace('.', "_"))
            })
        }).unwrap_or(flat);
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

fn rewrite_expr(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            let args: Vec<IrExpr> = args.into_iter().map(rewrite_expr).collect();

            match target {
                CallTarget::Named { name } => return rewrite_call_named(name, args, type_args, ty, span),
                CallTarget::Method { object, method } => return rewrite_call_method(object, method, args, type_args, ty, span),
                _ => IrExprKind::Call { target, args, type_args },
            }
        }

        // Recurse into all sub-expressions
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_expr(*cond)),
            then: Box::new(rewrite_expr(*then)),
            else_: Box::new(rewrite_expr(*else_)),
        },
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: rewrite_stmts(stmts),
            expr: expr.map(|e| Box::new(rewrite_expr(*e))),
        },

        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rewrite_expr(*subject)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(rewrite_expr),
                body: rewrite_expr(arm.body),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(rewrite_expr(*left)), right: Box::new(rewrite_expr(*right)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(rewrite_expr(*operand)),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(rewrite_expr(*body)), lambda_id,
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(rewrite_expr).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v))).collect(),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rewrite_expr(*object)), field,
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(rewrite_expr(*expr)), field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple, iterable: Box::new(rewrite_expr(*iterable)),
            body: rewrite_stmts(body),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_expr(*cond)), body: rewrite_stmts(body),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: rewrite_expr(expr) },
                other => other,
            }).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(rewrite_expr).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(rewrite_expr(*base)),
            fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v))).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (rewrite_expr(k), rewrite_expr(v))).collect(),
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(rewrite_expr(*object)),
            index: Box::new(rewrite_expr(*index)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(rewrite_expr(*object)),
            key: Box::new(rewrite_expr(*key)),
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(rewrite_expr(*object)), index,
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(rewrite_expr(*start)),
            end: Box::new(rewrite_expr(*end)),
            inclusive,
        },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(rewrite_expr(*expr)),
            fallback: Box::new(rewrite_expr(*fallback)),
        },
        IrExprKind::Await { expr } => IrExprKind::Await { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(rewrite_expr).collect(),
        },
        // Recurse into iterator chains so lambdas inside fold / map / filter
        // get builtin-lowered (e.g. println → RustMacro).
        IrExprKind::IterChain { source, consume, steps, collector } => IrExprKind::IterChain {
            source: Box::new(rewrite_expr(*source)),
            consume,
            steps: steps.into_iter().map(|s| s.map_exprs(&mut rewrite_expr)).collect(),
            collector: collector.map_exprs(&mut rewrite_expr),
        },
        // Recurse into InlineRust args so `__`-prefixed runtime calls
        // nested inside them (e.g. `__encode_option_string` inside a
        // `value.object(pairs)` InlineRust produced by stdlib lowering)
        // are reached by the `__` prefix transformer.
        IrExprKind::InlineRust { template, args } => IrExprKind::InlineRust {
            template,
            args: args.into_iter().map(|(n, a)| (n, rewrite_expr(a))).collect(),
        },
        // Traverse RuntimeCall args so `panic(...)` / `assert_eq(...)` etc.
        // nested inside a `@intrinsic` fn (e.g. `assert_throws(|| panic(...), msg)`)
        // get lowered to their RustMacro form instead of staying as free fn calls.
        IrExprKind::RuntimeCall { symbol, args } => IrExprKind::RuntimeCall {
            symbol,
            args: args.into_iter().map(rewrite_expr).collect(),
        },
        // Recurse through ownership wrappers inserted by BorrowInsertion /
        // CloneInsertion so derive-generated `__encode_*` calls living
        // inside a `Borrow { List { Tuple { __encode_* } } }` spine still
        // get rewritten to `almide_rt_*`.
        IrExprKind::Borrow { expr, as_str, mutable } => IrExprKind::Borrow {
            expr: Box::new(rewrite_expr(*expr)), as_str, mutable,
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(rewrite_expr(*expr)) },
        // No builtin lowering applies to this node — recurse into its children
        // via the exhaustive `map_children` so a lowerable call nested inside an
        // un-listed / future kind is still reached (was a silent `other => other`
        // drop). See docs/roadmap/active/codegen-traversal-totality.md.
        other => {
            return IrExpr { kind: other, ty, span, def_id: None }
                .map_children(&mut |e| rewrite_expr(e));
        }
    };

    IrExpr { kind, ty, span, def_id: None }
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

fn rewrite_stmts(stmts: Vec<IrStmt>) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: rewrite_expr(value),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: rewrite_expr(value) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: rewrite_expr(expr) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: rewrite_expr(cond), else_: rewrite_expr(else_),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: rewrite_expr(value),
            },
            // Recurse the exprs of any other statement kind via `map_exprs`
            // (was a silent drop of e.g. IndexAssign/MapInsert/ListSwap exprs).
            other => return IrStmt { kind: other, span: s.span }.map_exprs(&mut |e| rewrite_expr(e)),
        };
        IrStmt { kind, span: s.span }
    }).collect()
}
