// ── Call lowering ───────────────────────────────────────────────

use crate::ast;
use crate::ir::*;
use crate::types::{Ty, TypeConstructorId};
use crate::intern::sym;
use super::LowerCtx;
use super::expressions::lower_expr;
use super::types::resolve_type_expr;

pub(super) fn lower_call(ctx: &mut LowerCtx, callee: &ast::Expr, args: &[ast::Expr], named_args: &[(crate::intern::Sym, ast::Expr)], type_args: Option<&Vec<ast::TypeExpr>>, ty: Ty, span: Option<ast::Span>) -> IrExpr {
    // Convenience: json.encode(expr) → json.stringify(T.encode(expr)) when expr is Codec type
    if let ast::Expr::Member { object, field, .. } = callee {
        if let ast::Expr::Ident { name: module, .. } = object.as_ref() {
            if field == "encode" && args.len() == 1 {
                let arg_ty = ctx.expr_ty(&args[0]);
                if let Some(encode_fn) = ctx.find_convention_fn(&arg_ty, "encode") {
                    let ir_arg = lower_expr(ctx, &args[0]);
                    let encoded = ctx.mk(IrExprKind::Call {
                        target: CallTarget::Named { name: encode_fn },
                        args: vec![ir_arg], type_args: vec![],
                    }, Ty::Named("Value".into(), vec![]), span);
                    return ctx.mk(IrExprKind::Call {
                        target: CallTarget::Module { module: sym(module), func: sym("stringify") },
                        args: vec![encoded], type_args: vec![],
                    }, Ty::String, span);
                }
            }
            if field == "decode" && args.len() == 1
                && let Some(type_args) = type_args
                && let Some(ast::TypeExpr::Simple { name: type_name }) = type_args.first()
            {
                let ir_arg = lower_expr(ctx, &args[0]);
                // json.decode[T](text) → T.decode(json.parse(text)?)
                let parsed = ctx.mk(IrExprKind::Try { expr: Box::new(ctx.mk(IrExprKind::Call {
                    target: CallTarget::Module { module: sym(module), func: sym("parse") },
                    args: vec![ir_arg], type_args: vec![],
                }, Ty::result(Ty::Named("Value".into(), vec![]), Ty::String), span)) },
                Ty::Named("Value".into(), vec![]), span);
                let decode_fn = sym(&format!("{}.decode", type_name));
                return ctx.mk(IrExprKind::Call {
                    target: CallTarget::Named { name: decode_fn },
                    args: vec![parsed], type_args: vec![],
                }, ty, span);
            }
        }
    }


    let mut ir_args: Vec<IrExpr> = args.iter().map(|a| lower_expr(ctx, a)).collect();
    let ta = type_args.map(|tas| tas.iter().map(|t| resolve_type_expr(t)).collect()).unwrap_or_default();
    let target = lower_call_target(ctx, callee);

    // Named args: resolve to positional order using function signature
    if let (false, CallTarget::Named { name }) = (named_args.is_empty(), &target) {
        let param_names: Vec<String> = ctx.env.functions.get(name)
            .map(|sig| sig.params.iter().map(|(n, _)| n.to_string()).collect())
            .unwrap_or_default();
        let defaults = ctx.fn_defaults.get(name).cloned();
        let positional_count = ir_args.len();
        ir_args.extend(param_names[positional_count..].iter().filter_map(|param_name| {
            named_args.iter()
                .find(|(n, _)| n == param_name)
                .map(|(_, expr)| lower_expr(ctx, expr))
                .or_else(|| defaults.as_ref()
                    .and_then(|defs| defs.get(positional_count + param_names[positional_count..].iter().position(|p| p == param_name).unwrap_or(0)))
                    .and_then(|d| d.as_ref())
                    .map(|default_expr| lower_expr(ctx, default_expr)))
        }));
    }

    // Default args: fill in remaining defaults (for calls without named args)
    if let (true, CallTarget::Named { name }) = (named_args.is_empty(), &target) {
        if let Some(defaults) = ctx.fn_defaults.get(name).cloned() {
            ir_args.extend(
                defaults.iter().skip(ir_args.len())
                    .filter_map(|d| d.as_ref().map(|expr| lower_expr(ctx, expr)))
            );
        }
    }

    ctx.mk(IrExprKind::Call { target, args: ir_args, type_args: ta }, ty, span)
}

pub(super) fn lower_call_target(ctx: &mut LowerCtx, callee: &ast::Expr) -> CallTarget {
    match callee {
        ast::Expr::Ident { name, .. } | ast::Expr::TypeName { name, .. } => {
            // If the name resolves to a local variable (e.g., a closure), use Computed
            // so that use-count tracking properly counts this as a use of that variable.
            if let Some(var_id) = ctx.lookup_var(name)
                && matches!(ctx.var_table.get(var_id).ty, crate::types::Ty::Fn { .. })
            {
                let ty = ctx.expr_ty(callee);
                return CallTarget::Computed {
                    callee: Box::new(ctx.mk(IrExprKind::Var { id: var_id }, ty, callee.span())),
                };
            }
            CallTarget::Named { name: *name }
        }
        ast::Expr::Member { object, field, .. } => {
            // Check if this is a module call (e.g., string.trim, list.map)
            if let ast::Expr::Ident { name: module, .. } = object.as_ref() {
                // Local variables take precedence over module names
                if ctx.lookup_var(module).is_none() && (module == "fan"
                    || crate::stdlib::is_stdlib_module(module) || crate::stdlib::is_any_stdlib(module)
                    || ctx.env.user_modules.contains(module))
                {
                    let resolved = ctx.env.module_aliases.get(module).copied().unwrap_or(*module);
                    return CallTarget::Module { module: resolved, func: *field };
                }
                // Ident that's not a module: check if Type.method (protocol impl, e.g. Val.double)
                if ctx.lookup_var(module).is_none() {
                    let key = format!("{}.{}", module, field);
                    if ctx.env.functions.contains_key(&sym(&key))
                        || ctx.find_convention_fn(&Ty::Named(sym(module), vec![]), field).is_some()
                    {
                        return CallTarget::Named { name: sym(&key) };
                    }
                }
            }
            // TypeName.method(args) → direct named call (not UFCS, no object prepend)
            if let ast::Expr::TypeName { name: type_name, .. } = object.as_ref() {
                let key = format!("{}.{}", type_name, field);
                if ctx.env.functions.contains_key(&sym(&key))
                    || ctx.find_convention_fn(&Ty::Named(sym(type_name), vec![]), field).is_some()
                {
                    return CallTarget::Named { name: sym(&key) };
                }
            }
            // Built-in generic types: xs.len() → list.len(xs) for List, Map, etc.
            let obj_ty = ctx.expr_ty(object);
            let builtin_module = match &obj_ty {
                Ty::Applied(TypeConstructorId::List, _) => Some("list"),
                Ty::Applied(TypeConstructorId::Map, _) => Some("map"),
                Ty::Applied(TypeConstructorId::Set, _) => Some("set"),
                Ty::String => Some("string"),
                Ty::Int => Some("int"),
                Ty::Float => Some("float"),
                Ty::Applied(TypeConstructorId::Result, _) => Some("result"),
                Ty::Applied(TypeConstructorId::Option, _) => Some("option"),
                _ => None,
            };
            if let Some(module) = builtin_module {
                let key = format!("{}.{}", module, field);
                if ctx.env.functions.contains_key(&sym(&key))
                    || crate::stdlib::resolve_ufcs_candidates(field).contains(&module)
                {
                    let ir_obj = lower_expr(ctx, object);
                    return CallTarget::Method { object: Box::new(ir_obj), method: sym(&key) };
                }
            }
            // Check for convention method: dog.repr() → Dog.repr(dog)
            let type_name_opt = match &obj_ty {
                Ty::Named(name, _) => Some(name.to_string()),
                Ty::Record { .. } | Ty::Variant { .. } => {
                    ctx.env.types.iter().find_map(|(name, ty)| {
                        if ty == &obj_ty && name.chars().next().map_or(false, |c| c.is_uppercase()) {
                            Some(name.to_string())
                        } else { None }
                    })
                }
                _ => None,
            };
            if let Some(type_name) = type_name_opt {
                let convention_key = format!("{}.{}", type_name, field);
                if ctx.env.functions.contains_key(&sym(&convention_key))
                    || ctx.find_convention_fn(&Ty::Named(sym(&type_name), vec![]), field).is_some()
                {
                    let ir_obj = lower_expr(ctx, object);
                    return CallTarget::Method { object: Box::new(ir_obj), method: sym(&convention_key) };
                }
            }
            // Protocol method on TypeVar: item.show() where item: T, T: Showable
            // Lower as "T.show" convention key — monomorphizer will substitute T → ConcreteType
            if let Ty::TypeVar(tv) = &obj_ty {
                if let Some(proto_names) = ctx.protocol_bounds.get(tv).cloned() {
                    for proto_name in &proto_names {
                        if let Some(proto_def) = ctx.env.protocols.get(&sym(proto_name)) {
                            if proto_def.methods.iter().any(|m| m.name == *field) {
                                let ir_obj = lower_expr(ctx, object);
                                let convention_key = sym(&format!("{}.{}", tv, field));
                                return CallTarget::Method { object: Box::new(ir_obj), method: convention_key };
                            }
                        }
                    }
                }
            }
            // Generic method call: obj.method(args) → UFCS
            let ir_obj = lower_expr(ctx, object);
            CallTarget::Method { object: Box::new(ir_obj), method: *field }
        }
        _ => {
            let ir_callee = lower_expr(ctx, callee);
            CallTarget::Computed { callee: Box::new(ir_callee) }
        }
    }
}
