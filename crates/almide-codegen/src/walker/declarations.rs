//! Type declaration rendering and anonymous record collection.

use std::collections::{HashMap, HashSet};
use almide_ir::*;
use almide_lang::types::Ty;
use super::RenderContext;
use super::types::render_type;
use super::helpers::{template_or, render_type_field_fn};

pub fn render_type_decl(ctx: &RenderContext, td: &IrTypeDecl) -> String {
    let decl_attrs: Vec<&str> = if ctx.repr_c { vec!["repr_c"] } else { vec![] };

    // Build generics string e.g. "<T>" or "<T, U>"
    let generics_str = if ctx.ann.phantom_param_structs.contains(td.name.as_str()) {
        // All params phantom (#621): drop them so Rust doesn't reject E0392.
        String::new()
    } else if let Some(generics) = &td.generics {
        if generics.is_empty() {
            String::new()
        } else {
            let params = generics.iter().map(|g| {
                ctx.templates.render_with("generic_bound", None, &[], &[("name", g.name.as_str())])
                    .unwrap_or_else(|| g.name.to_string())
            }).collect::<Vec<_>>().join(", ");
            format!("<{}>", params)
        }
    } else {
        String::new()
    };

    let decl = match &td.kind {
        IrTypeDeclKind::Record { .. } => render_type_decl_record(ctx, td, &generics_str, &decl_attrs),
        IrTypeDeclKind::Variant { .. } => render_type_decl_variant(ctx, td, &generics_str, &decl_attrs),
        IrTypeDeclKind::Alias { target } => {
            // Fn type aliases are erased — the type checker expands them at use sites
            if matches!(target, Ty::Fn { .. }) {
                return String::new();
            }
            // Opaque (mod/local) aliases → newtype struct
            if matches!(td.visibility, IrVisibility::Mod | IrVisibility::Private) {
                let type_s = render_type(ctx, target);
                return format!(
                    "#[derive(Clone, Debug, PartialEq)]\npub struct {}({});",
                    td.name, type_s
                );
            }
            // Transparent aliases to primitives are expanded at use sites
            // by render_type via type_aliases. Don't emit a Rust `type`
            // declaration — it would shadow runtime `use` imports
            // (e.g. `type TcpStream = i64` shadows `std::net::TcpStream`).
            if ctx.type_aliases.contains_key(&td.name) {
                return String::new();
            }
            let type_s = render_type(ctx, target);
            ctx.templates.render_with("type_alias", None, &[], &[("name", td.name.as_str()), ("type", type_s.as_str())])
                .unwrap_or_else(|| format!("type {} = {};", td.name, render_type(ctx, target)))
        }
    };

    // Emit the `AlmideRepr` impl for records/variants so a record/variant value
    // interpolated in a string (`"${p}"`) renders to its Almide-literal form —
    // the SAME construction-literal format `deriving Repr` produces — on BOTH
    // targets (the WASM half walks the same layout). Closure-bearing types are
    // skipped (a closure is not `AlmideRepr`; they never reach compound interp).
    if let Some(impl_s) = render_repr_impl(ctx, td) {
        format!("{decl}\n{impl_s}")
    } else {
        decl
    }
}

/// `IrTypeDeclKind::Record` case of `render_type_decl`, extracted verbatim
/// (cog>30 decomposition, pattern 2 — mirrors the existing `render_type`'s
/// Named/Record split and `TyChecker::check_ty`'s Named/Variant split).
fn render_type_decl_record(ctx: &RenderContext, td: &IrTypeDecl, generics_str: &str, decl_attrs: &[&str]) -> String {
    let IrTypeDeclKind::Record { fields } = &td.kind else { unreachable!() };
    let has_fn_fields = fields.iter().any(|f| matches!(&f.ty, Ty::Fn { .. }));
    // Matrix / Fn / transitively-blocking types prevent PartialEq derive.
    // Uses the precomputed `eq_blocked_types` set so Named references
    // to other blocked user types propagate correctly.
    let has_non_eq_fields = fields.iter().any(|f| ty_blocks_eq_with(&f.ty, &ctx.ann.eq_blocked_types));
    let fields_str = fields.iter()
        .map(|f| {
            // Closure-bearing struct fields use Rc<dyn Fn> (impl Fn is
            // invalid in struct position, Box<dyn Fn> is not Clone) — also
            // when the closure is nested in a List/Map/Tuple field. Fn-free
            // field types fall through to the normal renderer.
            let type_s = render_type_field_fn(ctx, &f.ty);
            ctx.templates.render_with("struct_field", None, &[], &[("name", f.name.as_str()), ("type", type_s.as_str())])
                .unwrap_or_else(|| format!("    pub {}: {},", f.name, render_type(ctx, &f.ty)))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let full_name = format!("{}{}", td.name, generics_str);
    let has_hash = td.deriving.as_ref().map_or(false, |d| d.iter().any(|s| s.as_str() == "Hash"));
    let mut attrs = decl_attrs.to_vec();
    if has_fn_fields { attrs.push("has_fn_fields"); }
    if has_non_eq_fields { attrs.push("has_non_eq_fields"); }
    if has_hash && !has_fn_fields && !has_non_eq_fields { attrs.push("has_hash"); }
    let repr_prefix = if ctx.repr_c { "#[repr(C)]\n" } else { "" };
    let fallback = if has_fn_fields {
        format!("#[derive(Clone)]\npub struct {} {{\n{}\n}}", full_name, &fields_str)
    } else if has_non_eq_fields {
        format!("{}#[derive(Clone, Debug)]\npub struct {} {{\n{}\n}}", repr_prefix, full_name, &fields_str)
    } else {
        format!("{}pub struct {} {{\n{}\n}}", repr_prefix, full_name, &fields_str)
    };
    ctx.templates.render_with("struct_decl", None, &attrs, &[("name", full_name.as_str()), ("fields", fields_str.as_str())])
        .unwrap_or(fallback)
}

/// `IrTypeDeclKind::Variant` case of `render_type_decl`, extracted verbatim
/// (cog>30 decomposition, pattern 2).
fn render_type_decl_variant(ctx: &RenderContext, td: &IrTypeDecl, generics_str: &str, decl_attrs: &[&str]) -> String {
    let IrTypeDeclKind::Variant { cases, .. } = &td.kind else { unreachable!() };
    let variants_parts: Vec<String> = cases.iter()
        .map(|v| match &v.kind {
            IrVariantKind::Unit => {
                ctx.templates.render_with("enum_variant_unit", None, &[], &[("name", v.name.as_str())])
                    .unwrap_or_else(|| v.name.to_string())
            }
            IrVariantKind::Tuple { fields } => {
                let is_recursive = ctx.ann.recursive_enums.contains(&*td.name);
                let types: Vec<String> = fields.iter().map(|t| {
                    // Closure payloads (direct or nested in a container) use
                    // Rc<dyn Fn> — same as a struct field.
                    let rendered = render_type_field_fn(ctx, t);
                    // Box a field referencing ANY cycle member (mutual recursion), not
                    // just the enclosing type's own name (#656).
                    if is_recursive && super::ty_contains_any_recursive(t, &ctx.ann.recursive_enums) { format!("std::boxed::Box<{}>", rendered) } else { rendered }
                }).collect();
                let fields_str = types.join(", ");
                // Named params via fn_param template (respects JS/TS)
                let params_str = types.iter().enumerate()
                    .map(|(i, t)| {
                        let name = format!("v{}", i);
                        ctx.templates.render_with("fn_param", None, &[], &[("name", name.as_str()), ("type", t.as_str())])
                            .unwrap_or(name)
                    })
                    .collect::<Vec<_>>().join(", ");
                let param_names = (0..types.len()).map(|i| format!("v{}", i))
                    .collect::<Vec<_>>().join(", ");
                let fallback = format!("{}({})", v.name, &fields_str);
                ctx.templates.render_with("enum_variant", None, &[], &[("name", v.name.as_str()), ("fields", fields_str.as_str()), ("params", params_str.as_str()), ("param_names", param_names.as_str())])
                    .unwrap_or(fallback)
            }
            IrVariantKind::Record { fields } => {
                let fields_str = fields.iter()
                    .map(|f| {
                        let rendered = render_type_field_fn(ctx, &f.ty);
                        let boxed = if ctx.ann.recursive_enums.contains(&*td.name) && super::ty_contains_any_recursive(&f.ty, &ctx.ann.recursive_enums) {
                            format!("std::boxed::Box<{}>", rendered)
                        } else {
                            rendered
                        };
                        ctx.templates.render_with("fn_param", None, &[], &[("name", f.name.as_str()), ("type", boxed.as_str())])
                            .unwrap_or_else(|| format!("{}: {}", f.name, boxed))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let field_names = fields.iter().map(|f| f.name.to_string()).collect::<Vec<_>>().join(", ");
                ctx.templates.render_with("enum_variant_record", None, &[], &[("name", v.name.as_str()), ("fields", fields_str.as_str()), ("field_names", field_names.as_str())])
                    .unwrap_or_else(|| format!("{} {{ {} }}", v.name, fields_str))
            }
        })
        .collect::<Vec<_>>();
    let sep = template_or(ctx, "enum_variant_sep", &[], ",\n");
    let variants_str = variants_parts.join(&sep);
    let full_name = format!("{}{}", td.name, generics_str);
    let has_hash = td.deriving.as_ref().map_or(false, |d| d.iter().any(|s| s.as_str() == "Hash"));
    // A closure payload (Fn directly, or nested in a container) lowers to
    // `Rc<dyn Fn>`, which is neither Debug nor PartialEq → derive Clone only.
    let has_fn_fields = cases.iter().any(|v| match &v.kind {
        IrVariantKind::Unit => false,
        IrVariantKind::Tuple { fields } => fields.iter().any(ty_has_fn),
        IrVariantKind::Record { fields } => fields.iter().any(|f| ty_has_fn(&f.ty)),
    });
    let mut enum_attrs = decl_attrs.to_vec();
    if has_fn_fields { enum_attrs.push("has_fn_fields"); }
    else if has_hash { enum_attrs.push("has_hash"); }
    let repr_prefix = if ctx.repr_c { "#[repr(C)]\n" } else { "" };
    let fallback = if has_fn_fields {
        format!("#[derive(Clone)]\npub enum {} {{\n{}\n}}", full_name, &variants_str)
    } else {
        format!("{}pub enum {} {{\n{}\n}}", repr_prefix, full_name, &variants_str)
    };
    ctx.templates.render_with("enum_decl", None, &enum_attrs, &[("name", full_name.as_str()), ("variants", variants_str.as_str())])
        .unwrap_or(fallback)
}

/// Build `impl AlmideRepr for <Type>` for a record or variant type, mirroring
/// the `auto_derive_repr` literal format:
///   record       → `P { x: 1, y: 2 }`     (field declaration order)
///   tuple variant→ `Click(10, 20)`
///   record variant→`Scroll { dy: 5 }`
///   nullary       → `Quit`
/// Each field/payload routes through `almide_repr()`, so strings get quoted,
/// floats use the `Display` form, and nested compounds recurse — identical to
/// how a field renders inside any other container. Returns `None` for types
/// that cannot back a repr (closure fields) or are not records/variants.
/// Whether a type decl gets a generated `AlmideRepr` impl: a record or variant
/// whose fields/payloads are all `AlmideRepr` (no closure field). The interp
/// router (`ty_needs_repr`) and the impl emitter share this predicate so the
/// "this Named type is repr-backed" decision is made in exactly one place.
pub fn type_has_repr_impl(td: &IrTypeDecl) -> bool {
    match &td.kind {
        IrTypeDeclKind::Record { fields } => !fields.iter().any(|f| ty_has_fn(&f.ty)),
        IrTypeDeclKind::Variant { cases, .. } => !cases.iter().any(|v| match &v.kind {
            IrVariantKind::Unit => false,
            IrVariantKind::Tuple { fields } => fields.iter().any(ty_has_fn),
            IrVariantKind::Record { fields } => fields.iter().any(|f| ty_has_fn(&f.ty)),
        }),
        _ => false,
    }
}

fn render_repr_impl(ctx: &RenderContext, td: &IrTypeDecl) -> Option<String> {
    // Records/variants without a closure field back a repr; skip everything else.
    if !type_has_repr_impl(td) { return None; }

    // Generic header + target. The impl GENERICS carry every param's bounds; the
    // impl TARGET uses BARE params. For a generic type these MUST differ:
    //   impl<T: AlmideRepr + Clone + PartialEq> AlmideRepr for Tree<T> { … }
    //                       ^^^^^^^^^^^^^^^^^^^ bounds belong here          ^^^ bare
    // Reusing the decl's bounded `<T: Clone + PartialEq>` as the target emits the
    // invalid `for Tree<T: Clone + PartialEq>` (E0229: associated-item-constraint
    // not allowed). The param's own bound set is whatever the struct/enum decl
    // declared (via the `generic_bound` template), plus `AlmideRepr` so the field
    // reprs compose; rendering them through the same template keeps the impl
    // bounds in lock-step with the type decl (Rust requires the impl to satisfy
    // every bound the type definition declares).
    // A fully-phantom record (#621) has its generics stripped from the struct,
    // so its impl must be ungenerified too (`impl AlmideRepr for Tagged`).
    let phantom = ctx.ann.phantom_param_structs.contains(td.name.as_str());
    let (impl_generics, target_args) = match td.generics.as_ref().filter(|g| !g.is_empty() && !phantom) {
        Some(generics) => {
            let impl_bounds = generics.iter().map(|g| {
                // The type DEFINITION declares each param via `generic_bound`
                // (`T: Clone + PartialEq`); the impl must satisfy those same
                // bounds, so reuse the rendered form and splice `AlmideRepr` in
                // right after the `name:` colon → `T: AlmideRepr + Clone + …`.
                let own = ctx.templates.render_with("generic_bound", None, &[], &[("name", g.name.as_str())])
                    .unwrap_or_else(|| format!("{}: Clone + PartialEq", g.name));
                match own.split_once(':') {
                    Some((name, rest)) => format!("{}: AlmideRepr +{}", name.trim_end(), rest),
                    None => format!("{}: AlmideRepr", g.name),
                }
            }).collect::<Vec<_>>().join(", ");
            let bare = generics.iter().map(|g| g.name.to_string()).collect::<Vec<_>>().join(", ");
            (format!("<{}>", impl_bounds), format!("<{}>", bare))
        }
        None => (String::new(), String::new()),
    };
    let full_name = format!("{}{}", td.name, target_args);

    let body = match &td.kind {
        IrTypeDeclKind::Record { fields } => {
            // `Type { f0: {}, f1: {} }`, fields in declaration order.
            let fmt = fields.iter().enumerate()
                .map(|(i, f)| format!("{}{}: {{}}", if i > 0 { ", " } else { "" }, f.name))
                .collect::<Vec<_>>().join("");
            let args = fields.iter()
                .map(|f| format!("self.{}.almide_repr()", f.name))
                .collect::<Vec<_>>().join(", ");
            format!("format!(\"{} {{{{ {} }}}}\", {})", td.name, fmt, args)
        }
        IrTypeDeclKind::Variant { cases, .. } => {
            let arms = cases.iter().map(|v| render_repr_variant_arm(&td.name, v))
                .collect::<Vec<_>>().join("\n            ");
            format!("match self {{\n            {}\n        }}", arms)
        }
        _ => return None,
    };

    Some(format!(
        "impl{} AlmideRepr for {} {{ fn almide_repr(&self) -> String {{ {} }} }}",
        impl_generics, full_name, body
    ))
}

/// One match arm of a variant's `AlmideRepr` impl.
fn render_repr_variant_arm(type_name: &str, v: &IrVariantDecl) -> String {
    match &v.kind {
        IrVariantKind::Unit => {
            // Nullary constructor → bare name.
            format!("{}::{} => \"{}\".to_string(),", type_name, v.name, v.name)
        }
        IrVariantKind::Tuple { fields } => {
            // `Click(10, 20)`: positional bindings v0, v1, … rendered via repr.
            let binds = (0..fields.len()).map(|i| format!("v{}", i)).collect::<Vec<_>>().join(", ");
            let fmt = (0..fields.len()).map(|_| "{}").collect::<Vec<_>>().join(", ");
            let args = (0..fields.len()).map(|i| format!("v{}.almide_repr()", i)).collect::<Vec<_>>().join(", ");
            format!("{}::{}({}) => format!(\"{}({})\", {}),", type_name, v.name, binds, v.name, fmt, args)
        }
        IrVariantKind::Record { fields } => {
            // `Scroll { dy: 5 }`: named bindings, field declaration order.
            let binds = fields.iter().map(|f| f.name.to_string()).collect::<Vec<_>>().join(", ");
            let fmt = fields.iter().enumerate()
                .map(|(i, f)| format!("{}{}: {{}}", if i > 0 { ", " } else { "" }, f.name))
                .collect::<Vec<_>>().join("");
            let args = fields.iter().map(|f| format!("{}.almide_repr()", f.name)).collect::<Vec<_>>().join(", ");
            format!("{}::{} {{ {} }} => format!(\"{} {{{{ {} }}}}\", {}),", type_name, v.name, binds, v.name, fmt, args)
        }
    }
}

// ── Anonymous record collection ──
// Simplified version of emit_rust::lower_types logic, directly in codegen.

pub fn collect_named_records(program: &IrProgram) -> HashMap<Vec<String>, String> {
    let mut map = HashMap::new();
    for td in &program.type_decls {
        if let IrTypeDeclKind::Record { fields } = &td.kind {
            let mut names: Vec<String> = fields.iter().map(|f| f.name.to_string()).collect();
            names.sort();
            map.insert(names, td.name.to_string());
        }
    }
    // Also collect from module type declarations
    for module in &program.modules {
        for td in &module.type_decls {
            if let IrTypeDeclKind::Record { fields } = &td.kind {
                let mut names: Vec<String> = fields.iter().map(|f| f.name.to_string()).collect();
                names.sort();
                map.insert(names, td.name.to_string());
            }
        }
    }
    map
}

/// Map each named record type to its total field count so destructure
/// patterns can decide whether they need a trailing `..`.
pub fn collect_record_field_counts(program: &IrProgram) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for td in &program.type_decls {
        if let IrTypeDeclKind::Record { fields } = &td.kind {
            map.insert(td.name.to_string(), fields.len());
        }
    }
    for module in &program.modules {
        for td in &module.type_decls {
            if let IrTypeDeclKind::Record { fields } = &td.kind {
                map.insert(td.name.to_string(), fields.len());
            }
        }
    }
    map
}

/// The anon-record keys (sorted field names) that have a closure field, as
/// gathered by the most recent `collect_anon_records`. Call right after it.
pub fn take_anon_fn_keys() -> HashSet<Vec<String>> {
    ANON_FN_KEYS.with(|s| s.borrow().clone())
}

pub fn collect_anon_records(program: &IrProgram, named: &HashMap<Vec<String>, String>) -> HashMap<Vec<String>, String> {
    ANON_FN_KEYS.with(|s| s.borrow_mut().clear());
    let named_set: HashSet<Vec<String>> = named.keys().cloned().collect();
    let mut seen: HashSet<Vec<String>> = HashSet::new();

    // Collect from TYPE DECLARATIONS — a variant case payload or record field
    // whose type is an anonymous record (e.g. `| Square({ s: Int })`) may NEVER
    // be constructed in the program, so it is reachable only here; without this
    // its struct goes unregistered and `render_type` falls back to emitting the
    // bare field name as a type → `Square(s)` → rustc E0425 (#628).
    for td in program.type_decls.iter().chain(program.modules.iter().flat_map(|m| m.type_decls.iter())) {
        collect_anon_from_type_decl(td, &named_set, &mut seen);
    }

    // Collect from all types AND expressions in the program
    collect_anon_from_fns_and_lets(&program.functions, &program.top_lets, &named_set, &mut seen);
    // Also collect from module functions and top_lets
    for module in &program.modules {
        collect_anon_from_fns_and_lets(&module.functions, &module.top_lets, &named_set, &mut seen);
    }

    let mut map = HashMap::new();
    for key in seen {
        // Derive struct name from sorted field names to prevent cross-crate
        // collisions. Two records with different fields must never share a name.
        let name = format!("AlmdRec_{}", key.join("_"));
        map.insert(key, name);
    }
    map
}

/// Shared body of `collect_anon_records`'s program-level and per-module
/// loops, extracted (cog>30 decomposition, sequential-phase pattern — was
/// duplicated verbatim once for `program.functions`/`program.top_lets` and
/// once for each `module.functions`/`module.top_lets`).
fn collect_anon_from_fns_and_lets(functions: &[IrFunction], top_lets: &[IrTopLet], named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    for func in functions {
        for p in &func.params { collect_anon_from_ty(&p.ty, named, seen); }
        collect_anon_from_ty(&func.ret_ty, named, seen);
        collect_anon_from_expr(&func.body, named, seen);
    }
    for tl in top_lets {
        collect_anon_from_ty(&tl.ty, named, seen);
        collect_anon_from_expr(&tl.value, named, seen);
    }
}

/// Descend a type declaration's field / variant-payload types, registering any
/// anonymous record reachable only from the declaration (never constructed).
fn collect_anon_from_type_decl(td: &IrTypeDecl, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    match &td.kind {
        IrTypeDeclKind::Record { fields } => {
            for f in fields { collect_anon_from_ty(&f.ty, named, seen); }
        }
        IrTypeDeclKind::Variant { cases, .. } => {
            for c in cases { collect_anon_from_variant_case(&c.kind, named, seen); }
        }
        _ => {}
    }
}

/// `IrVariantKind` case of `collect_anon_from_type_decl`'s `Variant` arm,
/// extracted verbatim (cog>30 decomposition).
fn collect_anon_from_variant_case(kind: &IrVariantKind, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    match kind {
        IrVariantKind::Unit => {}
        IrVariantKind::Tuple { fields } => {
            for t in fields { collect_anon_from_ty(t, named, seen); }
        }
        IrVariantKind::Record { fields } => {
            for f in fields { collect_anon_from_ty(&f.ty, named, seen); }
        }
    }
}

fn collect_anon_from_expr(expr: &IrExpr, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    collect_anon_from_ty(&expr.ty, named, seen);
    match &expr.kind {
        IrExprKind::Block { .. } => collect_anon_from_block(expr, named, seen),
        IrExprKind::If { cond, then, else_ } => {
            collect_anon_from_expr(cond, named, seen);
            collect_anon_from_expr(then, named, seen);
            collect_anon_from_expr(else_, named, seen);
        }
        IrExprKind::Match { .. } => collect_anon_from_match(expr, named, seen),
        IrExprKind::Call { .. } => collect_anon_from_call(expr, named, seen),
        IrExprKind::BinOp { left, right, .. } => {
            collect_anon_from_expr(left, named, seen);
            collect_anon_from_expr(right, named, seen);
        }
        IrExprKind::UnOp { operand, .. } => collect_anon_from_expr(operand, named, seen),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::RustMacro { args: elements, .. } => {
            for e in elements { collect_anon_from_expr(e, named, seen); }
        }
        IrExprKind::Lambda { body, .. } => collect_anon_from_expr(body, named, seen),
        IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => {
            for (_, v) in fields { collect_anon_from_expr(v, named, seen); }
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            collect_anon_from_expr(object, named, seen);
        }
        IrExprKind::ForIn { iterable, body, .. } => collect_anon_from_loop_body(iterable, body, named, seen),
        IrExprKind::While { cond, body } => collect_anon_from_loop_body(cond, body, named, seen),
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::OptionalChain { expr, .. } => {
            collect_anon_from_expr(expr, named, seen);
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            collect_anon_from_expr(expr, named, seen);
            collect_anon_from_expr(fallback, named, seen);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p { collect_anon_from_expr(expr, named, seen); }
            }
        }
        // Codegen-specific nodes
        IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            collect_anon_from_expr(expr, named, seen);
        }
        _ => {}
    }
}

/// `IrExprKind::Block` case of `collect_anon_from_expr`, extracted verbatim
/// (cog>30 decomposition).
fn collect_anon_from_block(expr: &IrExpr, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    let IrExprKind::Block { stmts, expr: e } = &expr.kind else { unreachable!() };
    for s in stmts { collect_anon_from_stmt(s, named, seen); }
    if let Some(e) = e { collect_anon_from_expr(e, named, seen); }
}

/// `IrExprKind::Match` case of `collect_anon_from_expr`, extracted verbatim
/// (cog>30 decomposition).
fn collect_anon_from_match(expr: &IrExpr, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    let IrExprKind::Match { subject, arms } = &expr.kind else { unreachable!() };
    collect_anon_from_expr(subject, named, seen);
    for arm in arms { collect_anon_from_expr(&arm.body, named, seen); }
}

/// `IrExprKind::Call` case of `collect_anon_from_expr`, extracted verbatim
/// (cog>30 decomposition).
fn collect_anon_from_call(expr: &IrExpr, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    let IrExprKind::Call { args, target, .. } = &expr.kind else { unreachable!() };
    if let CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } = target {
        collect_anon_from_expr(object, named, seen);
    }
    for a in args { collect_anon_from_expr(a, named, seen); }
}

/// `IrExprKind::ForIn` / `IrExprKind::While` case of `collect_anon_from_expr`,
/// extracted verbatim — both arms shared the identical "recurse the head
/// expr, then walk body stmts" shape (`iterable`/`cond` as the head), so
/// they now share one helper instead of two copies.
fn collect_anon_from_loop_body(head: &IrExpr, body: &[IrStmt], named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    collect_anon_from_expr(head, named, seen);
    for s in body { collect_anon_from_stmt(s, named, seen); }
}

fn collect_anon_from_stmt(stmt: &IrStmt, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, ty, .. } => {
            collect_anon_from_ty(ty, named, seen);
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            collect_anon_from_expr(index, named, seen);
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::Guard { cond, else_ } => {
            collect_anon_from_expr(cond, named, seen);
            collect_anon_from_expr(else_, named, seen);
        }
        IrStmtKind::Expr { expr } => collect_anon_from_expr(expr, named, seen),
        _ => {}
    }
}

/// Does this type transitively hold a value that doesn't implement `PartialEq`
/// in the Rust runtime? Returning true means the enclosing struct cannot
/// derive PartialEq.
///
/// `eq_blocked_types` lists user-defined record names that have already
/// been determined to lack PartialEq (computed in a first pass); referring
/// to any of those transitively blocks eq as well.
pub(super) fn ty_blocks_eq(ty: &Ty) -> bool {
    ty_blocks_eq_with(ty, &HashSet::new())
}

/// True if `ty` mentions a function type anywhere (directly or nested in a
/// container/tuple/record). Such a type lowers to `Rc<dyn Fn>` (or a container
/// thereof), which is neither `Debug` nor `PartialEq`.
fn ty_has_fn(ty: &Ty) -> bool {
    matches!(ty, Ty::Fn { .. }) || ty.children().iter().any(|c| ty_has_fn(c))
}

pub(super) fn ty_blocks_eq_with(ty: &Ty, eq_blocked_types: &HashSet<String>) -> bool {
    match ty {
        // Burn Tensor doesn't implement PartialEq
        Ty::Matrix => true,
        // Function pointers can't be compared structurally
        Ty::Fn { .. } => true,
        // User-defined Named type: check against the precomputed blocked set
        Ty::Named(name, args) => {
            eq_blocked_types.contains(name.as_str())
                || args.iter().any(|t| ty_blocks_eq_with(t, eq_blocked_types))
        }
        // Structural recursion for containers
        Ty::Tuple(elems) => elems.iter().any(|t| ty_blocks_eq_with(t, eq_blocked_types)),
        Ty::Applied(_, args) => args.iter().any(|t| ty_blocks_eq_with(t, eq_blocked_types)),
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            fields.iter().any(|(_, t)| ty_blocks_eq_with(t, eq_blocked_types))
        }
        _ => false,
    }
}

/// Precompute the set of user-defined type names whose generated Rust struct
/// cannot derive `PartialEq` (because a field type blocks it, transitively).
/// Reaches a fixed point by repeating until no new blocked type is added.
pub(super) fn compute_eq_blocked_types(type_decls: &[IrTypeDecl]) -> HashSet<String> {
    let mut blocked: HashSet<String> = HashSet::new();
    loop {
        let mut changed = false;
        for td in type_decls {
            if blocked.contains(td.name.as_str()) { continue }
            let blocks = match &td.kind {
                IrTypeDeclKind::Record { fields } => {
                    fields.iter().any(|f| ty_blocks_eq_with(&f.ty, &blocked))
                }
                IrTypeDeclKind::Variant { cases, .. } => {
                    cases.iter().any(|c| match &c.kind {
                        IrVariantKind::Unit => false,
                        IrVariantKind::Tuple { fields } => {
                            fields.iter().any(|t| ty_blocks_eq_with(t, &blocked))
                        }
                        IrVariantKind::Record { fields } => {
                            fields.iter().any(|f| ty_blocks_eq_with(&f.ty, &blocked))
                        }
                    })
                }
                _ => false,
            };
            if blocks {
                blocked.insert(td.name.to_string());
                changed = true;
            }
        }
        if !changed { break }
    }
    blocked
}

/// True when `name` (a generic param) appears anywhere in `ty`, as either a
/// `TypeVar` or a `Named` reference (lowering may leave either spelling).
fn ty_mentions_param(ty: &Ty, name: &str) -> bool {
    match ty {
        Ty::TypeVar(n) => n.as_str() == name,
        Ty::Named(n, args) => n.as_str() == name || args.iter().any(|a| ty_mentions_param(a, name)),
        Ty::Applied(_, args) => args.iter().any(|a| ty_mentions_param(a, name)),
        Ty::Tuple(elems) => elems.iter().any(|e| ty_mentions_param(e, name)),
        Ty::Fn { params, ret } => params.iter().any(|p| ty_mentions_param(p, name)) || ty_mentions_param(ret, name),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| ty_mentions_param(t, name)),
        _ => false,
    }
}

/// Record types declaring generic params NONE of which are referenced by any
/// field — Rust's `error[E0392]` rejects such an unused param, so we strip the
/// generics from the emitted struct and from every reference to it (#621).
/// Only fully-phantom records qualify: a partly-used param list still needs its
/// (used) params, so it is left alone here.
pub(super) fn compute_phantom_param_structs(type_decls: &[IrTypeDecl]) -> HashSet<String> {
    let mut out = HashSet::new();
    for td in type_decls {
        let Some(generics) = &td.generics else { continue };
        if generics.is_empty() { continue }
        let IrTypeDeclKind::Record { fields } = &td.kind else { continue };
        let any_used = generics.iter().any(|g| {
            let pname = g.name.as_str();
            fields.iter().any(|f| ty_mentions_param(&f.ty, pname))
        });
        if !any_used {
            out.insert(td.name.to_string());
        }
    }
    out
}

thread_local! {
    /// Sorted field-name keys of anonymous records that have a closure (`Fn`) field.
    /// Their generated struct must derive `Clone` only (a closure is not `Debug` /
    /// `PartialEq`) — the same `has_fn_fields` relaxation a `type`-declared record
    /// gets. Populated during `collect_anon_records`; read back into
    /// `anon_records_with_fn`. Membership-only (never iterated for output), so it
    /// does not affect host-deterministic emit. (Closure codegen cross-target gaps.)
    static ANON_FN_KEYS: std::cell::RefCell<HashSet<Vec<String>>> =
        std::cell::RefCell::new(HashSet::new());
}

fn collect_anon_from_ty(ty: &Ty, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    // Record/OpenRecord: register anonymous record fields
    if let Ty::Record { fields } | Ty::OpenRecord { fields } = ty {
        let mut names: Vec<String> = fields.iter().map(|(n, _)| n.to_string()).collect();
        names.sort();
        if !named.contains(&names) {
            // A field whose type CONTAINS a closure anywhere (`Fn`, `List[Fn]`,
            // `Map[_, Fn]`, `(Fn, _)`, …) lowers to a type that is neither `Debug`
            // nor `PartialEq`, so the generated struct must derive `Clone` only.
            // Matching `Ty::Fn` alone missed boxed-closure containers.
            if fields.iter().any(|(_, t)| ty_has_fn(t)) {
                ANON_FN_KEYS.with(|s| { s.borrow_mut().insert(names.clone()); });
            }
            seen.insert(names);
        }
    }
    // Recurse into all children uniformly
    for child in ty.children() {
        collect_anon_from_ty(child, named, seen);
    }
}
