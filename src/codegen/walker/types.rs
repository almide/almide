//! Type rendering: converts Ty nodes to target-specific type strings.

use crate::types::{Ty, TypeConstructorId};
use super::RenderContext;
use super::helpers::{template_or, render_type_boxed_fn};

pub fn render_type(ctx: &RenderContext, ty: &Ty) -> String {
    match ty {
        Ty::Int => template_or(ctx, "type_int", &[], "i64"),
        Ty::Float => template_or(ctx, "type_float", &[], "f64"),
        Ty::String => template_or(ctx, "type_string", &[], "String"),
        Ty::Bool => template_or(ctx, "type_bool", &[], "bool"),
        Ty::Unit => template_or(ctx, "type_unit", &[], "()"),
        Ty::Bytes => template_or(ctx, "type_bytes", &[], "Vec<u8>"),
        Ty::Matrix => template_or(ctx, "type_matrix", &[], "Vec<Vec<f64>>"),
        Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => {
            let inner = &args[0];
            let inner_s = render_type(ctx, inner);
            ctx.templates.render_with("type_option", None, &[], &[("inner", inner_s.as_str())])
                .unwrap_or_else(|| format!("Option<{}>", render_type(ctx, inner)))
        }
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
            let (ok, err) = (&args[0], &args[1]);
            let ok_s = render_type(ctx, ok);
            let err_s = render_type(ctx, err);
            ctx.templates.render_with("type_result", None, &[], &[("ok", ok_s.as_str()), ("err", err_s.as_str())])
                .unwrap_or_else(|| format!("Result<{}, {}>", render_type(ctx, ok), render_type(ctx, err)))
        }
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            let inner = &args[0];
            let inner_s = render_type(ctx, inner);
            ctx.templates.render_with("type_list", None, &[], &[("inner", inner_s.as_str())])
                .unwrap_or_else(|| format!("Vec<{}>", render_type(ctx, inner)))
        }
        Ty::Named(name, args) => {
            // Set type → template
            if name == "Set" && args.len() == 1 {
                let inner = render_type(ctx, &args[0]);
                return ctx.templates.render_with("type_set", None, &[], &[("inner", &inner)])
                    .unwrap_or_else(|| format!("Set<{}>", inner));
            }
            // Expand type aliases transparently
            if args.is_empty() {
                if let Some(target) = ctx.type_aliases.get(name) {
                    return render_type(ctx, target);
                }
            }
            if args.is_empty() {
                name.to_string()
            } else {
                let args_str = args.iter().map(|a| render_type(ctx, a)).collect::<Vec<_>>().join(", ");
                format!("{}<{}>", name, args_str)
            }
        }
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            let mut names: Vec<String> = fields.iter().map(|(n, _)| n.to_string()).collect();
            names.sort();
            // Check named records first (user-defined types)
            if let Some(n) = ctx.ann.named_records.get(&names) {
                return n.clone();
            }
            // Check anonymous records
            if let Some(n) = ctx.ann.anon_records.get(&names) {
                // Generic anonymous record: AlmdRec0<Type0, Type1, ...>
                let mut sorted_fields: Vec<_> = fields.iter().collect();
                sorted_fields.sort_by(|a, b| a.0.cmp(&b.0));
                let args: Vec<String> = sorted_fields.iter().map(|(_, t)| render_type(ctx, t)).collect();
                if args.is_empty() {
                    n.clone()
                } else {
                    format!("{}<{}>", n, args.join(", "))
                }
            } else {
                // Fallback: sorted field names
                names.join("_")
            }
        }
        Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => {
            let (k, v) = (&args[0], &args[1]);
            let key_s = render_type(ctx, k);
            let value_s = render_type(ctx, v);
            ctx.templates.render_with("type_map", None, &[], &[("key", key_s.as_str()), ("value", value_s.as_str())])
                .unwrap_or_else(|| format!("HashMap<{}, {}>", render_type(ctx, k), render_type(ctx, v)))
        }
        Ty::Applied(TypeConstructorId::Set, args) if args.len() == 1 => {
            let inner_s = render_type(ctx, &args[0]);
            ctx.templates.render_with("type_set", None, &[], &[("inner", inner_s.as_str())])
                .unwrap_or_else(|| format!("std::collections::HashSet<{}>", render_type(ctx, &args[0])))
        }
        // Catch-all for other Applied types (e.g., user-defined type constructors)
        Ty::Applied(id, args) => {
            let name = match id {
                TypeConstructorId::UserDefined(n) => n.as_str(),
                _ => return id.to_string(),
            };
            if args.is_empty() {
                name.to_string()
            } else {
                let args_str = args.iter().map(|a| render_type(ctx, a)).collect::<Vec<_>>().join(", ");
                format!("{}<{}>", name, args_str)
            }
        }
        Ty::Fn { params, ret } => {
            let params_str = params.iter().map(|p| render_type(ctx, p)).collect::<Vec<_>>().join(", ");
            // Nested Fn return may need boxing (Rust: Box<dyn Fn>; TS: identity)
            let ret_str = if matches!(ret.as_ref(), Ty::Fn { .. }) {
                render_type_boxed_fn(ctx, ret)
            } else {
                render_type(ctx, ret)
            };
            ctx.templates.render_with("type_fn", None, &[], &[("params", params_str.as_str()), ("return", ret_str.as_str())])
                .unwrap_or_else(|| format!("Fn({})", params_str))
        }
        Ty::Tuple(elems) => {
            let parts = elems.iter().map(|t| render_type(ctx, t)).collect::<Vec<_>>().join(", ");
            ctx.templates.render_with("type_tuple", None, &[], &[("elements", parts.as_str())])
                .unwrap_or_else(|| "tuple".into())
        }
        Ty::TypeVar(n) => {
            if n.starts_with('?') {
                template_or(ctx, "typevar_infer", &[], "_")
            } else {
                n.to_string()
            }
        }
        Ty::Unknown | Ty::Union(_) => {
            template_or(ctx, "unknown_type", &[], "_")
        }
        Ty::Variant { name, .. } => name.to_string(),
        // Fallback
        #[allow(unreachable_patterns)]
        _ => format!("{}", ty.display()),
    }
}
