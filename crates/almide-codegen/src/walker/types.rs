//! Type rendering: converts Ty nodes to target-specific type strings.

use almide_lang::types::{Ty, TypeConstructorId};
use super::RenderContext;
use super::helpers::template_or;

/// `Ty::Named` case of `render_type`, extracted verbatim (cog>30
/// decomposition, second round on top of round 1's).
fn render_type_named(ctx: &RenderContext, name: &almide_base::intern::Sym, args: &[Ty]) -> String {
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
        // If the type has generic parameters but no type arguments,
        // emit `_` to let Rust infer the concrete type
        if ctx.generic_types.contains(name) {
            return "_".to_string();
        }
        // Strip module qualifier: module.Type → Type
        // (all modules flatten into one file in generated Rust)
        let bare = name.rsplit('.').next().unwrap_or(name);
        // A runtime-owned nominal renders under the runtime's reserved
        // spelling (`Value` → `AlmideValue`, #1821); the bare spelling is a
        // user type's.
        if let Some(reserved) = ctx.ann.runtime_owned_types.get(bare) {
            return reserved.clone();
        }
        bare.to_string()
    } else {
        let bare = name.rsplit('.').next().unwrap_or(name);
        // A fully-phantom record's struct is emitted WITHOUT generics
        // (#621), so a reference must drop its type args too.
        if ctx.ann.phantom_param_structs.contains(name.as_str())
            || ctx.ann.phantom_param_structs.contains(bare)
        {
            return bare.to_string();
        }
        let args_str = args.iter().map(|a| render_type(ctx, a)).collect::<Vec<_>>().join(", ");
        format!("{}<{}>", bare, args_str)
    }
}

/// `Ty::Record | Ty::OpenRecord` case of `render_type`, extracted verbatim.
fn render_type_record(ctx: &RenderContext, fields: &[(almide_base::intern::Sym, Ty)]) -> String {
    let mut names: Vec<String> = fields.iter().map(|(n, _)| n.to_string()).collect();
    names.sort();
    // Check named records first (user-defined types)
    if let Some(n) = ctx.ann.named_records.get(&names) {
        // If the struct is generic but no type args are present, let Rust infer
        if ctx.generic_types.contains(&almide_base::intern::sym(n)) {
            return "_".to_string();
        }
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

/// The types with a direct Rust primitive spelling and no templating layer.
///
/// The sized numerics are Stage 1a of the sized-numeric-types arc: each has a
/// Rust primitive with matching width + signedness, and every backend that uses
/// this renderer (Rust) has identical mappings. WASM emission has its own
/// `ty_to_valtype` flow.
fn primitive_type_name(ty: &Ty) -> Option<&'static str> {
    Some(match ty {
        Ty::Int8 => "i8",
        Ty::Int16 => "i16",
        Ty::Int32 => "i32",
        Ty::Int64 => "i64",
        Ty::UInt8 => "u8",
        Ty::UInt16 => "u16",
        Ty::UInt32 => "u32",
        Ty::UInt64 => "u64",
        Ty::Float32 => "f32",
        Ty::Float64 => "f64",
        Ty::RawPtr => "*mut u8",
        _ => return None,
    })
}

/// The leaf types rendered through a template key, as `(key, fallback)`.
///
/// #617: Bytes/Matrix are RcCow VALUE types on the Rust target — copies are O(1)
/// Rc bumps, mutation is make_mut copy-on-write (rust.toml templates + the
/// `rc_cow_result_glue` boundary in the expression walker).
///
/// The bottom type renders as `()` — consistent with how the rest of codegen
/// treats it (`Ty::Unit | Ty::Never` are grouped in pass_stack_balance and
/// emit_wasm/values). A diverging value is never used, so `()` is sound. Without
/// this, a `Ty::Never` reaching a NAMED position (e.g. `Rc<dyn Fn() -> Never>`
/// under the uniform closure repr — a closure body that always throws) fell to
/// the Display fallback and emitted the invalid Rust type name `Never`.
fn templated_scalar_type(ty: &Ty) -> Option<(&'static str, &'static str)> {
    Some(match ty {
        Ty::Int => ("type_int", "i64"),
        Ty::Float => ("type_float", "f64"),
        Ty::String => ("type_string", "String"),
        Ty::Bool => ("type_bool", "bool"),
        Ty::Unit | Ty::Never => ("type_unit", "()"),
        Ty::Bytes => ("type_bytes", "RcCow<Vec<u8>>"),
        Ty::Matrix => ("type_matrix", "RcCow<AlmideMatrix>"),
        _ => return None,
    })
}

/// Every `Ty::Applied` form: the builtin containers, then user-defined
/// constructors. Arity guards keep a malformed application on the catch-all.
fn render_type_applied(ctx: &RenderContext, id: &TypeConstructorId, args: &[Ty]) -> String {
    match (id, args) {
        // Matrix[T] parametric form (Sized Numeric Types arc P4 kickoff): the
        // runtime representation stays `AlmideMatrix` (a tagged enum that
        // dispatches to SmallF32 / Vec<Vec<f64>> backends). The `T` parameter
        // gates type-level distinction only — user code can mix
        // `Matrix[Float32]` / `Matrix[Float64]` annotations without a separate
        // Rust type surface yet. Type-specialised layouts will fold into this
        // arm in a follow-up codegen arc.
        (TypeConstructorId::Matrix, _) => template_or(ctx, "type_matrix", &[], "RcCow<AlmideMatrix>"),
        (TypeConstructorId::Option, [inner]) => {
            let inner_s = render_type(ctx, inner);
            ctx.templates.render_with("type_option", None, &[], &[("inner", inner_s.as_str())])
                .unwrap_or_else(|| format!("Option<{}>", inner_s))
        }
        (TypeConstructorId::Result, [ok, err]) => {
            let ok_s = render_type(ctx, ok);
            // An UNCONSTRAINED error type (e.g. `ok(7)` whose `?E` is never forced
            // concrete) renders as `_`, which rustc cannot infer (E0283). Default
            // it to `String` — Almide's conventional error type (`effect fn` →
            // `Result<_, String>`). A real generic error param (named typevar) is
            // left untouched.
            let err_s = match err {
                Ty::Unknown => "String".to_string(),
                Ty::TypeVar(n) if n.starts_with('?') => "String".to_string(),
                _ => render_type(ctx, err),
            };
            ctx.templates.render_with("type_result", None, &[], &[("ok", ok_s.as_str()), ("err", err_s.as_str())])
                .unwrap_or_else(|| format!("Result<{}, {}>", ok_s, err_s))
        }
        (TypeConstructorId::List, [inner]) => {
            let inner_s = render_type(ctx, inner);
            ctx.templates.render_with("type_list", None, &[], &[("inner", inner_s.as_str())])
                .unwrap_or_else(|| format!("Vec<{}>", inner_s))
        }
        (TypeConstructorId::Map, [k, v]) => {
            let key_s = render_type(ctx, k);
            let value_s = render_type(ctx, v);
            ctx.templates.render_with("type_map", None, &[], &[("key", key_s.as_str()), ("value", value_s.as_str())])
                .unwrap_or_else(|| format!("HashMap<{}, {}>", key_s, value_s))
        }
        (TypeConstructorId::Set, [inner]) => {
            let inner_s = render_type(ctx, inner);
            ctx.templates.render_with("type_set", None, &[], &[("inner", inner_s.as_str())])
                .unwrap_or_else(|| format!("std::collections::HashSet<{}>", inner_s))
        }
        // Other Applied types (e.g. user-defined type constructors).
        (TypeConstructorId::UserDefined(n), _) if args.is_empty() => n.as_str().to_string(),
        (TypeConstructorId::UserDefined(n), _) => {
            let args_str = args.iter().map(|a| render_type(ctx, a)).collect::<Vec<_>>().join(", ");
            format!("{}<{}>", n.as_str(), args_str)
        }
        _ => id.to_string(),
    }
}

pub fn render_type(ctx: &RenderContext, ty: &Ty) -> String {
    if let Some(prim) = primitive_type_name(ty) {
        return prim.to_string();
    }
    if let Some((key, fallback)) = templated_scalar_type(ty) {
        return template_or(ctx, key, &[], fallback);
    }
    match ty {
        Ty::Applied(id, args) => render_type_applied(ctx, id, args),
        Ty::Named(name, args) => render_type_named(ctx, name, args),
        Ty::Record { fields } | Ty::OpenRecord { fields } => render_type_record(ctx, fields),
        // UNIFORM-REPR SPIKE: every closure type — in ANY position (var, field,
        // function parameter, return) — is `Rc<dyn Fn(...) -> T>`. This kills the
        // `impl Fn` vs `Rc<dyn Fn>` split that forced per-site boxing decisions
        // and the HOF-param E0277 divergence. `render_type_field_fn` already
        // renders every (possibly nested) Fn as `Rc<dyn Fn>`.
        Ty::Fn { .. } => super::helpers::render_type_field_fn(ctx, ty),
        Ty::Tuple(elems) => {
            let parts = super::helpers::tuple_elems_join(
                &elems.iter().map(|t| render_type(ctx, t)).collect::<Vec<_>>(),
            );
            ctx.templates.render_with("type_tuple", None, &[], &[("elements", parts.as_str())])
                .unwrap_or_else(|| "tuple".into())
        }
        Ty::TypeVar(n) if n.starts_with('?') => template_or(ctx, "typevar_infer", &[], "_"),
        Ty::TypeVar(n) => n.to_string(),
        Ty::Unknown | Ty::Union(_) => template_or(ctx, "unknown_type", &[], "_"),
        Ty::Variant { name, .. } => ctx.ann.runtime_owned_types.get(name.as_str())
            .cloned()
            .unwrap_or_else(|| name.to_string()),
        // Fallback
        #[allow(unreachable_patterns)]
        _ => format!("{}", ty.display()),
    }
}
